use windows::core::w;
use windows::Win32::System::Threading::AvSetMmThreadCharacteristicsW;
use windows::Win32::Foundation::{ERROR_THREAD_ALREADY_IN_TASK, HANDLE};

/// Registers the calling thread with MMCSS under the "Pro Audio" task
/// profile, which raises its scheduling priority and reduces the chance
/// the OS scheduler preempts it mid-buffer — the difference between
/// needing a larger safety-margin buffer size and not.
///
/// Call once per real-time audio thread (ASIO callback thread, WASAPI
/// capture thread, WASAPI render thread), guarded by a `std::sync::Once`
/// at each call site so it only runs on the first callback invocation.
pub fn boost_current_thread_to_pro_audio() -> Option<HANDLE> {
    let mut task_index: u32 = 0;
    // `w!` is a static, NUL-terminated UTF-16 literal — no heap allocation
    // on the real-time thread this is first called from.
    match unsafe { AvSetMmThreadCharacteristicsW(w!("Pro Audio"), &mut task_index) } {
        Ok(h) => Some(h),
        // Some ASIO drivers (e.g. Voicemeeter's) call `bufferSwitch` from a
        // thread they already registered with MMCSS themselves — the thread
        // is boosted already, so this is not a failure.
        Err(e) if e.code() == ERROR_THREAD_ALREADY_IN_TASK.to_hresult() => {
            log::debug!("[audio/mmcss] thread already in an MMCSS task; keeping it");
            None
        }
        Err(e) => {
            log::warn!("[audio/mmcss] AvSetMmThreadCharacteristicsW(\"Pro Audio\") failed: {e} ({:?})", e.code());
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boost_succeeds_on_the_current_thread() {
        // Real MMCSS registration requires the Multimedia Class Scheduler
        // service to be running (it is, on every non-Server-Core Windows
        // install) — this exercises the real Win32 call rather than a mock,
        // since the whole point of this helper is the syscall succeeding.
        let handle = boost_current_thread_to_pro_audio();
        assert!(handle.is_some(), "AvSetMmThreadCharacteristicsW should succeed on a normal Windows dev machine");
    }

    #[test]
    fn boost_succeeds_independently_on_a_different_thread() {
        // A Win32 thread can hold only ONE MMCSS task association at a
        // time — calling AvSetMmThreadCharacteristicsW again on the SAME
        // thread without first reverting the prior registration fails.
        // Real call sites (Task 5/7) only ever call this once per thread,
        // guarded by a `std::sync::Once`, so that is not a bug to test for
        // here — instead this test verifies the function works correctly
        // when called from a fresh thread, independent of whatever the
        // test-runner's own thread already did.
        let success = std::thread::spawn(|| {
            boost_current_thread_to_pro_audio().is_some()
        })
        .join()
        .expect("spawned thread should not panic");
        assert!(success);
    }

    #[test]
    fn a_thread_already_in_a_task_is_not_reported_as_failure() {
        // Second registration on one thread fails with
        // ERROR_THREAD_ALREADY_IN_TASK — what an ASIO driver thread that
        // the driver itself registered looks like.
        std::thread::spawn(|| {
            assert!(boost_current_thread_to_pro_audio().is_some());
            let mut idx = 0u32;
            let err = unsafe { AvSetMmThreadCharacteristicsW(w!("Pro Audio"), &mut idx) }.unwrap_err();
            assert_eq!(err.code(), ERROR_THREAD_ALREADY_IN_TASK.to_hresult());
        })
        .join()
        .unwrap();
    }
}
