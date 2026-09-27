#![cfg(target_os = "windows")]

use windows::core::PCWSTR;
use windows::Win32::System::Threading::AvSetMmThreadCharacteristicsW;
use windows::Win32::Foundation::HANDLE;

/// Registers the calling thread with MMCSS under the "Pro Audio" task
/// profile, which raises its scheduling priority and reduces the chance
/// the OS scheduler preempts it mid-buffer — the difference between
/// needing a larger safety-margin buffer size and not.
///
/// Call once per real-time audio thread (ASIO callback thread, WASAPI
/// capture thread, WASAPI render thread), guarded by a `std::sync::Once`
/// at each call site so it only runs on the first callback invocation.
pub fn boost_current_thread_to_pro_audio() -> Option<HANDLE> {
    let name: Vec<u16> = "Pro Audio\0".encode_utf16().collect();
    let mut task_index: u32 = 0;
    let handle = unsafe {
        AvSetMmThreadCharacteristicsW(PCWSTR(name.as_ptr()), &mut task_index)
    };
    match handle {
        Ok(h) if !h.is_invalid() => Some(h),
        _ => {
            log::warn!("{} AvSetMmThreadCharacteristicsW(\"Pro Audio\") failed", crate::core::threading::thread_prefix("audio/mmcss"));
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
}
