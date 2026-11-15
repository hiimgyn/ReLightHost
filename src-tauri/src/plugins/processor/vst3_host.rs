//! Host-side VST3 objects the plugin calls into: the host context
//! (`IHostApplication`), the component handler (GUI edits → host) and the
//! `IParameterChanges` handed to `process()` (host → audio processor).


use std::cell::Cell;
use std::collections::HashMap;
use std::ffi::{c_void, CStr, CString};
use std::sync::{Arc, OnceLock};

use parking_lot::Mutex;
use vst3::Steinberg::Vst::IAttributeList_::AttrID;
use vst3::Steinberg::Vst::{
    IAttributeList, IAttributeListTrait, IComponentHandler, IComponentHandlerTrait,
    IHostApplication, IHostApplicationTrait, IMessage, IMessageTrait, IParamValueQueue,
    IParamValueQueueTrait, IParameterChanges, IParameterChangesTrait, ParamID, ParamValue,
    String128, TChar,
};
use vst3::Steinberg::{
    int32, int64, kInvalidArgument, kNoInterface, kResultFalse, kResultOk, tresult, uint32,
    FIDString, FUnknown, TUID,
};
use vst3::{Class, ComPtr, ComWrapper, Interface};

/// The process-wide host context passed to `initialize()` and
/// `IPluginFactory3::setHostContext`. Created once and leaked: plugins may
/// keep it for their whole life.
pub fn host_context() -> *mut FUnknown {
    static HOST: OnceLock<usize> = OnceLock::new();
    *HOST.get_or_init(|| {
        ComWrapper::new(HostApplication)
            .to_com_ptr::<IHostApplication>()
            .expect("IHostApplication")
            .into_raw() as usize
    }) as *mut FUnknown
}

/// Minimal `IHostApplication`: a name, and the `IMessage`/`IAttributeList`
/// objects SDK plugins get from `allocateMessage()` to talk between their
/// component and controller.
struct HostApplication;

impl Class for HostApplication {
    type Interfaces = (IHostApplication,);
}

#[allow(non_snake_case)]
impl IHostApplicationTrait for HostApplication {
    unsafe fn getName(&self, name: *mut String128) -> tresult {
        let Some(name) = name.as_mut() else { return kInvalidArgument };
        name.fill(0);
        for (d, c) in name.iter_mut().zip("ReLightHost".encode_utf16()) {
            *d = c;
        }
        kResultOk
    }

    unsafe fn createInstance(&self, cid: *mut TUID, iid: *mut TUID, obj: *mut *mut c_void) -> tresult {
        let Some(obj) = obj.as_mut() else { return kInvalidArgument };
        *obj = std::ptr::null_mut();
        let (Some(cid), Some(iid)) = (cid.as_ref(), iid.as_ref()) else { return kInvalidArgument };
        let is = |want: &[u8; 16]| cid.map(|b| b as u8) == *want && iid.map(|b| b as u8) == *want;
        let made = if is(&IMessage::IID) {
            let msg = HostMessage {
                id: Mutex::new(None),
                attributes: ComWrapper::new(HostAttributeList::default()),
            };
            ComWrapper::new(msg).to_com_ptr::<IMessage>().map(|p| p.into_raw() as *mut c_void)
        } else if is(&IAttributeList::IID) {
            ComWrapper::new(HostAttributeList::default()).to_com_ptr::<IAttributeList>().map(|p| p.into_raw() as *mut c_void)
        } else {
            None
        };
        match made {
            Some(p) => {
                *obj = p;
                kResultOk
            }
            None => kNoInterface,
        }
    }
}

struct HostMessage {
    id: Mutex<Option<CString>>,
    attributes: ComWrapper<HostAttributeList>,
}

impl Class for HostMessage {
    type Interfaces = (IMessage,);
}

#[allow(non_snake_case)]
impl IMessageTrait for HostMessage {
    unsafe fn getMessageID(&self) -> FIDString {
        self.id.lock().as_ref().map_or(std::ptr::null(), |s| s.as_ptr())
    }

    unsafe fn setMessageID(&self, id: FIDString) {
        *self.id.lock() = (!id.is_null()).then(|| CStr::from_ptr(id).to_owned());
    }

    /// Borrowed (not AddRef'd), as in the SDK's HostMessage.
    unsafe fn getAttributes(&self) -> *mut IAttributeList {
        self.attributes.as_com_ref::<IAttributeList>().map_or(std::ptr::null_mut(), |r| r.as_ptr())
    }
}

enum Attr {
    Int(i64),
    Float(f64),
    /// UTF-16 including the terminating 0.
    Str(Vec<TChar>),
    Bin(Vec<u8>),
}

#[derive(Default)]
struct HostAttributeList(Mutex<HashMap<CString, Attr>>);

impl HostAttributeList {
    unsafe fn set(&self, id: AttrID, value: Attr) -> tresult {
        if id.is_null() {
            return kInvalidArgument;
        }
        self.0.lock().insert(CStr::from_ptr(id).to_owned(), value);
        kResultOk
    }

    /// Runs `read` on the attribute; kResultFalse if absent or the wrong type.
    unsafe fn get(&self, id: AttrID, read: impl FnOnce(&Attr) -> bool) -> tresult {
        if id.is_null() {
            return kInvalidArgument;
        }
        match self.0.lock().get(CStr::from_ptr(id)) {
            Some(a) if read(a) => kResultOk,
            _ => kResultFalse,
        }
    }
}

impl Class for HostAttributeList {
    type Interfaces = (IAttributeList,);
}

#[allow(non_snake_case)]
impl IAttributeListTrait for HostAttributeList {
    unsafe fn setInt(&self, id: AttrID, value: int64) -> tresult {
        self.set(id, Attr::Int(value))
    }

    unsafe fn getInt(&self, id: AttrID, value: *mut int64) -> tresult {
        self.get(id, |a| match (a, value.as_mut()) {
            (Attr::Int(v), Some(out)) => { *out = *v; true }
            _ => false,
        })
    }

    unsafe fn setFloat(&self, id: AttrID, value: f64) -> tresult {
        self.set(id, Attr::Float(value))
    }

    unsafe fn getFloat(&self, id: AttrID, value: *mut f64) -> tresult {
        self.get(id, |a| match (a, value.as_mut()) {
            (Attr::Float(v), Some(out)) => { *out = *v; true }
            _ => false,
        })
    }

    unsafe fn setString(&self, id: AttrID, string: *const TChar) -> tresult {
        if string.is_null() {
            return kInvalidArgument;
        }
        let mut len = 0;
        while *string.add(len) != 0 {
            len += 1;
        }
        self.set(id, Attr::Str(std::slice::from_raw_parts(string, len + 1).to_vec()))
    }

    unsafe fn getString(&self, id: AttrID, string: *mut TChar, sizeInBytes: uint32) -> tresult {
        let cap = sizeInBytes as usize / std::mem::size_of::<TChar>();
        self.get(id, |a| match a {
            Attr::Str(s) if !string.is_null() && cap > 0 => {
                let n = s.len().min(cap);
                std::ptr::copy_nonoverlapping(s.as_ptr(), string, n);
                *string.add(n - 1) = 0; // terminated even when truncated
                true
            }
            _ => false,
        })
    }

    unsafe fn setBinary(&self, id: AttrID, data: *const c_void, sizeInBytes: uint32) -> tresult {
        let bytes = if data.is_null() {
            Vec::new()
        } else {
            std::slice::from_raw_parts(data as *const u8, sizeInBytes as usize).to_vec()
        };
        self.set(id, Attr::Bin(bytes))
    }

    /// The pointer stays valid until the attribute is replaced or the list
    /// is released (as in the SDK).
    unsafe fn getBinary(&self, id: AttrID, data: *mut *const c_void, sizeInBytes: *mut uint32) -> tresult {
        self.get(id, |a| match (a, data.as_mut(), sizeInBytes.as_mut()) {
            (Attr::Bin(b), Some(d), Some(n)) => {
                *d = b.as_ptr() as *const c_void;
                *n = b.len() as uint32;
                true
            }
            _ => false,
        })
    }
}

/// Most distinct parameters delivered to `process()` in one block; the rest
/// wait for the next block. Pre-allocated so the audio thread never allocates.
pub const CAPACITY: usize = 64;
/// Most distinct parameters waiting in [`PendingParams`]; also the size the
/// processor pre-allocates its per-block scratch with.
pub const PENDING_CAPACITY: usize = 256;

/// Normalized parameter edits waiting for the audio thread: pushed by the
/// plugin's GUI (via `performEdit`) or by the host, drained once per block.
/// One entry per parameter id (a newer value replaces an older one, keeping
/// its place), at most [`PENDING_CAPACITY`] of them (the oldest goes).
#[derive(Clone, Default)]
pub struct PendingParams(Arc<Mutex<Vec<(u32, f64)>>>);

impl PendingParams {
    pub fn push(&self, id: u32, value: f64) {
        let mut pending = self.0.lock();
        if let Some(e) = pending.iter_mut().find(|e| e.0 == id) {
            e.1 = value;
            return;
        }
        if pending.len() >= PENDING_CAPACITY {
            pending.remove(0);
        }
        pending.push((id, value));
    }

    /// Audio-thread side: never blocks — a contended lock just leaves the
    /// edits for the next block — and never allocates: moves only as many
    /// (oldest first) as fit `out`'s spare capacity; the rest wait.
    pub fn try_drain_into(&self, out: &mut Vec<(u32, f64)>) {
        if let Some(mut pending) = self.0.try_lock() {
            let n = pending.len().min(out.capacity() - out.len());
            out.extend(pending.drain(..n));
        }
    }

    #[cfg(test)]
    pub fn take_all(&self) -> Vec<(u32, f64)> {
        std::mem::take(&mut *self.0.lock())
    }
}

/// `IComponentHandler` given to the edit controller. Per the VST3 spec a
/// plugin with a separate controller reports GUI edits *only* here, and its
/// processor learns them *only* through `ProcessData::inputParameterChanges`
/// — without this, turning a knob in such a plugin's editor never reached
/// the audio.
pub struct HostComponentHandler {
    pending: PendingParams,
}

impl HostComponentHandler {
    pub fn new(pending: PendingParams) -> Self {
        Self { pending }
    }
}

impl Class for HostComponentHandler {
    type Interfaces = (IComponentHandler,);
}

#[allow(non_snake_case)]
impl IComponentHandlerTrait for HostComponentHandler {
    unsafe fn beginEdit(&self, _id: ParamID) -> tresult {
        kResultOk
    }

    unsafe fn performEdit(&self, id: ParamID, valueNormalized: ParamValue) -> tresult {
        self.pending.push(id, valueNormalized);
        kResultOk
    }

    /// An edit in the plugin's own editor finished: autosave (debounced,
    /// so a burst of edits is one save).
    unsafe fn endEdit(&self, _id: ParamID) -> tresult {
        crate::core::autosave::request_plugin_chain_autosave();
        kResultOk
    }

    unsafe fn restartComponent(&self, flags: int32) -> tresult {
        log::debug!("VST3 plugin requested restartComponent(flags={flags:#x}); not supported");
        kResultOk
    }
}

/// One parameter's single change point for the current block.
struct HostParamValueQueue {
    id: Cell<u32>,
    value: Cell<f64>,
}

// Only touched on the audio thread, inside `process()`.
unsafe impl Send for HostParamValueQueue {}
unsafe impl Sync for HostParamValueQueue {}

impl Class for HostParamValueQueue {
    type Interfaces = (IParamValueQueue,);
}

#[allow(non_snake_case)]
impl IParamValueQueueTrait for HostParamValueQueue {
    unsafe fn getParameterId(&self) -> ParamID {
        self.id.get()
    }

    unsafe fn getPointCount(&self) -> int32 {
        1
    }

    unsafe fn getPoint(&self, index: int32, sampleOffset: *mut int32, value: *mut ParamValue) -> tresult {
        if index != 0 || sampleOffset.is_null() || value.is_null() {
            return kInvalidArgument;
        }
        *sampleOffset = 0;
        *value = self.value.get();
        kResultOk
    }

    unsafe fn addPoint(&self, _sampleOffset: int32, _value: ParamValue, _index: *mut int32) -> tresult {
        kResultFalse // input changes are read-only for the plugin
    }
}

struct ParamChangesImpl {
    queues: Vec<ComWrapper<HostParamValueQueue>>,
    queue_ptrs: Vec<ComPtr<IParamValueQueue>>,
    count: Cell<usize>,
}

unsafe impl Send for ParamChangesImpl {}
unsafe impl Sync for ParamChangesImpl {}

impl Class for ParamChangesImpl {
    type Interfaces = (IParameterChanges,);
}

#[allow(non_snake_case)]
impl IParameterChangesTrait for ParamChangesImpl {
    unsafe fn getParameterCount(&self) -> int32 {
        self.count.get() as int32
    }

    unsafe fn getParameterData(&self, index: int32) -> *mut IParamValueQueue {
        match usize::try_from(index) {
            Ok(i) if i < self.count.get() => self.queue_ptrs[i].as_ptr(),
            _ => std::ptr::null_mut(),
        }
    }

    unsafe fn addParameterData(&self, _id: *const ParamID, _index: *mut int32) -> *mut IParamValueQueue {
        std::ptr::null_mut()
    }
}

/// Pre-allocated `IParameterChanges` for `ProcessData::inputParameterChanges`.
pub struct HostParameterChanges {
    inner: ComWrapper<ParamChangesImpl>,
    ptr: ComPtr<IParameterChanges>,
}

// The COM objects are only read by the plugin inside `process()` on the
// audio thread; `load`/`clear` run on that same thread just before/after.
unsafe impl Send for HostParameterChanges {}

impl HostParameterChanges {
    pub fn new() -> Self {
        let queues: Vec<_> = (0..CAPACITY)
            .map(|_| ComWrapper::new(HostParamValueQueue { id: Cell::new(0), value: Cell::new(0.0) }))
            .collect();
        let queue_ptrs = queues
            .iter()
            .map(|q| q.to_com_ptr::<IParamValueQueue>().expect("IParamValueQueue"))
            .collect();
        let inner = ComWrapper::new(ParamChangesImpl { queues, queue_ptrs, count: Cell::new(0) });
        let ptr = inner.to_com_ptr::<IParameterChanges>().expect("IParameterChanges");
        Self { inner, ptr }
    }

    /// Loads `changes` (later entries win per id). Distinct ids beyond
    /// [`CAPACITY`] are appended to `overflow` for the next block.
    pub fn load(&self, changes: &[(u32, f64)], overflow: &mut Vec<(u32, f64)>) {
        let q = &self.inner.queues;
        let mut count = self.inner.count.get();
        for &(id, value) in changes {
            if let Some(existing) = q[..count].iter().find(|e| e.id.get() == id) {
                existing.value.set(value);
            } else if count < CAPACITY {
                q[count].id.set(id);
                q[count].value.set(value);
                count += 1;
            } else {
                overflow.push((id, value));
            }
        }
        self.inner.count.set(count);
    }

    pub fn is_empty(&self) -> bool {
        self.inner.count.get() == 0
    }

    pub fn clear(&self) {
        self.inner.count.set(0);
    }

    pub fn com_ptr(&self) -> ComPtr<IParameterChanges> {
        self.ptr.clone()
    }

    /// Raw pointer for `ProcessData` (valid while `self` lives).
    pub fn as_ptr(&self) -> *mut IParameterChanges {
        self.ptr.as_ptr()
    }
}

impl Default for HostParameterChanges {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vst3::Steinberg::Vst::{
        IComponentHandler, IComponentHandlerTrait, IParamValueQueue, IParamValueQueueTrait,
        IParameterChanges, IParameterChangesTrait,
    };
    use vst3::{ComPtr, ComRef, ComWrapper};

    #[test]
    fn performed_edits_reach_the_pending_queue() {
        let pending = PendingParams::default();
        let handler = ComWrapper::new(HostComponentHandler::new(pending.clone()));
        let h: ComPtr<IComponentHandler> = handler.to_com_ptr().unwrap();
        unsafe {
            h.beginEdit(7);
            h.performEdit(7, 0.25);
            h.endEdit(7);
        }
        assert_eq!(pending.take_all(), vec![(7, 0.25)]);
    }

    #[test]
    fn host_context_makes_messages_with_working_attribute_lists() {
        unsafe {
            let host = ComRef::<IHostApplication>::from_raw(host_context() as *mut IHostApplication).unwrap();
            let mut name = [0u16; 128];
            assert_eq!(host.getName(&mut name), kResultOk);
            assert_eq!(String::from_utf16_lossy(&name[..11]), "ReLightHost");

            let mut iid = IMessage::IID.map(|b| b as i8);
            let mut cid = iid;
            let mut obj = std::ptr::null_mut();
            assert_eq!(host.createInstance(&mut cid, &mut iid, &mut obj), kResultOk);
            let msg = ComPtr::<IMessage>::from_raw(obj as *mut IMessage).unwrap();
            msg.setMessageID(c"Ping".as_ptr());
            assert_eq!(CStr::from_ptr(msg.getMessageID()), c"Ping");

            let attrs = ComRef::<IAttributeList>::from_raw(msg.getAttributes()).unwrap();
            attrs.setInt(c"n".as_ptr(), 42);
            let mut n = 0;
            assert_eq!(attrs.getInt(c"n".as_ptr(), &mut n), kResultOk);
            assert_eq!(n, 42);
            let hello: Vec<u16> = "hello\0".encode_utf16().collect();
            attrs.setString(c"s".as_ptr(), hello.as_ptr());
            let mut short = [7u16; 3];
            attrs.getString(c"s".as_ptr(), short.as_mut_ptr(), 6);
            assert_eq!(short, [b'h' as u16, b'e' as u16, 0], "truncated but terminated");
            assert_eq!(attrs.getInt(c"s".as_ptr(), &mut n), kResultFalse, "wrong type");

            let mut other = [0i8; 16];
            let mut other_iid = other;
            assert_eq!(host.createInstance(&mut other, &mut other_iid, &mut obj), kNoInterface);
            assert!(obj.is_null());
        }
    }

    #[test]
    fn parameter_changes_expose_the_latest_value_per_id_as_one_point() {
        let changes = HostParameterChanges::new();
        let mut overflow = Vec::new();
        changes.load(&[(1, 0.5), (2, 0.25), (1, 0.75)], &mut overflow);
        assert!(overflow.is_empty());
        let ptr: ComPtr<IParameterChanges> = changes.com_ptr();
        unsafe {
            assert_eq!(ptr.getParameterCount(), 2);
            // getParameterData hands out a borrowed (non-AddRef'd) pointer.
            let q = ComRef::<IParamValueQueue>::from_raw(ptr.getParameterData(0)).unwrap();
            assert_eq!(q.getParameterId(), 1);
            assert_eq!(q.getPointCount(), 1);
            let (mut offset, mut value) = (-1, 0.0);
            q.getPoint(0, &mut offset, &mut value);
            assert_eq!((offset, value), (0, 0.75));
            assert!(ptr.getParameterData(2).is_null());
        }
        changes.clear();
        assert_eq!(unsafe { changes.com_ptr().getParameterCount() }, 0);
    }

    #[test]
    fn pending_edits_coalesce_per_id_and_stay_bounded() {
        let pending = PendingParams::default();
        for i in 0..1000 {
            pending.push(7, i as f64);
        }
        assert_eq!(pending.take_all(), vec![(7, 999.0)], "one entry per id, latest value");
        for id in 0..(PENDING_CAPACITY as u32 + 10) {
            pending.push(id, 0.5);
        }
        let all = pending.take_all();
        assert_eq!(all.len(), PENDING_CAPACITY);
        assert_eq!(all.last(), Some(&(PENDING_CAPACITY as u32 + 9, 0.5)), "oldest dropped, newest kept");
    }

    #[test]
    fn draining_never_grows_the_audio_thread_buffer() {
        let pending = PendingParams::default();
        for id in 0..20 {
            pending.push(id, 0.5);
        }
        let mut out = Vec::with_capacity(8);
        out.push((99, 0.0));
        pending.try_drain_into(&mut out);
        assert_eq!((out.len(), out.capacity()), (8, 8), "no reallocation on the audio thread");
        assert_eq!(out[1], (0, 0.5), "oldest first");
        out.clear();
        pending.try_drain_into(&mut out);
        assert_eq!(out[0], (7, 0.5), "the rest waits for the next block");
    }

    #[test]
    fn changes_beyond_capacity_are_kept_for_the_next_block() {
        let changes = HostParameterChanges::new();
        let many: Vec<(u32, f64)> = (0..(CAPACITY as u32 + 3)).map(|i| (i, 0.5)).collect();
        let mut overflow = Vec::new();
        changes.load(&many, &mut overflow);
        assert_eq!(overflow.len(), 3);
        assert_eq!(unsafe { changes.com_ptr().getParameterCount() }, CAPACITY as i32);
    }
}
