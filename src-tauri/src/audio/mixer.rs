use serde::Serialize;

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct MixerDevice {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub volume: f32,
    pub mute: bool,
    pub default: bool,
    pub peak: f32,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct MixerApp {
    pub key: String,
    pub name: String,
    pub icon: Option<String>,
    pub device_id: String,
    pub device_name: String,
    pub volume: f32,
    pub mute: bool,
    pub active: bool,
    pub peak: f32,
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct MixerState {
    pub devices: Vec<MixerDevice>,
    pub apps: Vec<MixerApp>,
}

#[cfg(windows)]
pub fn snapshot() -> anyhow::Result<MixerState> {
    imp::snapshot()
}
#[cfg(windows)]
pub fn set_device_volume(device_id: &str, volume: f32) -> anyhow::Result<()> {
    imp::set_device_volume(device_id, volume)
}
#[cfg(windows)]
pub fn set_device_mute(device_id: &str, mute: bool) -> anyhow::Result<()> {
    imp::set_device_mute(device_id, mute)
}
#[cfg(windows)]
pub fn set_app_volume(device_id: &str, key: &str, volume: f32) -> anyhow::Result<()> {
    imp::set_app(device_id, key, Some(volume), None)
}
#[cfg(windows)]
pub fn set_app_mute(device_id: &str, key: &str, mute: bool) -> anyhow::Result<()> {
    imp::set_app(device_id, key, None, Some(mute))
}
#[cfg(windows)]
pub fn set_default_device(device_id: &str) -> anyhow::Result<()> {
    imp::set_default(device_id)
}

#[cfg(not(windows))]
pub fn snapshot() -> anyhow::Result<MixerState> {
    Ok(MixerState::default())
}
#[cfg(not(windows))]
pub fn set_device_volume(_: &str, _: f32) -> anyhow::Result<()> {
    Ok(())
}
#[cfg(not(windows))]
pub fn set_device_mute(_: &str, _: bool) -> anyhow::Result<()> {
    Ok(())
}
#[cfg(not(windows))]
pub fn set_app_volume(_: &str, _: &str, _: f32) -> anyhow::Result<()> {
    Ok(())
}
#[cfg(not(windows))]
pub fn set_app_mute(_: &str, _: &str, _: bool) -> anyhow::Result<()> {
    Ok(())
}
#[cfg(not(windows))]
pub fn set_default_device(_: &str) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::collections::HashMap;
    use std::ffi::c_void;
    use std::sync::OnceLock;

    use anyhow::{anyhow, Result};
    use parking_lot::Mutex;
    use windows::core::{Interface, GUID, HRESULT, PCWSTR, PWSTR};
    use windows::Win32::Media::Audio::Endpoints::{IAudioEndpointVolume, IAudioMeterInformation};
    use windows::Win32::Media::Audio::{
        eConsole, eRender, AudioSessionStateActive, AudioSessionStateExpired, IAudioSessionControl2,
        IAudioSessionManager2, IMMDevice, IMMDeviceEnumerator, ISimpleAudioVolume, MMDeviceEnumerator,
        DEVICE_STATE_ACTIVE,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_MULTITHREADED, STGM_READ,
    };
    use windows::Win32::System::Com::StructuredStorage::{PropVariantToStringAlloc, PropVariantToUInt32};
    use windows::Win32::UI::Shell::PropertiesSystem::PROPERTYKEY;

    use crate::audio::sessions::{exe_of_pid, friendly_name, icon_of_exe};

    const DEVICE_PROPS: GUID = GUID::from_u128(0xa45c254e_df1c_4efd_8020_67d146a850e0);
    const KEY_FRIENDLY_NAME: PROPERTYKEY = PROPERTYKEY { fmtid: DEVICE_PROPS, pid: 14 };
    const KEY_ENUMERATOR: PROPERTYKEY = PROPERTYKEY { fmtid: DEVICE_PROPS, pid: 24 };
    const KEY_FORM_FACTOR: PROPERTYKEY =
        PROPERTYKEY { fmtid: GUID::from_u128(0x1da5d803_d492_4edd_8c23_e0c0ffee7f0e), pid: 0 };

    const SYSTEM_KEY: &str = "@system";

    fn enumerator() -> Result<IMMDeviceEnumerator> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            Ok(CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?)
        }
    }

    fn take_string(p: PWSTR) -> String {
        unsafe {
            let s = p.to_string().unwrap_or_default();
            CoTaskMemFree(Some(p.0 as *const c_void));
            s
        }
    }

    fn device_id(d: &IMMDevice) -> Result<String> {
        unsafe { Ok(take_string(d.GetId()?)) }
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn device_by_id(en: &IMMDeviceEnumerator, id: &str) -> Result<IMMDevice> {
        let w = wide(id);
        unsafe { Ok(en.GetDevice(PCWSTR(w.as_ptr()))?) }
    }

    fn describe(d: &IMMDevice) -> (String, String, bool) {
        unsafe {
            let Ok(store) = d.OpenPropertyStore(STGM_READ) else {
                return ("Périphérique audio".into(), "other".into(), false);
            };
            let name = store
                .GetValue(&KEY_FRIENDLY_NAME)
                .ok()
                .and_then(|pv| PropVariantToStringAlloc(&pv).ok())
                .map(take_string)
                .unwrap_or_else(|| "Périphérique audio".into());
            let enumerator = store
                .GetValue(&KEY_ENUMERATOR)
                .ok()
                .and_then(|pv| PropVariantToStringAlloc(&pv).ok())
                .map(take_string)
                .unwrap_or_default();
            let form = store.GetValue(&KEY_FORM_FACTOR).ok().and_then(|pv| PropVariantToUInt32(&pv).ok());
            let up = enumerator.to_uppercase();
            let kind = if up.starts_with("BTH") {
                "bluetooth"
            } else if matches!(form, Some(7 | 8 | 9)) {
                "hdmi"
            } else if form == Some(0) {
                "network"
            } else if up == "USB" {
                "usb"
            } else if matches!(form, Some(1 | 2 | 3 | 5)) {
                "wired"
            } else {
                "other"
            };
            (name, kind.to_string(), up == "BTHHFENUM")
        }
    }

    fn icon_cached(exe: &str) -> Option<String> {
        static CACHE: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();
        let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
        if let Some(v) = cache.lock().get(exe) {
            return v.clone();
        }
        let v = icon_of_exe(exe);
        cache.lock().insert(exe.to_string(), v.clone());
        v
    }

    fn session_key(ctl: &IAudioSessionControl2) -> Option<String> {
        unsafe {
            if ctl.IsSystemSoundsSession().0 == 0 {
                return Some(SYSTEM_KEY.into());
            }
            let pid = ctl.GetProcessId().ok().filter(|p| *p != 0)?;
            Some(exe_of_pid(pid).unwrap_or_else(|| format!("pid:{pid}")))
        }
    }

    pub fn snapshot() -> Result<MixerState> {
        let en = enumerator()?;
        let own_exe = std::env::current_exe().ok().map(|p| p.to_string_lossy().to_lowercase());
        unsafe {
            let default_id = en.GetDefaultAudioEndpoint(eRender, eConsole).ok().and_then(|d| device_id(&d).ok());
            let coll = en.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)?;
            let mut state = MixerState::default();
            for i in 0..coll.GetCount()? {
                let Ok(dev) = coll.Item(i) else { continue };
                let Ok(id) = device_id(&dev) else { continue };
                let (name, kind, hands_free) = describe(&dev);
                if hands_free {
                    continue;
                }
                let vol: Option<IAudioEndpointVolume> = dev.Activate(CLSCTX_ALL, None).ok();
                let meter: Option<IAudioMeterInformation> = dev.Activate(CLSCTX_ALL, None).ok();
                state.devices.push(MixerDevice {
                    default: default_id.as_deref() == Some(id.as_str()),
                    volume: vol.as_ref().and_then(|v| v.GetMasterVolumeLevelScalar().ok()).unwrap_or(0.0),
                    mute: vol.as_ref().and_then(|v| v.GetMute().ok()).map(|b| b.as_bool()).unwrap_or(false),
                    peak: meter.as_ref().and_then(|m| m.GetPeakValue().ok()).unwrap_or(0.0),
                    id: id.clone(),
                    name: name.clone(),
                    kind,
                });

                let Ok(mgr) = dev.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) else { continue };
                let Ok(sessions) = mgr.GetSessionEnumerator() else { continue };
                let mut rows: HashMap<String, MixerApp> = HashMap::new();
                for k in 0..sessions.GetCount().unwrap_or(0) {
                    let Ok(ctl) = sessions.GetSession(k) else { continue };
                    let Ok(ctl2) = ctl.cast::<IAudioSessionControl2>() else { continue };
                    let state_now = ctl.GetState().ok();
                    if state_now == Some(AudioSessionStateExpired) {
                        continue;
                    }
                    let Some(key) = session_key(&ctl2) else { continue };
                    if key == SYSTEM_KEY && default_id.as_deref() != Some(id.as_str()) {
                        continue;
                    }
                    if own_exe.as_deref() == Some(key.to_lowercase().as_str()) {
                        continue;
                    }
                    let simple = ctl.cast::<ISimpleAudioVolume>().ok();
                    let peak = ctl.cast::<IAudioMeterInformation>().ok().and_then(|m| m.GetPeakValue().ok()).unwrap_or(0.0);
                    let row = rows.entry(key.clone()).or_insert_with(|| MixerApp {
                        name: if key == SYSTEM_KEY {
                            "Sons système Windows".into()
                        } else if key.starts_with("pid:") {
                            format!("Processus {}", &key[4..])
                        } else {
                            friendly_name(&key)
                        },
                        icon: if key == SYSTEM_KEY || key.starts_with("pid:") { None } else { icon_cached(&key) },
                        key: key.clone(),
                        device_id: id.clone(),
                        device_name: name.clone(),
                        volume: simple.as_ref().and_then(|s| s.GetMasterVolume().ok()).unwrap_or(1.0),
                        mute: simple.as_ref().and_then(|s| s.GetMute().ok()).map(|b| b.as_bool()).unwrap_or(false),
                        active: false,
                        peak: 0.0,
                    });
                    row.active |= state_now == Some(AudioSessionStateActive);
                    row.peak = row.peak.max(peak);
                }
                let mut v: Vec<MixerApp> = rows.into_values().collect();
                v.sort_by(|a, b| b.active.cmp(&a.active).then(a.name.to_lowercase().cmp(&b.name.to_lowercase())));
                state.apps.extend(v);
            }
            Ok(state)
        }
    }

    pub fn set_device_volume(device_id: &str, volume: f32) -> Result<()> {
        let en = enumerator()?;
        let dev = device_by_id(&en, device_id)?;
        unsafe {
            let vol: IAudioEndpointVolume = dev.Activate(CLSCTX_ALL, None)?;
            vol.SetMasterVolumeLevelScalar(volume.clamp(0.0, 1.0), std::ptr::null())?;
        }
        Ok(())
    }

    pub fn set_device_mute(device_id: &str, mute: bool) -> Result<()> {
        let en = enumerator()?;
        let dev = device_by_id(&en, device_id)?;
        unsafe {
            let vol: IAudioEndpointVolume = dev.Activate(CLSCTX_ALL, None)?;
            vol.SetMute(mute, std::ptr::null())?;
        }
        Ok(())
    }

    pub fn set_app(device_id: &str, key: &str, volume: Option<f32>, mute: Option<bool>) -> Result<()> {
        let en = enumerator()?;
        let dev = device_by_id(&en, device_id)?;
        let mut hit = false;
        unsafe {
            let mgr: IAudioSessionManager2 = dev.Activate(CLSCTX_ALL, None)?;
            let sessions = mgr.GetSessionEnumerator()?;
            for k in 0..sessions.GetCount()? {
                let Ok(ctl) = sessions.GetSession(k) else { continue };
                let Ok(ctl2) = ctl.cast::<IAudioSessionControl2>() else { continue };
                if session_key(&ctl2).as_deref() != Some(key) {
                    continue;
                }
                let Ok(simple) = ctl.cast::<ISimpleAudioVolume>() else { continue };
                if let Some(v) = volume {
                    simple.SetMasterVolume(v.clamp(0.0, 1.0), std::ptr::null())?;
                }
                if let Some(m) = mute {
                    simple.SetMute(m, std::ptr::null())?;
                }
                hit = true;
            }
        }
        if hit {
            Ok(())
        } else {
            Err(anyhow!("l'application n'émet plus de son sur ce périphérique"))
        }
    }

    const CLSID_POLICY_CONFIG: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);
    const IID_POLICY_CONFIG: GUID = GUID::from_u128(0xf8679f50_850a_41cf_9c72_430f290290c8);

    // IPolicyConfig is undocumented but stable since Vista; only SetDefaultEndpoint is called.
    #[repr(C)]
    struct PolicyConfigVtbl {
        _query_interface: usize,
        _add_ref: usize,
        release: unsafe extern "system" fn(*mut c_void) -> u32,
        _unused: [usize; 10],
        set_default_endpoint: unsafe extern "system" fn(*mut c_void, PCWSTR, i32) -> HRESULT,
        _set_endpoint_visibility: usize,
    }

    pub fn set_default(device_id: &str) -> Result<()> {
        let w = wide(device_id);
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let unk: windows::core::IUnknown = CoCreateInstance(&CLSID_POLICY_CONFIG, None, CLSCTX_ALL)?;
            let mut raw: *mut c_void = std::ptr::null_mut();
            unk.query(&IID_POLICY_CONFIG, &mut raw).ok()?;
            if raw.is_null() {
                return Err(anyhow!("IPolicyConfig indisponible"));
            }
            let vtbl = *(raw as *mut *const PolicyConfigVtbl);
            let mut result = Ok(());
            for role in 0..3 {
                let hr = ((*vtbl).set_default_endpoint)(raw, PCWSTR(w.as_ptr()), role);
                if hr.is_err() {
                    result = Err(anyhow!("changement de périphérique par défaut refusé ({hr:?})"));
                    break;
                }
            }
            ((*vtbl).release)(raw);
            result
        }
    }
}

#[cfg(all(test, windows))]
mod tests {
    #[test]
    fn snapshot_reads_real_devices() {
        let s = super::snapshot().expect("énumération des sorties");
        for d in &s.devices {
            println!("sortie : {} [{}] vol={:.2} mute={} défaut={}", d.name, d.kind, d.volume, d.mute, d.default);
            assert!((0.0..=1.0).contains(&d.volume));
        }
        for a in &s.apps {
            println!("appli : {} sur {} vol={:.2} actif={}", a.name, a.device_name, a.volume, a.active);
        }
    }
}
