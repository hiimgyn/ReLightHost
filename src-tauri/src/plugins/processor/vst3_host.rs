//! Host-side VST3 objects the plugin calls into: the component handler
//! (GUI edits → host) and the `IParameterChanges` handed to `process()`
//! (host → audio processor).


use std::cell::Cell;
use std::sync::Arc;

use parking_lot::Mutex;
use vst3::Steinberg::Vst::{
    IComponentHandler, IComponentHandlerTrait, IParamValueQueue, IParamValueQueueTrait,
    IParameterChanges, IParameterChangesTrait, ParamID, ParamValue,
};
use vst3::Steinberg::{int32, kInvalidArgument, kResultFalse, kResultOk, tresult};
use vst3::{Class, ComPtr, ComWrapper};

/// Most distinct parameters delivered to `process()` in one block; the rest
/// wait for the next block. Pre-allocated so the audio thread never allocates.
pub const CAPACITY: usize = 64;

/// Normalized parameter edits waiting for the audio thread: pushed by the
/// plugin's GUI (via `performEdit`) or by the host, drained once per block.
#[derive(Clone, Default)]
pub struct PendingParams(Arc<Mutex<Vec<(u32, f64)>>>);

impl PendingParams {
    pub fn push(&self, id: u32, value: f64) {
        self.0.lock().push((id, value));
    }

    /// Audio-thread side: never blocks — a contended lock just leaves the
    /// edits for the next block. Moves them into `out` (reusing capacity).
    pub fn try_drain_into(&self, out: &mut Vec<(u32, f64)>) {
        if let Some(mut pending) = self.0.try_lock() {
            out.append(&mut pending);
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

    unsafe fn endEdit(&self, _id: ParamID) -> tresult {
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
    fn changes_beyond_capacity_are_kept_for_the_next_block() {
        let changes = HostParameterChanges::new();
        let many: Vec<(u32, f64)> = (0..(CAPACITY as u32 + 3)).map(|i| (i, 0.5)).collect();
        let mut overflow = Vec::new();
        changes.load(&many, &mut overflow);
        assert_eq!(overflow.len(), 3);
        assert_eq!(unsafe { changes.com_ptr().getParameterCount() }, CAPACITY as i32);
    }
}
