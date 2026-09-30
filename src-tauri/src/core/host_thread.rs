//! The plugin host thread: the one thread that creates, drives and destroys
//! plugins and the audio streams.
//!
//! Tauri runs synchronous commands on the app's main (UI) thread, so a slow
//! plugin load or session restore froze the window. That work now runs here
//! instead, behind async commands, while this thread keeps the properties
//! plugins and ASIO drivers relied on the main thread for: one stable
//! thread identity, COM initialised as a single-threaded apartment, and a
//! Win32 message pump (JUCE timers, driver window messages) between jobs.

use std::cell::Cell;
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::sync::mpsc::{self, Sender, TryRecvError};
use std::sync::OnceLock;
use std::time::Duration;

type Job = Box<dyn FnOnce() + Send>;

static SENDER: OnceLock<parking_lot::Mutex<Sender<Job>>> = OnceLock::new();

thread_local! {
    static IS_HOST: Cell<bool> = const { Cell::new(false) };
}

/// Longest a queued job waits while the thread idles in its message wait.
const PUMP_INTERVAL: Duration = Duration::from_millis(5);

fn sender() -> Sender<Job> {
    SENDER
        .get_or_init(|| {
            let (tx, rx) = mpsc::channel::<Job>();
            std::thread::Builder::new()
                .name("plugin-host".into())
                .spawn(move || host_loop(rx))
                .expect("failed to spawn the plugin host thread");
            parking_lot::Mutex::new(tx)
        })
        .lock()
        .clone()
}

fn host_loop(rx: mpsc::Receiver<Job>) {
    IS_HOST.with(|h| h.set(true));
    #[cfg(target_os = "windows")]
    unsafe {
        use windows_sys::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
        // Same apartment model as the UI thread these jobs used to run on.
        CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32);
    }
    loop {
        // Run everything queued, then dispatch window messages.
        loop {
            match rx.try_recv() {
                Ok(job) => job(),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }
        pump_messages();
        wait_for_work(&rx);
    }
}

/// Sleeps until a window message arrives or `PUMP_INTERVAL` passes, so
/// driver/plugin messages are handled as promptly as on a normal UI thread
/// and a queued job waits at most one interval.
#[cfg(target_os = "windows")]
fn wait_for_work(_rx: &mpsc::Receiver<Job>) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MsgWaitForMultipleObjects, QS_ALLINPUT};
    unsafe {
        MsgWaitForMultipleObjects(0, std::ptr::null(), 0, PUMP_INTERVAL.as_millis() as u32, QS_ALLINPUT);
    }
}

#[cfg(not(target_os = "windows"))]
fn wait_for_work(rx: &mpsc::Receiver<Job>) {
    if let Ok(job) = rx.recv_timeout(PUMP_INTERVAL) {
        job();
    }
}

/// Dispatches pending window messages for windows/timers owned by this
/// thread (JUCE message-thread timers, ASIO driver windows, …).
fn pump_messages() {
    #[cfg(target_os = "windows")]
    unsafe {
        use windows_sys::Win32::UI::WindowsAndMessaging::{DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE};
        let mut msg: MSG = std::mem::zeroed();
        while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

/// A ~5 ms pause for polling loops (waiting for a plugin GUI thread to exit,
/// for the VST3 state replay, …). On the host thread it keeps dispatching
/// window messages meanwhile: a plugin thread may itself be waiting on this
/// thread's message queue (JUCE's message-thread lock), and a plain sleep
/// would stall both until the loop times out.
pub fn wait_a_moment() {
    if IS_HOST.with(|h| h.get()) {
        pump_messages();
        #[cfg(target_os = "windows")]
        {
            use windows_sys::Win32::UI::WindowsAndMessaging::{MsgWaitForMultipleObjects, QS_ALLINPUT};
            unsafe {
                MsgWaitForMultipleObjects(0, std::ptr::null(), 0, PUMP_INTERVAL.as_millis() as u32, QS_ALLINPUT);
            }
        }
        #[cfg(not(target_os = "windows"))]
        std::thread::sleep(PUMP_INTERVAL);
        pump_messages();
    } else {
        std::thread::sleep(PUMP_INTERVAL);
    }
}

/// Runs `f` on the host thread without waiting for it.
pub fn post(f: impl FnOnce() + Send + 'static) {
    let job: Job = Box::new(move || {
        if catch_unwind(AssertUnwindSafe(f)).is_err() {
            log::error!("A posted plugin-host job panicked");
        }
    });
    let _ = sender().send(job);
}

/// Runs `f` on the host thread and waits for its result. Called from the
/// host thread itself it runs inline (no self-deadlock). A panic in `f` is
/// re-raised in the caller; the host thread survives it.
pub fn run_blocking<R: Send + 'static>(f: impl FnOnce() -> R + Send + 'static) -> R {
    if IS_HOST.with(|h| h.get()) {
        return f();
    }
    let (tx, rx) = mpsc::sync_channel(1);
    let job: Job = Box::new(move || {
        let _ = tx.send(catch_unwind(AssertUnwindSafe(f)));
    });
    sender().send(job).expect("plugin host thread is gone");
    match rx.recv().expect("plugin host thread dropped a job") {
        Ok(r) => r,
        Err(panic) => resume_unwind(panic),
    }
}

/// Async-command form of [`run_blocking`]: waits on a blocking-pool thread,
/// so neither the UI thread nor an async worker is held up.
pub async fn run<R: Send + 'static>(f: impl FnOnce() -> R + Send + 'static) -> R {
    tauri::async_runtime::spawn_blocking(move || run_blocking(f))
        .await
        .unwrap_or_else(|e| panic!("plugin host job could not be awaited: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jobs_run_in_order_on_one_dedicated_thread() {
        let caller = std::thread::current().id();
        let a = run_blocking(|| std::thread::current().id());
        let b = run_blocking(|| std::thread::current().id());
        assert_eq!(a, b, "always the same thread");
        assert_ne!(a, caller, "never the caller's thread");
    }

    #[test]
    fn nested_calls_from_the_host_thread_run_inline_instead_of_deadlocking() {
        let inner = run_blocking(|| run_blocking(|| 7));
        assert_eq!(inner, 7);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn waiting_on_the_host_thread_keeps_its_message_queue_moving() {
        use windows_sys::Win32::System::Threading::GetCurrentThreadId;
        use windows_sys::Win32::UI::WindowsAndMessaging::{PeekMessageW, PostThreadMessageW, MSG, PM_NOREMOVE, WM_APP};
        let still_queued = run_blocking(|| unsafe {
            PostThreadMessageW(GetCurrentThreadId(), WM_APP + 7, 0, 0);
            wait_a_moment();
            let mut msg: MSG = std::mem::zeroed();
            PeekMessageW(&mut msg, std::ptr::null_mut(), WM_APP + 7, WM_APP + 7, PM_NOREMOVE) != 0
        });
        assert!(!still_queued, "a polling wait on the host thread must dispatch messages");
    }

    #[test]
    fn posted_jobs_run_before_a_later_blocking_job() {
        let (tx, rx) = std::sync::mpsc::channel();
        post(move || tx.send(1).unwrap());
        run_blocking(|| ());
        assert_eq!(rx.try_recv(), Ok(1));
    }
}
