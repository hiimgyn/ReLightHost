//! Out-of-process VST3 hosting for plugins that have repeatedly crashed
//! in-process (see `registry`). A crash inside `vst3_sandbox_host.exe` kills
//! that child, not `ReLightHost.exe` — `crash_protection.rs`'s
//! `catch_unwind` guard can never see a native SEH crash (heap corruption,
//! access violation) from a plugin's own C++ code, so a real OS process
//! boundary is the only isolation primitive that actually holds.
//!
//! `SandboxedVst3Processor` mirrors `processor::vst3::Vst3Processor`'s
//! public surface (`process_stereo`, `get_state`, `set_state`,
//! `set_param_normalized`, `open_gui`) so `Vst3ProcessorKind` below can
//! dispatch to either without `instance.rs` needing to know which one it has
//! beyond the initial load decision.

pub mod protocol;
pub mod registry;

use anyhow::{anyhow, Result};
use parking_lot::Mutex as PLMutex;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::JoinHandle;
use std::time::Duration;

use protocol::{ControlRequest, ControlResponse};

/// How long a single audio block waits for the child's response before this
/// block is left unprocessed (pass-through). Chosen to comfortably fit
/// inside the smallest realistic block budget (256 samples @ 48kHz ≈ 5.3ms)
/// while still being short enough that a hung child doesn't stall the audio
/// thread for long.
///
/// ponytail: fixed value rather than derived from the configured buffer
/// size — the real budget varies per device/session, but sandboxing only
/// ever applies to plugins that have already proven unreliable, so the
/// occasional extra glitch this causes at very small buffer sizes is an
/// accepted trade for "never blocks the audio thread indefinitely". Revisit
/// if sandboxed plugins turn out to be used at buffer sizes small enough
/// that this dominates.
const PROCESS_RESPONSE_TIMEOUT: Duration = Duration::from_millis(20);

/// Loading a VST3 plugin can legitimately take a long time (observed: ~60s
/// for a plugin loading a bundled ML model) — the load handshake gets its
/// own, much longer deadline than a single audio block.
const LOAD_TIMEOUT: Duration = Duration::from_secs(90);

/// Non-realtime control round-trips (state save/restore, parameter set).
const CONTROL_TIMEOUT: Duration = Duration::from_secs(5);

fn sandbox_host_exe_path() -> Result<std::path::PathBuf> {
    let mut path = std::env::current_exe()
        .map_err(|e| anyhow!("Cannot resolve current_exe: {e}"))?;
    path.pop();
    path.push(if cfg!(windows) { "vst3_sandbox_host.exe" } else { "vst3_sandbox_host" });
    Ok(path)
}

/// One-shot mailbox for the next `GuiClosed` notification. `open_gui` drops
/// a fresh `Sender` in here right before asking the child to open the
/// editor and keeps the matching `Receiver` for itself; the reader thread
/// takes (consumes) whatever sender is currently parked here when a
/// `GuiClosed` frame arrives. `None` between GUI sessions means "nobody is
/// watching" and the notification is simply dropped — there's nothing to
/// tell in that case.
type GuiClosedMailbox = Arc<PLMutex<Option<mpsc::Sender<()>>>>;

struct ChildLink {
    child: Child,
    stdin: ChildStdin,
    /// Audio block responses only — `process_stereo` is the sole reader, so
    /// it never competes with control traffic for a message.
    process_rx: mpsc::Receiver<Vec<u8>>,
    /// Every `ControlResponse` except `GuiClosed`, which is routed through
    /// `gui_closed_mailbox` instead — otherwise a `GuiClosed` arriving while
    /// `process_stereo` happens to be the one polling would be silently
    /// dropped (only one receiver ever gets a given message).
    control_rx: mpsc::Receiver<ControlResponse>,
    gui_closed_mailbox: GuiClosedMailbox,
    /// Kept only so the reader thread is joined (and doesn't leak) when a
    /// `ChildLink` is replaced or dropped.
    _reader: JoinHandle<()>,
}

impl Drop for ChildLink {
    fn drop(&mut self) {
        let _ = protocol::write_control(&mut self.stdin, &ControlRequest::Shutdown);
        // Give the child a brief window to exit cleanly, then force it —
        // matches the "never block indefinitely on a plugin" rule.
        for _ in 0..20 {
            if let Ok(Some(_)) = self.child.try_wait() { return; }
            std::thread::sleep(Duration::from_millis(25));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn spawn_child(plugin_path: &str, sample_rate: f64, block_size: usize) -> Result<ChildLink> {
    let exe = sandbox_host_exe_path()?;
    let mut child = Command::new(&exe)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        // Inherit stderr so the child's own log lines land in the same
        // console/log file as the host — nothing extra to wire up.
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| anyhow!("Failed to spawn '{}': {e}", exe.display()))?;

    #[cfg(target_os = "windows")]
    win::assign_kill_on_close_job(&child)?;

    let mut stdin = child.stdin.take().ok_or_else(|| anyhow!("sandbox child has no stdin"))?;
    let stdout = child.stdout.take().ok_or_else(|| anyhow!("sandbox child has no stdout"))?;

    let (process_tx, process_rx) = mpsc::channel::<Vec<u8>>();
    let (control_tx, control_rx) = mpsc::channel::<ControlResponse>();
    let gui_closed_mailbox: GuiClosedMailbox = Arc::new(PLMutex::new(None));
    let mailbox_for_reader = Arc::clone(&gui_closed_mailbox);
    let reader = std::thread::Builder::new()
        .name("vst3-sandbox-reader".into())
        .spawn(move || {
            let mut stdout = stdout;
            loop {
                let (tag, payload) = match protocol::read_frame(&mut stdout) {
                    Ok(f) => f,
                    Err(_) => break, // pipe closed — child exited or crashed
                };
                match tag {
                    protocol::TAG_PROCESS_RESPONSE => {
                        if process_tx.send(payload).is_err() { break; }
                    }
                    protocol::TAG_CONTROL => {
                        match protocol::decode_control_response(&payload) {
                            Ok(ControlResponse::GuiClosed) => {
                                if let Some(tx) = mailbox_for_reader.lock().take() {
                                    let _ = tx.send(());
                                }
                            }
                            Ok(resp) => {
                                if control_tx.send(resp).is_err() { break; }
                            }
                            Err(_) => {} // malformed control frame — ignore, keep reading
                        }
                    }
                    _ => {} // unknown tag — ignore, keep reading
                }
            }
        })
        .map_err(|e| anyhow!("Failed to spawn sandbox reader thread: {e}"))?;

    protocol::write_control(&mut stdin, &ControlRequest::Load {
        plugin_path: plugin_path.to_string(),
        sample_rate,
        block_size,
    }).map_err(|e| anyhow!("Failed to send Load to sandbox child: {e}"))?;

    match control_rx.recv_timeout(LOAD_TIMEOUT) {
        Ok(ControlResponse::Loaded { name }) => {
            log::info!("VST3 sandboxed: '{name}' loaded in child process (pid {})", child.id());
        }
        Ok(ControlResponse::LoadFailed { error }) => {
            let _ = child.kill();
            return Err(anyhow!("sandbox child failed to load plugin: {error}"));
        }
        Ok(other) => return Err(anyhow!("unexpected response during load: {other:?}")),
        Err(_) => {
            let _ = child.kill();
            return Err(anyhow!("sandbox child did not respond to Load within {:?}", LOAD_TIMEOUT));
        }
    }

    Ok(ChildLink { child, stdin, process_rx, control_rx, gui_closed_mailbox, _reader: reader })
}

pub struct SandboxedVst3Processor {
    plugin_path: String,
    sample_rate: f64,
    block_size: usize,
    inner: PLMutex<Option<ChildLink>>,
    /// Set once by `process_stereo` the first time it notices the child is
    /// gone; cleared by `take_child_died()`. `instance.rs` polls this to
    /// feed the existing `CrashProtection` restart/threshold machinery —
    /// the child dying isn't a Rust panic, so `protected_call`'s
    /// `catch_unwind` never sees it on its own.
    child_died: AtomicBool,
    gui_hwnd: Arc<AtomicIsize>,
}

impl SandboxedVst3Processor {
    pub fn load(plugin_path: &str, sample_rate: f64, block_size: usize) -> Result<Self> {
        let link = spawn_child(plugin_path, sample_rate, block_size)?;
        Ok(Self {
            plugin_path: plugin_path.to_string(),
            sample_rate,
            block_size,
            inner: PLMutex::new(Some(link)),
            child_died: AtomicBool::new(false),
            gui_hwnd: Arc::new(AtomicIsize::new(0)),
        })
    }

    /// Process one stereo block. Real-time-safe: never blocks longer than
    /// `PROCESS_RESPONSE_TIMEOUT`, and never blocks at all if the inner lock
    /// is contended (matches `Vst3Processor`/`PluginInstance`'s existing
    /// try_lock-and-pass-through philosophy).
    pub fn process_stereo(&self, left: &mut [f32], right: &mut [f32]) {
        let Some(mut guard) = self.inner.try_lock() else { return };
        let Some(link) = guard.as_mut() else { return };

        let payload = protocol::encode_process_block(left, right);
        if protocol::write_frame(&mut link.stdin, protocol::TAG_PROCESS_REQUEST, &payload).is_err() {
            *guard = None;
            self.child_died.store(true, Ordering::Release);
            return;
        }

        match link.process_rx.recv_timeout(PROCESS_RESPONSE_TIMEOUT) {
            Ok(payload) => {
                if let Some((decoded_l, decoded_r)) = protocol::decode_process_block(&payload) {
                    let n = left.len().min(right.len()).min(decoded_l.len()).min(decoded_r.len());
                    left[..n].copy_from_slice(&decoded_l[..n]);
                    right[..n].copy_from_slice(&decoded_r[..n]);
                }
                // Malformed payload: leave audio unchanged for this block.
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // Child is slow/stuck on this block — pass through and try
                // again next block. Only a broken pipe (below) counts as
                // "the child is gone".
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                *guard = None;
                self.child_died.store(true, Ordering::Release);
            }
        }
    }

    /// See `child_died` doc comment.
    pub fn take_child_died(&self) -> bool {
        self.child_died.swap(false, Ordering::AcqRel)
    }

    /// Reload the plugin in a fresh child process, replacing the dead one.
    /// Blocking (up to `LOAD_TIMEOUT`) — callers must run this off the
    /// audio thread, same rule as `CrashProtection::try_auto_recover`'s
    /// in-process counterpart.
    pub fn restart(&self) -> Result<()> {
        let link = spawn_child(&self.plugin_path, self.sample_rate, self.block_size)?;
        *self.inner.lock() = Some(link);
        Ok(())
    }

    pub fn get_state(&self) -> Vec<u8> {
        let Some(mut guard) = self.inner.try_lock() else { return Vec::new() };
        let Some(link) = guard.as_mut() else { return Vec::new() };
        if protocol::write_control(&mut link.stdin, &ControlRequest::GetState).is_err() {
            return Vec::new();
        }
        match link.control_rx.recv_timeout(CONTROL_TIMEOUT) {
            Ok(ControlResponse::State { data }) => data,
            _ => Vec::new(),
        }
    }

    pub fn set_state(&self, data: &[u8]) {
        let Some(mut guard) = self.inner.try_lock() else { return };
        let Some(link) = guard.as_mut() else { return };
        let _ = protocol::write_control(&mut link.stdin, &ControlRequest::SetState { data: data.to_vec() });
        let _ = link.control_rx.recv_timeout(CONTROL_TIMEOUT);
    }

    pub fn set_param_normalized(&self, param_id: u32, normalized: f64) {
        let Some(mut guard) = self.inner.try_lock() else { return };
        let Some(link) = guard.as_mut() else { return };
        let _ = protocol::write_control(&mut link.stdin, &ControlRequest::SetParameter { param_id, normalized });
    }

    /// Ask the child to open the plugin's editor. The child creates and owns
    /// the whole window (reuses `gui::vst3::win::run_gui_window_impl`
    /// unmodified) and reports the HWND back; from then on
    /// `PluginInstance::request_close_gui`'s existing `PostMessageW` works
    /// unchanged, since HWNDs are session-wide, not process-scoped.
    pub fn open_gui(
        &self,
        plugin_name: &str,
        gui_flag: Arc<AtomicBool>,
        gui_hwnd: Arc<AtomicIsize>,
    ) -> Result<()> {
        let Some(mut guard) = self.inner.try_lock() else {
            return Err(anyhow!("'{plugin_name}' sandbox link busy — try again"));
        };
        let Some(link) = guard.as_mut() else {
            return Err(anyhow!("'{plugin_name}' has no active sandbox child"));
        };

        // Register for the eventual GuiClosed notification before asking
        // the child to open anything, so there's no window where it could
        // arrive before anyone is listening.
        let (closed_tx, closed_rx) = mpsc::channel::<()>();
        *link.gui_closed_mailbox.lock() = Some(closed_tx);

        #[cfg(target_os = "windows")]
        win::allow_foreground(link.child.id());

        protocol::write_control(&mut link.stdin, &ControlRequest::OpenGui)
            .map_err(|e| anyhow!("failed to request GUI open: {e}"))?;

        // Wait for the child to report the HWND it created (or a failure).
        // This is the one control round-trip allowed to block the caller —
        // `open_gui` is already invoked off the audio thread (a Tauri
        // command handler), same as the in-process path.
        let hwnd = match link.control_rx.recv_timeout(CONTROL_TIMEOUT) {
            Ok(ControlResponse::GuiHwnd { hwnd }) => hwnd,
            Ok(ControlResponse::GuiOpenFailed { error }) => {
                *link.gui_closed_mailbox.lock() = None;
                gui_flag.store(false, Ordering::Release);
                return Err(anyhow!("'{plugin_name}' sandbox GUI open failed: {error}"));
            }
            _ => {
                *link.gui_closed_mailbox.lock() = None;
                gui_flag.store(false, Ordering::Release);
                return Err(anyhow!("'{plugin_name}' sandbox child did not respond to OpenGui"));
            }
        };
        drop(guard);

        self.gui_hwnd.store(hwnd, Ordering::Release);
        gui_hwnd.store(hwnd, Ordering::Release);

        // Wait for the close notification on its own thread — this can sit
        // for as long as the user keeps the editor open.
        let plugin_name_owned = plugin_name.to_string();
        std::thread::Builder::new()
            .name("vst3-sandbox-gui-watch".into())
            .spawn(move || {
                let _ = closed_rx.recv();
                gui_flag.store(false, Ordering::Release);
                gui_hwnd.store(0, Ordering::Release);
                crate::app_events::emit_plugin_chain_changed("gui_close", None);
                log::debug!("'{plugin_name_owned}' sandboxed VST3 GUI closed");
            })
            .map_err(|e| anyhow!("failed to spawn sandbox GUI-close watcher: {e}"))?;

        Ok(())
    }
}

#[cfg(target_os = "windows")]
mod win {
    use anyhow::{anyhow, Result};
    use std::os::windows::io::AsRawHandle;
    use std::process::Child;
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, SetInformationJobObject,
        JobObjectExtendedLimitInformation, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };
    use windows_sys::Win32::Foundation::HANDLE;

    /// Assign `child` to a Job Object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`.
    /// The job handle is intentionally leaked (owned for the process
    /// lifetime): Windows kills every process still assigned to it — the
    /// sandboxed child included — the moment this handle closes, which we
    /// want to happen only on host process exit (including a host crash),
    /// not whenever this function returns.
    pub fn assign_kill_on_close_job(child: &Child) -> Result<()> {
        unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() {
                return Err(anyhow!("CreateJobObjectW failed"));
            }

            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let ok = SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const std::ffi::c_void,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            if ok == 0 {
                return Err(anyhow!("SetInformationJobObject failed"));
            }

            let process_handle = child.as_raw_handle() as HANDLE;
            if AssignProcessToJobObject(job, process_handle) == 0 {
                return Err(anyhow!("AssignProcessToJobObject failed"));
            }
        }
        Ok(())
    }

    /// Grant the sandboxed child the right to call SetForegroundWindow once.
    ///
    /// Windows silently refuses SetForegroundWindow for a process with no
    /// recent input of its own (the taskbar icon flashes instead) — a plain
    /// spawned child process never has that standing on its own. The editor
    /// window this backs (`gui::vst3::win`) still gets created and reports
    /// success from IPlugView::attached(), but DWM never promotes it to a
    /// properly composited top-level window, so it paints as a blank host
    /// background instead of the plugin's own content. Call this from the
    /// same Tauri-command call stack as the user's "Launch" click (still
    /// within its input-event window) right before asking the child to open
    /// its GUI, not once at spawn time — the grant does not survive
    /// unrelated foreground-window changes in between.
    pub fn allow_foreground(child_pid: u32) {
        use windows_sys::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow;
        let ok = unsafe { AllowSetForegroundWindow(child_pid) };
        log::debug!("AllowSetForegroundWindow({child_pid}) -> {ok} (0 = failed/refused)");
    }
}

/// Either an in-process or sandboxed VST3 processor, behind one interface so
/// `instance.rs` only branches on which one to construct — every call site
/// after that (`process_stereo`, `get_state`, ...) stays uniform.
pub enum Vst3ProcessorKind {
    InProcess(super::vst3::Vst3Processor),
    Sandboxed(Arc<SandboxedVst3Processor>),
}

impl Vst3ProcessorKind {
    pub fn process_stereo(&mut self, left: &mut [f32], right: &mut [f32]) {
        match self {
            Self::InProcess(p) => p.process_stereo(left, right),
            Self::Sandboxed(p) => p.process_stereo(left, right),
        }
    }

    pub fn get_state(&self) -> Vec<u8> {
        match self {
            Self::InProcess(p) => p.get_state(),
            Self::Sandboxed(p) => p.get_state(),
        }
    }

    pub fn set_state(&self, data: &[u8]) {
        match self {
            Self::InProcess(p) => p.set_state(data),
            Self::Sandboxed(p) => p.set_state(data),
        }
    }

    pub fn set_param_normalized(&self, param_id: u32, normalized: f64) {
        match self {
            Self::InProcess(p) => p.set_param_normalized(param_id, normalized),
            Self::Sandboxed(p) => p.set_param_normalized(param_id, normalized),
        }
    }

    pub fn open_gui(
        &self,
        plugin_name: &str,
        gui_flag: Arc<AtomicBool>,
        gui_hwnd: Arc<AtomicIsize>,
        sync_component_state: bool,
        restored_state_blob: Option<Vec<u8>>,
    ) -> Result<()> {
        match self {
            Self::InProcess(p) => p.open_gui(plugin_name, gui_flag, gui_hwnd, sync_component_state, restored_state_blob),
            Self::Sandboxed(p) => p.open_gui(plugin_name, gui_flag, gui_hwnd),
        }
    }

    /// `Some` only for the sandboxed variant — see module docs.
    pub fn as_sandboxed(&self) -> Option<&Arc<SandboxedVst3Processor>> {
        match self {
            Self::Sandboxed(p) => Some(p),
            Self::InProcess(_) => None,
        }
    }
}
