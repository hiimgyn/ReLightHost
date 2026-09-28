//! VST3 audio processor — uses the `vst3` crate (ComPtr / ComWrapper / Interface).
//!
//! CRITICAL: VST3 plugins often use COM internally (D2D, DirectWrite on Windows).
//! Every thread that accesses VST3 interfaces must have COM initialized.
//! This module ensures COM is initialized on the audio thread before calling process().

#[cfg(target_os = "windows")]
mod win {
    use anyhow::{anyhow, Result};
    use libloading::{Library, Symbol};
    use std::ffi::c_void;
    use std::ptr;
    use std::time::{Duration, Instant};
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::Arc;
    use parking_lot::Mutex as PLMutex;
    use windows_sys::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

    use vst3::Steinberg::{
        kResultOk, IBStream, IBStreamTrait, IPluginFactory, IPluginFactoryTrait,
        IPluginBaseTrait, PClassInfo,
    };
    use vst3::Steinberg::Vst::
        {BusDirections_::{kInput, kOutput},
        AudioBusBuffers, AudioBusBuffers__type0,
        IAudioProcessor, IAudioProcessorTrait,
        IComponent, IComponentTrait,
        IEditController, IEditControllerTrait,
        MediaTypes_::{kAudio, kEvent},
        ProcessContext,
        ProcessData,
        ProcessModes_::kRealtime,
        ProcessSetup,
        SymbolicSampleSizes_::kSample32,
    };
    use vst3::{Class, ComPtr, ComWrapper, Interface};

    // ──────────────────────────────────────────────────────────────────
    // COM initialization per-thread (CRITICAL for VST3 plugin stability)
    // ──────────────────────────────────────────────────────────────────
    thread_local! {
        /// COM initialized on this thread with MULTITHREADED apartment
        static COM_INITIALIZED: std::cell::RefCell<bool> = const { std::cell::RefCell::new(false) };
    }

    // Milliseconds since UNIX_EPOCH; 0 means no global block.
    static GLOBAL_PROCESS_BLOCK_UNTIL_MS: AtomicU64 = AtomicU64::new(0);

    fn now_epoch_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }

    pub fn set_global_process_block_ms(block_ms: u64) {
        if block_ms == 0 {
            GLOBAL_PROCESS_BLOCK_UNTIL_MS.store(0, Ordering::Release);
            return;
        }
        let until = now_epoch_ms().saturating_add(block_ms);
        GLOBAL_PROCESS_BLOCK_UNTIL_MS.store(until, Ordering::Release);
        log::warn!("{} Global VST3 process guard enabled for {} ms", crate::core::threading::thread_prefix("plugin/vst3/guard"), block_ms);
    }

    pub fn is_vst3_settling() -> bool {
        let until = GLOBAL_PROCESS_BLOCK_UNTIL_MS.load(Ordering::Relaxed);
        until != 0 && now_epoch_ms() < until
    }

    /// Ensure COM is initialized on the current thread with MULTITHREADED model.
    /// Safe to call multiple times — only initializes once per thread.
    /// Returns true if COM was just initialized, false if already initialized.
    #[inline]
    fn ensure_com_initialized() -> bool {
        COM_INITIALIZED.with(|initialized| {
            let mut inited = initialized.borrow_mut();
            if !*inited {
                unsafe {
                    let hr = CoInitializeEx(ptr::null(), COINIT_MULTITHREADED as u32);
                    // hr == 0 (S_OK) means COM is now initialized
                    // hr == 1 (S_FALSE) means COM was already initialized on this thread
                    // hr == 0x80010106 (RPC_E_CHANGED_MODE) means COM is already
                    // initialized on this thread with a different apartment model.
                    // This is still a valid COM context for plugin calls.
                    if hr == 0 || hr == 1 || hr == 0x80010106u32 as i32 {
                        *inited = true;
                        if hr == 0x80010106u32 as i32 {
                            log::debug!("{} COM already initialized on thread (different apartment)", crate::core::threading::thread_prefix("plugin/vst3/com"));
                        } else {
                            log::debug!("{} COM initialized on thread with MULTITHREADED model", crate::core::threading::thread_prefix("plugin/vst3/com"));
                        }
                    } else {
                        log::warn!("{} CoInitializeEx failed with hr={:#010x}; proceeding anyway", crate::core::threading::thread_prefix("plugin/vst3/com"), hr as u32);
                        *inited = true; // Mark as attempted to avoid repeated failures
                    }
                }
            }
            *inited
        })
    }

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

    /// Serializes DLL loading + factory creation across concurrently-loading
    /// VST3 plugins (see PluginInstanceManager::load_plugins_parallel_results'
    /// `parallel_vst3` flag). Scope is intentionally narrow — see load()'s
    /// comment for exactly what's inside vs. outside this lock and why.
    static FACTORY_CREATE_LOCK: PLMutex<()> = PLMutex::new(());

    /// Per-plugin-*path* locks for everything after FACTORY_CREATE_LOCK
    /// releases — `component.initialize()`, bus setup, and controller
    /// creation. Two *different* plugin paths must stay free to run this
    /// section fully in parallel (that's the whole point of `parallel_vst3`),
    /// but two *instances of the same plugin DLL* share that DLL's
    /// process-wide static/global state, so their calls into this section
    /// must not overlap — a single shared lock here would defeat the
    /// parallelism this feature exists for, and no lock at all would let two
    /// copies of one plugin corrupt each other's global state for the first
    /// time (previously all VST3 loads were fully sequential, so this never
    /// happened). The map itself grows by one entry per *distinct* plugin
    /// path ever loaded this run and is never shrunk — bounded by the size
    /// of the user's plugin catalog, not by how many times they load it.
    static INIT_LOCKS: PLMutex<Option<std::collections::HashMap<String, Arc<PLMutex<()>>>>> = PLMutex::new(None);

    fn init_lock_for(plugin_path: &str) -> Arc<PLMutex<()>> {
        let mut map_guard = INIT_LOCKS.lock();
        let map = map_guard.get_or_insert_with(std::collections::HashMap::new);
        map.entry(plugin_path.to_string())
            .or_insert_with(|| Arc::new(PLMutex::new(())))
            .clone()
    }

    //  Vst3Processor
    pub struct Vst3Processor {
        component:  ComPtr<IComponent>,
        audio_proc: ComPtr<IAudioProcessor>,
        /// Optional edit controller for parameter automation.
        /// Present for both single-component and separate-controller VST3 designs.
        controller: Option<ComPtr<IEditController>>,
        in_l:       Vec<f32>,
        in_r:       Vec<f32>,
        /// Startup warmup window: skip process() briefly right after load.
        /// Some VST3 plugins crash if processed immediately after initialization.
        warmup_until: Instant,
        /// Synchronization: GUI attachment must complete before audio processing starts.
        /// Set to true by open_gui after IPlugView::attached() completes successfully.
        /// Audio processing checks this before calling process() to ensure plugin
        /// initialization is fully complete.
        attachment_ready: Arc<AtomicBool>,
        /// Held for the *entire* GUI-open lifetime (createView through cleanup),
        /// not just the initial attach handshake — closes a TOCTOU race where
        /// autosave's get_state()/set_state() (from the autosave-worker
        /// thread) could still be mid-call into the plugin's component when a
        /// quick re-open already flips `attachment_ready` back to false and
        /// starts a fresh createView()/attached() sequence on another thread.
        /// Both windows into the same live COM object at once is exactly the
        /// kind of concurrent access fragile plugins (Supertone Clear) don't
        /// tolerate. try_lock() here always yields to an open GUI session
        /// rather than block; get_state()/set_state() already treat "can't
        /// safely read/write it right now" as an empty/no-op result.
        com_access_lock: Arc<PLMutex<()>>,
        /// Tracks whether IConnectionPoint (component ↔ controller) has already been connected,
        /// ensuring we only connect once per plugin instance lifetime and never reconnect on reopen.
        icp_connected: Arc<AtomicBool>,
        /// maxSamplesPerBlock negotiated with the plugin via setupProcessing.
        /// The VST3 spec forbids calling process() with more samples than this;
        /// fragile plugins (e.g. Supertone Clear, which uses fixed-size internal
        /// buffers for its ML inference) corrupt their own heap if that contract
        /// is violated, so callers must chunk to this size (see process_stereo).
        sample_rate: f64,
        max_block:  usize,
        /// MUST be last: DLL must outlive all COM interface pointers above so that
        /// ComPtr::drop() (which calls Release() through the vtable) never fires
        /// after the DLL is unloaded.
        _lib:       Option<Library>,
    }

    // Safety: Vst3Processor owns the plugin instance; we never share it across
    // threads simultaneously.
    unsafe impl Send for Vst3Processor {}

    impl Vst3Processor {
        pub fn load(plugin_path: &str, sample_rate: f64, block_size: usize) -> Result<Self> {
            // Some plugins call COM APIs during load/initialize.
            ensure_com_initialized();

            // Only DLL loading + factory + createInstance are serialized —
            // the Windows loader lock and any COM apartment setup a plugin's
            // DllMain/static initializers do are the actual concurrency risk
            // when multiple *different* VST3 DLLs load at once (see
            // PluginInstanceManager's `parallel_vst3` flag). component.
            // initialize() below — where a heavy plugin does its own slow
            // work (e.g. loading an ML model) — runs UNLOCKED, on purpose:
            // that's the part parallel loading is trying to overlap, and it's
            // ordinary plugin code, not host-loader-adjacent code, so there's
            // no more reason to serialize it across *different* plugins than
            // there is for any other method call into a plugin.
            let (lib, factory, component): (Library, ComPtr<IPluginFactory>, ComPtr<IComponent>) = {
                let _factory_guard = FACTORY_CREATE_LOCK.lock();

                // Load DLL
                log::debug!("{} loading DLL for '{}'", crate::core::threading::thread_prefix("plugin/vst3/load"), plugin_path);
                let lib = unsafe { Library::new(plugin_path) }
                    .map_err(|e| anyhow!("Failed to load '{}': {}", plugin_path, e))?;

                // Optional InitDll (some plugins require it)
                type BoolFn = unsafe extern "system" fn() -> bool;
                if let Ok(init_dll) = unsafe { lib.get::<BoolFn>(b"InitDll\0") } {
                    if !unsafe { init_dll() } {
                        log::warn!("{} InitDll() returned false for '{}'", crate::core::threading::thread_prefix("plugin/vst3/load"), plugin_path);
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
                log::debug!("{} createInstance (IComponent) for '{}'", crate::core::threading::thread_prefix("plugin/vst3/load"), plugin_path);
                let mut component_ptr: *mut IComponent = ptr::null_mut();
                let result = unsafe {
                    factory.createInstance(
                        cid.as_ptr(),
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
                            cid.as_ptr(),
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

                // Tail expression: hand `lib`, `factory`, and `component` out
                // of the locked block as a tuple, instead of letting them
                // drop when the block ends — `lib` (the Library) must
                // outlive `component`, same as before this refactor (see the
                // existing `_lib: Option<Library>` field ordering / "MUST be
                // last" comment further down in this struct). `factory` is
                // needed again below for the separate-controller
                // createInstance call, which re-acquires the same lock for
                // that one call (see there for why).
                (lib, factory, component)
            };

            // Initialize, activate all buses (audio + event). Only serialized
            // against OTHER instances of this SAME plugin path (see
            // INIT_LOCKS) — different plugins still run this concurrently.
            let init_lock = init_lock_for(plugin_path);
            let _init_guard = init_lock.lock();
            log::debug!("{} calling initialize() for '{}' — locked only against other instances of the same plugin, may still run concurrently with other plugins", crate::core::threading::thread_prefix("plugin/vst3/load"), plugin_path);
            unsafe {
                component.initialize(ptr::null_mut());
                let ai = component.getBusCount(kAudio, kInput);
                let ao = component.getBusCount(kAudio, kOutput);
                let ei = component.getBusCount(kEvent, kInput);
                let eo = component.getBusCount(kEvent, kOutput);
                for i in 0..ai { component.activateBus(kAudio, kInput,  i, 1); }
                for i in 0..ao { component.activateBus(kAudio, kOutput, i, 1); }
                for i in 0..ei { component.activateBus(kEvent, kInput,  i, 1); }
                for i in 0..eo { component.activateBus(kEvent, kOutput, i, 1); }
            }
            log::debug!("{} initialize() returned for '{}'", crate::core::threading::thread_prefix("plugin/vst3/load"), plugin_path);

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
                processMode:        kRealtime,
                symbolicSampleSize: kSample32,
                maxSamplesPerBlock: block_size as i32,
                sampleRate:         sample_rate,
            };
            unsafe { audio_proc.setupProcessing(&mut setup) };
            unsafe {
                component.setActive(1);
                audio_proc.setProcessing(1);
            }

            // Try to obtain an IEditController for parameter automation.
            // Single-component design: IComponent itself implements IEditController.
            // Separate-controller design: ask the component for its controller CID.
            //
            // IConnectionPoint connection (component ↔ controller) is deferred
            // to open_gui() and made on the GUI thread, AFTER audio processing
            // is stable.  Connecting during load() while the audio callback is
            // already running can trigger plugin-internal callbacks that corrupt
            // the stack (STATUS_STACK_BUFFER_OVERRUN).
            let controller: Option<ComPtr<IEditController>> = {
                // Single-component: IComponent itself implements IEditController.
                // Obtain via QI — do NOT call initialize() again (component was
                // already initialized above) and do NOT terminate() this separately
                // in Drop (component.terminate() covers both interfaces).
                if let Some(ec) = component.cast::<IEditController>() {
                    // separate_controller stays false
                    Some(ec)
                } else {
                    // Separate-controller: getControllerClassId + createInstance
                    let mut ctrl_cid: vst3::Steinberg::TUID = [0i8; 16];
                    let gc_ok = unsafe { component.getControllerClassId(&mut ctrl_cid) };
                    if gc_ok == kResultOk && ctrl_cid != [0i8; 16] {
                        let mut ec_ptr: *mut IEditController = ptr::null_mut();
                        // Same lock as the IComponent createInstance above —
                        // same risk category (factory method call into the
                        // plugin's own code), reacquired here since this one
                        // fires after component.initialize(), which runs
                        // deliberately unlocked (see that block's comment).
                        let r = {
                            let _factory_guard = FACTORY_CREATE_LOCK.lock();
                            unsafe {
                                factory.createInstance(
                                    ctrl_cid.as_ptr(),
                                    IEditController::IID.as_ptr() as *const i8,
                                    &mut ec_ptr as *mut _ as *mut _,
                                )
                            }
                        };
                        if r == kResultOk && !ec_ptr.is_null() {
                            if let Some(ec) = unsafe { ComPtr::<IEditController>::from_raw(ec_ptr) } {
                                unsafe { ec.initialize(ptr::null_mut()) };
                                Some(ec)
                            } else { None }
                        } else { None }
                    } else { None }
                }
            };
            if controller.is_none() {
                log::debug!("{} '{}': IEditController not available (parameter automation disabled)", crate::core::threading::thread_prefix("plugin/vst3/load"), plugin_path);
            }

            log::info!("{} VST3 plugin loaded: '{}'", crate::core::threading::thread_prefix("plugin/vst3/load"), plugin_path);
            Ok(Self {
                _lib: Some(lib),
                component,
                audio_proc,
                controller,
                in_l: vec![0.0f32; block_size.max(4096)],
                in_r: vec![0.0f32; block_size.max(4096)],
                warmup_until: Instant::now() + Duration::from_millis(1500),
                // Processing is safe immediately after load(); this flag is only
                // toggled during GUI attach to avoid init-time races.
                attachment_ready: Arc::new(AtomicBool::new(true)),
                com_access_lock: Arc::new(PLMutex::new(())),
                icp_connected: Arc::new(AtomicBool::new(false)),
                sample_rate,
                max_block: block_size.max(1),
            })
        }

        /// Process one stereo block in-place.
        /// CRITICAL: Ensures COM is initialized on the audio thread before calling
        /// the plugin's process() function. This prevents crashes in plugins that
        /// use COM internally (like SuperTone which uses DirectWrite/D2D).
        pub fn process_stereo(&mut self, left: &mut [f32], right: &mut [f32]) {
            // Ensure COM is initialized on this audio thread with MULTITHREADED model
            ensure_com_initialized();

            // While the editor is attaching, some plugins are not ready for
            // concurrent process() calls yet. Keep pass-through until attach
            // completes to avoid STATUS_ACCESS_VIOLATION on fragile VST3s.
            if !self.attachment_ready.load(std::sync::atomic::Ordering::Acquire) {
                return;
            }

            // Backend startup guard window for fragile VST3 plugins.
            let guard_until = GLOBAL_PROCESS_BLOCK_UNTIL_MS.load(Ordering::Acquire);
            if guard_until != 0 && now_epoch_ms() < guard_until {
                return;
            }

            // Startup warmup: pass-through briefly after plugin load.
            if Instant::now() < self.warmup_until {
                return;
            }

            let total = left.len().min(right.len());
            if total == 0 { return; }

            // The host's ASIO/WASAPI callback size can vary at runtime (see
            // audio::manager's BufferSize::Default) and may exceed the
            // maxSamplesPerBlock negotiated at load(). Never hand the plugin
            // more samples per call than it was configured for.
            for (start, end) in chunk_bounds(total, self.max_block) {
                self.process_chunk(&mut left[start..end], &mut right[start..end]);
            }
        }

        fn process_chunk(&mut self, left: &mut [f32], right: &mut [f32]) {
            let n = left.len().min(right.len());
            if n == 0 { return; }
            if self.in_l.len() < n { self.in_l.resize(n, 0.0); }
            if self.in_r.len() < n { self.in_r.resize(n, 0.0); }
            self.in_l[..n].copy_from_slice(&left[..n]);
            self.in_r[..n].copy_from_slice(&right[..n]);

            let in_ptrs:  [*mut f32; 2] = [self.in_l.as_mut_ptr(), self.in_r.as_mut_ptr()];
            let out_ptrs: [*mut f32; 2] = [left.as_mut_ptr(),      right.as_mut_ptr()];

            let mut in_bus = AudioBusBuffers {
                numChannels: 2,
                silenceFlags: 0,
                __field0: AudioBusBuffers__type0 {
                    channelBuffers32: in_ptrs.as_ptr() as *mut *mut f32,
                },
            };
            let mut out_bus = AudioBusBuffers {
                numChannels: 2,
                silenceFlags: 0,
                __field0: AudioBusBuffers__type0 {
                    channelBuffers32: out_ptrs.as_ptr() as *mut *mut f32,
                },
            };
            let mut pd = ProcessData {
                processMode: kRealtime,
                symbolicSampleSize: kSample32,
                numSamples: n as i32,
                numInputs: 1,
                numOutputs: 1,
                inputs: &mut in_bus, outputs: &mut out_bus,
                inputParameterChanges: ptr::null_mut(),
                outputParameterChanges: ptr::null_mut(),
                inputEvents: ptr::null_mut(),
                outputEvents: ptr::null_mut(),
                // Some plugins incorrectly assume processContext is non-null.
                processContext: ptr::null_mut(),
            };
            unsafe {
                let mut process_ctx: ProcessContext = std::mem::zeroed();
                process_ctx.sampleRate = self.sample_rate;
                pd.processContext = &mut process_ctx;
                self.audio_proc.process(&mut pd);
            }
        }

        /// Set a parameter via IEditController::setParamNormalized.
        ///
        /// `normalized` must be in [0.0, 1.0].  No-op if no controller is available.
        pub fn set_param_normalized(&self, param_id: u32, normalized: f64) {
            ensure_com_initialized();
            if let Some(ref ctrl) = self.controller {
                unsafe { ctrl.setParamNormalized(param_id, normalized); }
            }
        }

        /// Snapshot the plugin state as raw bytes (serialised via IComponent::getState).
        pub fn get_state(&self) -> Vec<u8> {
            ensure_com_initialized();
            // com_access_lock is held for the GUI's entire open lifetime (see
            // open_gui_window) — try_lock skips rather than races if a GUI
            // session is active anywhere in it, not just mid-attach.
            let Some(_guard) = self.com_access_lock.try_lock() else {
                return Vec::new();
            };
            if !self.attachment_ready.load(std::sync::atomic::Ordering::Acquire) {
                return Vec::new();
            }
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
            ensure_com_initialized();
            let Some(_guard) = self.com_access_lock.try_lock() else {
                return;
            };
            if !self.attachment_ready.load(std::sync::atomic::Ordering::Acquire) {
                return;
            }
            let stream = ComWrapper::new(RustIBStream::read_stream(data.to_vec()));
            if let Some(ptr) = stream.to_com_ptr::<IBStream>() {
                unsafe { self.component.setState(ptr.as_ptr()); }
            }
        }

        /// Open the plugin's native editor GUI using the existing IEditController.
        ///
        /// The IConnectionPoint component↔controller handshake and parameter sync
        /// (setComponentState) are performed on the dedicated GUI thread, just
        /// before createView(), so that the audio callback is not disturbed.
        ///
        /// Sets `attachment_ready` to true after IPlugView::attached() completes
        /// successfully, ensuring the audio thread doesn't process until the plugin
        /// GUI initialization is fully complete.
        pub fn open_gui(
            &self,
            plugin_name: &str,
            gui_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
            gui_hwnd: std::sync::Arc<std::sync::atomic::AtomicIsize>,
            sync_component_state: bool,
            restored_state_blob: Option<Vec<u8>>,
        ) -> Result<()> {
            ensure_com_initialized();
            let controller = self.controller.as_ref()
                .ok_or_else(|| anyhow!("Plugin '{}' has no IEditController (GUI not supported)", plugin_name))?;

            // Block process() while the editor performs attach-time
            // initialization on another thread.
            self.attachment_ready.store(false, std::sync::atomic::Ordering::Release);

            // Clone the component so the GUI thread can:
            //   1. connect it to the controller via IConnectionPoint
            //   2. replay the restored snapshot into controller.setComponentState()
            let component = self.component.clone();
            let attachment_ready = Arc::clone(&self.attachment_ready);
            let com_access_lock = Arc::clone(&self.com_access_lock);
            let icp_connected = Arc::clone(&self.icp_connected);

            if let Err(e) = crate::plugins::gui::vst3::win::open_gui_window(
                controller,
                &component,
                plugin_name,
                gui_flag,
                gui_hwnd,
                attachment_ready.clone(),
                com_access_lock,
                icp_connected,
                sync_component_state,
                restored_state_blob,
            ) {
                // If spawn/open failed before attach could run, unblock process().
                attachment_ready.store(true, std::sync::atomic::Ordering::Release);
                return Err(e);
            }
            Ok(())
        }
    }

    impl Drop for Vst3Processor {
        fn drop(&mut self) {
            unsafe {
                // Conservative shutdown: request processing stop and component inactive.
                // Do NOT call terminate() explicitly here; some plugins corrupt heap
                // when terminate() races with GUI/thread teardown during removal.
                let _ = self.audio_proc.setProcessing(0);
                let _ = self.component.setActive(0);
            }

            // Stability > reclamation: keep plugin DLL loaded until process exit.
            // This avoids heap corruption/use-after-free in plugins that keep
            // background threads alive briefly after host teardown.
            if let Some(lib) = self._lib.take() {
                std::mem::forget(lib);
            }
        }
    }

    /// Split `[0, total)` into contiguous `(start, end)` ranges no larger than
    /// `max_block`, so process_stereo never violates a plugin's maxSamplesPerBlock.
    fn chunk_bounds(total: usize, max_block: usize) -> impl Iterator<Item = (usize, usize)> {
        let max_block = max_block.max(1);
        (0..total).step_by(max_block).map(move |start| (start, (start + max_block).min(total)))
    }

    #[cfg(test)]
    mod chunk_tests {
        use super::chunk_bounds;

        #[test]
        fn covers_whole_range_without_gaps_or_overlaps() {
            for (total, max_block) in [(0, 512), (100, 512), (512, 512), (1000, 512), (2049, 1024)] {
                let bounds: Vec<_> = chunk_bounds(total, max_block).collect();
                let mut expected_start = 0;
                for (start, end) in &bounds {
                    assert_eq!(*start, expected_start);
                    assert!(end - start <= max_block);
                    expected_start = *end;
                }
                assert_eq!(expected_start, total);
            }
        }
    }
}

#[cfg(target_os = "windows")]
pub use win::Vst3Processor;
#[cfg(target_os = "windows")]
pub use win::set_global_process_block_ms;
#[cfg(target_os = "windows")]
pub use win::is_vst3_settling;

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

#[cfg(not(target_os = "windows"))]
pub fn set_global_process_block_ms(_block_ms: u64) {}

#[cfg(not(target_os = "windows"))]
pub fn is_vst3_settling() -> bool { false }

#[cfg(test)]
mod lock_tests {
    use parking_lot::Mutex;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    /// Stands in for `win::FACTORY_CREATE_LOCK` — same shape, defined here
    /// so this test compiles on non-Windows too. Proves the pattern Task 1
    /// step 3 wires into `load()`: while one thread holds the lock, a second
    /// thread attempting to acquire it must block until the first releases.
    fn test_lock() -> &'static Mutex<()> {
        static LOCK: Mutex<()> = Mutex::new(());
        &LOCK
    }

    #[test]
    fn second_acquirer_blocks_until_first_releases() {
        let in_critical_section = Arc::new(AtomicU32::new(0));
        let max_concurrent = Arc::new(AtomicU32::new(0));

        let mut handles = Vec::new();
        for _ in 0..8 {
            let in_cs = Arc::clone(&in_critical_section);
            let max_cs = Arc::clone(&max_concurrent);
            handles.push(thread::spawn(move || {
                let _guard = test_lock().lock();
                let now = in_cs.fetch_add(1, Ordering::SeqCst) + 1;
                max_cs.fetch_max(now, Ordering::SeqCst);
                thread::sleep(Duration::from_millis(5));
                in_cs.fetch_sub(1, Ordering::SeqCst);
            }));
        }
        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(
            max_concurrent.load(Ordering::SeqCst),
            1,
            "more than one thread was inside the locked section at once"
        );
    }

    /// Stands in for `win::init_lock_for` — same shape, defined here so this
    /// test compiles on non-Windows too. Fixes review finding "Important #3":
    /// two *instances of the same plugin path* must never run their
    /// `initialize()` (and the rest of the unlocked post-createInstance
    /// section) concurrently, since they share that DLL's process-wide
    /// static/global state — but *different* plugin paths must still be
    /// free to run fully in parallel (that's the whole point of this
    /// feature), so this can't just be one lock shared by everyone.
    fn test_init_lock_for(
        registry: &Mutex<std::collections::HashMap<String, Arc<Mutex<()>>>>,
        plugin_path: &str,
    ) -> Arc<Mutex<()>> {
        let mut map = registry.lock();
        map.entry(plugin_path.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    #[test]
    fn same_path_serializes_initialize_but_different_paths_run_concurrently() {
        let registry: Arc<Mutex<std::collections::HashMap<String, Arc<Mutex<()>>>>> =
            Arc::new(Mutex::new(std::collections::HashMap::new()));

        // Two "instances" of the SAME plugin path — their critical sections
        // (standing in for initialize()) must never overlap.
        let same_path_concurrent = Arc::new(AtomicU32::new(0));
        let same_path_max = Arc::new(AtomicU32::new(0));
        let mut handles = Vec::new();
        for _ in 0..4 {
            let reg = Arc::clone(&registry);
            let cur = Arc::clone(&same_path_concurrent);
            let max = Arc::clone(&same_path_max);
            handles.push(thread::spawn(move || {
                let lock = test_init_lock_for(&reg, "C:/Same/Plugin.vst3");
                let _guard = lock.lock();
                let now = cur.fetch_add(1, Ordering::SeqCst) + 1;
                max.fetch_max(now, Ordering::SeqCst);
                thread::sleep(Duration::from_millis(5));
                cur.fetch_sub(1, Ordering::SeqCst);
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(
            same_path_max.load(Ordering::SeqCst),
            1,
            "two instances of the SAME plugin path ran initialize() concurrently"
        );

        // Two DIFFERENT plugin paths — must be able to overlap (this is what
        // the whole feature is for; a global per-registry lock would break it).
        let different_paths_concurrent = Arc::new(AtomicU32::new(0));
        let different_paths_max = Arc::new(AtomicU32::new(0));
        let mut handles = Vec::new();
        for path in ["C:/Plugin/A.vst3", "C:/Plugin/B.vst3"] {
            let reg = Arc::clone(&registry);
            let cur = Arc::clone(&different_paths_concurrent);
            let max = Arc::clone(&different_paths_max);
            handles.push(thread::spawn(move || {
                let lock = test_init_lock_for(&reg, path);
                let _guard = lock.lock();
                let now = cur.fetch_add(1, Ordering::SeqCst) + 1;
                max.fetch_max(now, Ordering::SeqCst);
                thread::sleep(Duration::from_millis(20));
                cur.fetch_sub(1, Ordering::SeqCst);
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(
            different_paths_max.load(Ordering::SeqCst),
            2,
            "different plugin paths were serialized against each other — parallelism broken"
        );
    }
}