// Mutes the default output while streaming and restores that exact endpoint by id (the default may change
// in between). WASAPI loopback taps the signal before the endpoint mute, so capture keeps working.
use anyhow::Result;

#[derive(Clone, Debug)]
pub struct Muted {
    pub device_id: String,
    pub prev: bool,
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::ffi::c_void;

    use windows::core::PCWSTR;
    use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
    use windows::Win32::Media::Audio::{eConsole, eRender, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator, DEVICE_STATE_ACTIVE};
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_MULTITHREADED};

    fn enumerator() -> Result<IMMDeviceEnumerator> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            Ok(CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?)
        }
    }

    fn id_of(d: &IMMDevice) -> Result<String> {
        unsafe {
            let p = d.GetId()?;
            let s = p.to_string().unwrap_or_default();
            CoTaskMemFree(Some(p.0 as *const c_void));
            Ok(s)
        }
    }

    fn device(en: &IMMDeviceEnumerator, id: &str) -> Result<IMMDevice> {
        let w: Vec<u16> = id.encode_utf16().chain(std::iter::once(0)).collect();
        let d = unsafe { en.GetDevice(PCWSTR(w.as_ptr()))? };
        let state = unsafe { d.GetState()? };
        if state != DEVICE_STATE_ACTIVE {
            anyhow::bail!("sortie absente (état {})", state.0);
        }
        Ok(d)
    }

    pub fn default_render_id() -> Result<String> {
        let en = enumerator()?;
        let d = unsafe { en.GetDefaultAudioEndpoint(eRender, eConsole)? };
        id_of(&d)
    }

    pub fn mute_default() -> Result<Muted> {
        let en = enumerator()?;
        unsafe {
            let d = en.GetDefaultAudioEndpoint(eRender, eConsole)?;
            let device_id = id_of(&d)?;
            let vol: IAudioEndpointVolume = d.Activate(CLSCTX_ALL, None)?;
            let prev = vol.GetMute()?.as_bool();
            vol.SetMute(true, std::ptr::null())?;
            Ok(Muted { device_id, prev })
        }
    }

    pub fn set_mute(device_id: &str, mute: bool) -> Result<()> {
        let en = enumerator()?;
        let d = device(&en, device_id)?;
        unsafe {
            let vol: IAudioEndpointVolume = d.Activate(CLSCTX_ALL, None)?;
            vol.SetMute(mute, std::ptr::null())?;
        }
        Ok(())
    }

    pub fn set_default_render_mute(mute: bool) -> Result<()> {
        let en = enumerator()?;
        unsafe {
            let d = en.GetDefaultAudioEndpoint(eRender, eConsole)?;
            let vol: IAudioEndpointVolume = d.Activate(CLSCTX_ALL, None)?;
            vol.SetMute(mute, std::ptr::null())?;
        }
        Ok(())
    }
}

#[cfg(windows)]
pub fn default_render_id() -> Result<String> {
    imp::default_render_id()
}

#[cfg(windows)]
pub fn mute_default() -> Result<Muted> {
    imp::mute_default()
}

#[cfg(windows)]
pub fn set_mute(device_id: &str, mute: bool) -> Result<()> {
    imp::set_mute(device_id, mute)
}

#[cfg(windows)]
pub fn set_default_render_mute(mute: bool) -> Result<()> {
    imp::set_default_render_mute(mute)
}

#[cfg(not(windows))]
pub fn default_render_id() -> Result<String> {
    anyhow::bail!("unavailable on this platform")
}
#[cfg(not(windows))]
pub fn mute_default() -> Result<Muted> {
    anyhow::bail!("Smart Output is only available on Windows")
}
#[cfg(not(windows))]
pub fn set_mute(_: &str, _: bool) -> Result<()> {
    Ok(())
}
#[cfg(not(windows))]
pub fn set_default_render_mute(_: bool) -> Result<()> {
    Ok(())
}
