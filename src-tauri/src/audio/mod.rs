pub mod capture;
pub mod dsp;
pub mod encoder;
pub mod mixer;
pub mod output_switch;
pub mod router;
pub mod sessions;

use std::sync::atomic::{AtomicU32, Ordering};

pub const SAMPLE_RATE: u32 = 48_000;
pub const CHANNELS: usize = 2;
pub const BLOCK_FRAMES: usize = 120;
pub const BLOCK_SAMPLES: usize = BLOCK_FRAMES * CHANNELS;

#[derive(Debug, Default)]
pub struct AtomicF32(AtomicU32);

impl AtomicF32 {
    #[inline]
    pub fn load(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Relaxed))
    }
    #[inline]
    pub fn store(&self, v: f32) {
        self.0.store(v.to_bits(), Ordering::Relaxed)
    }
    #[inline]
    pub fn fetch_max_pos(&self, v: f32) {
        self.0.fetch_max(v.to_bits(), Ordering::Relaxed);
    }
    #[inline]
    pub fn take(&self) -> f32 {
        f32::from_bits(self.0.swap(0, Ordering::Relaxed))
    }
}

// Held only while streaming: while an MMCSS task is registered, Windows may throttle other apps' network traffic.
pub struct MmcssGuard {
    #[cfg(windows)]
    handle: windows::Win32::Foundation::HANDLE,
}

impl Drop for MmcssGuard {
    fn drop(&mut self) {
        #[cfg(windows)]
        unsafe {
            let _ = windows::Win32::System::Threading::AvRevertMmThreadCharacteristics(self.handle);
        }
    }
}

#[cfg(windows)]
#[must_use = "la classe MMCSS est retirée dès que la garde est détruite"]
pub fn promote_current_thread() -> Option<MmcssGuard> {
    use windows::core::w;
    use windows::Win32::System::Threading::AvSetMmThreadCharacteristicsW;
    let mut task_index = 0u32;
    match unsafe { AvSetMmThreadCharacteristicsW(w!("Pro Audio"), &mut task_index) } {
        Ok(handle) => Some(MmcssGuard { handle }),
        Err(e) => {
            log::warn!("MMCSS Pro Audio indisponible : {e}");
            None
        }
    }
}

#[cfg(not(windows))]
pub fn promote_current_thread() -> Option<MmcssGuard> {
    None
}

#[cfg(windows)]
pub fn set_timer_resolution(on: bool) {
    use std::sync::atomic::AtomicBool;
    static ACTIVE: AtomicBool = AtomicBool::new(false);
    if ACTIVE.swap(on, Ordering::AcqRel) == on {
        return;
    }
    unsafe {
        if on {
            windows::Win32::Media::timeBeginPeriod(1);
        } else {
            windows::Win32::Media::timeEndPeriod(1);
        }
    }
}

#[cfg(not(windows))]
pub fn set_timer_resolution(_on: bool) {}
