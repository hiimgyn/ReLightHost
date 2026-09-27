use windows::core::PCWSTR;
use windows::Win32::Media::Multimedia::AvSetMmThreadCharacteristicsW;
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
    fn boost_does_not_panic_and_is_idempotent_per_call() {
        // Real MMCSS registration requires the Multimedia Class Scheduler
        // service to be running (it is, on every non-Server-Core Windows
        // install) — this exercises the real Win32 call rather than a mock,
        // since the whole point of this helper is the syscall succeeding.
        let handle1 = boost_current_thread_to_pro_audio();
        assert!(handle1.is_some(), "AvSetMmThreadCharacteristicsW should succeed on a normal Windows dev machine");
        let handle2 = boost_current_thread_to_pro_audio();
        assert!(handle2.is_some());
    }
}
