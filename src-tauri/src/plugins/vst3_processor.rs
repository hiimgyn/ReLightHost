//! VST3 audio processor  uses the `vst3` crate (ComPtr / ComWrapper / Interface).

#[cfg(target_os = "windows")]
mod win {
    use anyhow::{anyhow, Result};
    use libloading::{Library, Symbol};
    use std::ffi::c_void;
    use std::ptr;

    use vst3::Steinberg::{
        kResultOk, IBStream, IBStreamTrait, IPluginFactory, IPluginFactoryTrait,
        IPluginBaseTrait, PClassInfo,
    };
    use vst3::Steinberg::Vst::{
        BusDirections_::{kInput, kOutput},
        IAudioProcessor, IAudioProcessorTrait,
        IComponent, IComponentTrait,
        MediaTypes_::kAudio,
        ProcessModes_::kRealtime,
        ProcessSetup,
        SymbolicSampleSizes_::kSample32,
    };
    use vst3::{Class, ComPtr, ComWrapper, Interface};

    //  IBStream implementation 
    // IBStreamTrait requires &self (not &mut self), so we use Cell/RefCell for
    // interior mutability.
    struct RustIBStream {
        buf: std::cell::RefCell<Vec<u8>>,
        pos: std::cell::Cell<usize>,
    }

    impl RustIBStream {
        fn write_stream() -> Self {
            Self { buf: std::cell::RefCell::new(Vec::new()), pos: std::cell::Cell::new(0) }
        }
        fn read_stream(data: Vec<u8>) -> Self {
            Self { buf: std::cell::RefCell::new(data), pos: std::cell::Cell::new(0) }
        }
    }

    impl Class for RustIBStream {
        type Interfaces = (IBStream,);
    }

    #[allow(non_snake_case)]
    impl IBStreamTrait for RustIBStream {
        unsafe fn read(&self, buffer: *mut c_void, numBytes: i32, numBytesRead: *mut i32) -> i32 {
            let borrow = self.buf.borrow();
            let n = numBytes.max(0) as usize;
            let avail = borrow.len().saturating_sub(self.pos.get());
            let to_read = n.min(avail);
            if to_read > 0 {
                ptr::copy_nonoverlapping(
                    borrow[self.pos.get()..].as_ptr(),
                    buffer as *mut u8,
                    to_read,
                );
                self.pos.set(self.pos.get() + to_read);
            }
            if !numBytesRead.is_null() { *numBytesRead = to_read as i32; }
            0
        }

        unsafe fn write(&self, buffer: *mut c_void, numBytes: i32, numBytesWritten: *mut i32) -> i32 {
            let mut borrow = self.buf.borrow_mut();
            let n = numBytes.max(0) as usize;
            let pos = self.pos.get();
            let end = pos + n;
            if end > borrow.len() { borrow.resize(end, 0); }
            ptr::copy_nonoverlapping(buffer as *const u8, borrow[pos..end].as_mut_ptr(), n);
            self.pos.set(end);
            if !numBytesWritten.is_null() { *numBytesWritten = n as i32; }
            0
        }

        unsafe fn seek(&self, pos: i64, mode: i32, result: *mut i64) -> i32 {
            let len = self.buf.borrow().len();
            let new_pos = match mode {
                0 => pos.max(0) as usize,               // kIBSeekSet
                1 => (self.pos.get() as i64 + pos).max(0) as usize, // kIBSeekCur
                2 => (len as i64 + pos).max(0) as usize, // kIBSeekEnd
                _ => return 0x80004005u32 as i32,
            };
            self.pos.set(new_pos);
            if !result.is_null() { *result = new_pos as i64; }
            0
        }

        unsafe fn tell(&self, pos: *mut i64) -> i32 {
            if !pos.is_null() { *pos = self.pos.get() as i64; }
            0
        }
    }

    //  Layout-critical structs 
    // Keep hand-rolled for process()  must match SDK binary layout exactly.
    #[repr(C)]
    struct AudioBusBuffers {
        num_channels: i32,
        _pad: i32,
        silence_flags: u64,
        channel_buffers: *mut *mut f32,
    }

    #[repr(C)]
    struct ProcessData {
        process_mode: i32,
        symbolic_sample_size: i32,
        num_samples: i32,
        num_inputs: i32,
        num_outputs: i32,
        _pad: i32,
        inputs: *mut AudioBusBuffers,
        outputs: *mut AudioBusBuffers,
        input_param_changes: *mut c_void,
        output_param_changes: *mut c_void,
        input_events: *mut c_void,
        output_events: *mut c_void,
        process_context: *mut c_void,
    }

    //  Vst3Processor 
    pub struct Vst3Processor {
        _lib:       Library,
        component:  ComPtr<IComponent>,
        audio_proc: ComPtr<IAudioProcessor>,
        in_l:       Vec<f32>,
        in_r:       Vec<f32>,
    }

    // Safety: Vst3Processor owns the plugin instance; we never share it across
    // threads simultaneously.
    unsafe impl Send for Vst3Processor {}

    impl Vst3Processor {
        pub fn load(plugin_path: &str, sample_rate: f64, block_size: usize) -> Result<Self> {
            // Load DLL
            let lib = unsafe { Library::new(plugin_path) }
                .map_err(|e| anyhow!("Failed to load '{}': {}", plugin_path, e))?;

            // Optional InitDll (some plugins require it)
            type BoolFn = unsafe extern "system" fn() -> bool;
            if let Ok(init_dll) = unsafe { lib.get::<BoolFn>(b"InitDll\0") } {
                if !unsafe { init_dll() } {
                    log::warn!("InitDll() returned false for '{}'", plugin_path);
                }
            }

            // GetPluginFactory
            type GetPluginFactory = unsafe extern "system" fn() -> *mut IPluginFactory;
            let get_factory: Symbol<GetPluginFactory> = unsafe { lib.get(b"GetPluginFactory\0") }
                .map_err(|_| anyhow!("'{}' has no GetPluginFactory export", plugin_path))?;
            let factory_ptr = unsafe { get_factory() };
            if factory_ptr.is_null() {
                return Err(anyhow!("GetPluginFactory returned null for '{}'", plugin_path));
            }
            let factory = unsafe {
                ComPtr::<IPluginFactory>::from_raw(factory_ptr)
                    .ok_or_else(|| anyhow!("Failed to wrap IPluginFactory"))?
            };

            // Find the Audio Module Class CID
            let n = unsafe { factory.countClasses() };
            let mut audio_cid: Option<vst3::Steinberg::TUID> = None;
            for i in 0..n {
                let mut ci: PClassInfo = unsafe { std::mem::zeroed() };
                if unsafe { factory.getClassInfo(i, &mut ci) } == kResultOk {
                    let cat: &[u8] = unsafe {
                        std::slice::from_raw_parts(ci.category.as_ptr() as *const u8, ci.category.len())
                    };
                    if cat.starts_with(b"Audio Module Class") && audio_cid.is_none() {
                        audio_cid = Some(ci.cid);
                    }
                }
            }
            let cid = audio_cid
                .ok_or_else(|| anyhow!("'{}': no Audio Module Class found", plugin_path))?;

            // createInstance  IComponent (with FUnknown fallback)
            let mut component_ptr: *mut IComponent = ptr::null_mut();
            let result = unsafe {
                factory.createInstance(
                    cid.as_ptr() as *const i8,
                    IComponent::IID.as_ptr() as *const i8,
                    &mut component_ptr as *mut _ as *mut _,
                )
            };

            let component: ComPtr<IComponent> = if result == kResultOk && !component_ptr.is_null() {
                unsafe {
                    ComPtr::<IComponent>::from_raw(component_ptr)
                        .ok_or_else(|| anyhow!("Failed to wrap IComponent"))?
                }
            } else {
                // Fallback: createInstance with FUnknown IID then QueryInterface
                let mut raw_ptr: *mut vst3::Steinberg::FUnknown = ptr::null_mut();
                let r2 = unsafe {
                    factory.createInstance(
                        cid.as_ptr() as *const i8,
                        vst3::Steinberg::FUnknown::IID.as_ptr() as *const i8,
                        &mut raw_ptr as *mut _ as *mut _,
                    )
                };
                if r2 != kResultOk || raw_ptr.is_null() {
                    return Err(anyhow!(
                        "'{}': createInstance failed ({:#010x})",
                        plugin_path, result as u32
                    ));
                }
                let fu = unsafe {
                    ComPtr::<vst3::Steinberg::FUnknown>::from_raw(raw_ptr)
                        .ok_or_else(|| anyhow!("Failed to wrap FUnknown"))?
                };
                fu.cast::<IComponent>().ok_or_else(|| {
                    anyhow!("'{}': IComponent QueryInterface failed after FUnknown createInstance", plugin_path)
                })?
            };

            // Initialize, activate buses
            unsafe {
                component.initialize(ptr::null_mut());
                component.activateBus(kAudio as i32, kInput as i32, 0, 1);
                component.activateBus(kAudio as i32, kOutput as i32, 0, 1);
            }

            // QueryInterface  IAudioProcessor
            let audio_proc = component.cast::<IAudioProcessor>().ok_or_else(|| {
                anyhow!("'{}' does not implement IAudioProcessor", plugin_path)
            })?;

            // Bus arrangements (stereo = 0x03)
            let stereo_mask: u64 = 0x03;
            unsafe {
                audio_proc.setBusArrangements(
                    &stereo_mask as *const u64 as *mut u64, 1,
                    &stereo_mask as *const u64 as *mut u64, 1,
                );
            }

            // Setup processing
            let mut setup = ProcessSetup {
                processMode:        kRealtime as i32,
                symbolicSampleSize: kSample32 as i32,
                maxSamplesPerBlock: block_size as i32,
                sampleRate:         sample_rate,
            };
            unsafe { audio_proc.setupProcessing(&mut setup) };
            unsafe {
                component.setActive(1);
                audio_proc.setProcessing(1);
            }

            log::info!("VST3 plugin loaded: '{}'", plugin_path);
            Ok(Self {
                _lib: lib,
                component,
                audio_proc,
                in_l: vec![0.0f32; block_size.max(4096)],
                in_r: vec![0.0f32; block_size.max(4096)],
            })
        }

        /// Process one stereo block in-place.
        pub fn process_stereo(&mut self, left: &mut [f32], right: &mut [f32]) {
            let n = left.len().min(right.len());
            if n == 0 { return; }
            if self.in_l.len() < n { self.in_l.resize(n, 0.0); }
            if self.in_r.len() < n { self.in_r.resize(n, 0.0); }
            self.in_l[..n].copy_from_slice(&left[..n]);
            self.in_r[..n].copy_from_slice(&right[..n]);

            let in_ptrs:  [*mut f32; 2] = [self.in_l.as_mut_ptr(), self.in_r.as_mut_ptr()];
            let out_ptrs: [*mut f32; 2] = [left.as_mut_ptr(),      right.as_mut_ptr()];

            let mut in_bus = AudioBusBuffers {
                num_channels: 2, _pad: 0, silence_flags: 0,
                channel_buffers: in_ptrs.as_ptr() as *mut *mut f32,
            };
            let mut out_bus = AudioBusBuffers {
                num_channels: 2, _pad: 0, silence_flags: 0,
                channel_buffers: out_ptrs.as_ptr() as *mut *mut f32,
            };
            let mut pd = ProcessData {
                process_mode: kRealtime as i32,
                symbolic_sample_size: kSample32 as i32,
                num_samples: n as i32,
                num_inputs: 1, num_outputs: 1, _pad: 0,
                inputs: &mut in_bus, outputs: &mut out_bus,
                input_param_changes: ptr::null_mut(),
                output_param_changes: ptr::null_mut(),
                input_events: ptr::null_mut(),
                output_events: ptr::null_mut(),
                process_context: ptr::null_mut(),
            };
            unsafe {
                let pd_ptr = &mut pd as *mut ProcessData as *mut vst3::Steinberg::Vst::ProcessData;
                self.audio_proc.process(pd_ptr);
            }
        }

        /// Snapshot the plugin state as raw bytes (serialised via IComponent::getState).
        pub fn get_state(&self) -> Vec<u8> {
            let stream = ComWrapper::new(RustIBStream::write_stream());
            if let Some(ptr) = stream.to_com_ptr::<IBStream>() {
                unsafe { self.component.getState(ptr.as_ptr()); }
                // Recover data from the inner struct via Deref
                // ComWrapper<T>: Deref<Target = T>
                return stream.buf.borrow().clone();
            }
            Vec::new()
        }

        /// Restore plugin state from raw bytes.
        pub fn set_state(&self, data: &[u8]) {
            let stream = ComWrapper::new(RustIBStream::read_stream(data.to_vec()));
            if let Some(ptr) = stream.to_com_ptr::<IBStream>() {
                unsafe { self.component.setState(ptr.as_ptr()); }
            }
        }
    }

    impl Drop for Vst3Processor {
        fn drop(&mut self) {
            unsafe {
                self.audio_proc.setProcessing(0);
                self.component.setActive(0);
                self.component.terminate();
            }
        }
    }
}

#[cfg(target_os = "windows")]
pub use win::Vst3Processor;

/// Stub for non-Windows platforms.
#[cfg(not(target_os = "windows"))]
pub struct Vst3Processor;

#[cfg(not(target_os = "windows"))]
impl Vst3Processor {
    pub fn load(_plugin_path: &str, _sample_rate: f64, _block_size: usize) -> anyhow::Result<Self> {
        Err(anyhow::anyhow!("VST3 audio processing is only supported on Windows"))
    }
    pub fn process_stereo(&mut self, _left: &mut [f32], _right: &mut [f32]) {}
    pub fn get_state(&self) -> Vec<u8> { Vec::new() }
    pub fn set_state(&self, _data: &[u8]) {}
}