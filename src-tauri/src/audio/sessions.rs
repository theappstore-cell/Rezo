use serde::Serialize;

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AudioApp {
    pub pid: u32,
    pub name: String,
    pub exe: String,
    pub icon: Option<String>,
    pub active: bool,
}

#[cfg(windows)]
pub fn list_audio_apps() -> Vec<AudioApp> {
    imp::list().unwrap_or_else(|e| {
        log::warn!("énumération des sessions audio : {e}");
        vec![]
    })
}

#[cfg(not(windows))]
pub fn list_audio_apps() -> Vec<AudioApp> {
    vec![]
}

#[cfg(windows)]
pub fn exe_of_pid(pid: u32) -> Option<String> {
    imp::exe_of(pid)
}

#[cfg(windows)]
pub fn icon_of_exe(exe: &str) -> Option<String> {
    imp::icon_png_data_url(exe)
}

pub fn friendly_name(exe_path: &str) -> String {
    let stem = std::path::Path::new(exe_path)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| exe_path.to_string());
    match stem.to_lowercase().as_str() {
        "chrome" => "Google Chrome".into(),
        "msedge" => "Microsoft Edge".into(),
        "firefox" => "Firefox".into(),
        "spotify" => "Spotify".into(),
        "vlc" => "VLC".into(),
        "discord" => "Discord".into(),
        "wmplayer" => "Lecteur Windows Media".into(),
        "foobar2000" => "foobar2000".into(),
        _ => {
            let mut c = stem.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => stem,
            }
        }
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::collections::HashMap;

    use windows::core::{Interface, PWSTR};
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW, BITMAP, BITMAPINFO, BITMAPINFOHEADER,
        BI_RGB, DIB_RGB_COLORS,
    };
    use windows::Win32::Media::Audio::{
        eConsole, eRender, AudioSessionStateActive, IAudioSessionControl2, IAudioSessionManager2,
        IMMDeviceEnumerator, MMDeviceEnumerator,
    };
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
    };
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::Shell::ExtractIconExW;
    use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};

    pub(super) fn exe_of(pid: u32) -> Option<String> {
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let mut buf = [0u16; 520];
            let mut len = buf.len() as u32;
            let r = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
            let _ = CloseHandle(h);
            r.ok()?;
            Some(String::from_utf16_lossy(&buf[..len as usize]))
        }
    }

    fn process_table() -> HashMap<u32, u32> {
        let mut m = HashMap::new();
        unsafe {
            if let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
                let mut e = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
                if Process32FirstW(snap, &mut e).is_ok() {
                    loop {
                        m.insert(e.th32ProcessID, e.th32ParentProcessID);
                        if Process32NextW(snap, &mut e).is_err() {
                            break;
                        }
                    }
                }
                let _ = CloseHandle(snap);
            }
        }
        m
    }

    fn root_pid(pid: u32, exe: &str, table: &HashMap<u32, u32>) -> u32 {
        let mut cur = pid;
        for _ in 0..8 {
            match table.get(&cur).copied() {
                Some(parent) if parent != 0 && parent != cur && exe_of(parent).as_deref() == Some(exe) => cur = parent,
                _ => break,
            }
        }
        cur
    }

    pub fn list() -> anyhow::Result<Vec<AudioApp>> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            let dev = en.GetDefaultAudioEndpoint(eRender, eConsole)?;
            let mgr: IAudioSessionManager2 = dev.Activate(CLSCTX_ALL, None)?;
            let sessions = mgr.GetSessionEnumerator()?;
            let table = process_table();
            let mut apps: HashMap<u32, AudioApp> = HashMap::new();
            for i in 0..sessions.GetCount()? {
                let ctl: IAudioSessionControl2 = match sessions.GetSession(i)?.cast() {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                if ctl.IsSystemSoundsSession().0 == 0 {
                    continue;
                }
                let pid = match ctl.GetProcessId() {
                    Ok(p) if p != 0 => p,
                    _ => continue,
                };
                let Some(exe) = exe_of(pid) else { continue };
                let root = root_pid(pid, &exe, &table);
                let active = ctl.GetState().map(|s| s == AudioSessionStateActive).unwrap_or(false);
                let entry = apps.entry(root).or_insert_with(|| AudioApp {
                    pid: root,
                    name: friendly_name(&exe),
                    icon: icon_png_data_url(&exe),
                    exe: exe.clone(),
                    active: false,
                });
                entry.active |= active;
            }
            let mut v: Vec<AudioApp> = apps.into_values().collect();
            v.sort_by(|a, b| b.active.cmp(&a.active).then(a.name.cmp(&b.name)));
            Ok(v)
        }
    }

    pub(super) fn icon_png_data_url(exe: &str) -> Option<String> {
        use base64::Engine as _;
        unsafe {
            let wide: Vec<u16> = exe.encode_utf16().chain(std::iter::once(0)).collect();
            let mut large = HICON::default();
            if ExtractIconExW(windows::core::PCWSTR(wide.as_ptr()), 0, Some(&mut large), None, 1) == 0 || large.is_invalid() {
                return None;
            }
            let mut info = ICONINFO::default();
            let ok = GetIconInfo(large, &mut info).is_ok();
            let result = if ok && !info.hbmColor.is_invalid() {
                let mut bm = BITMAP::default();
                GetObjectW(info.hbmColor.into(), std::mem::size_of::<BITMAP>() as i32, Some(&mut bm as *mut _ as *mut _));
                let (w, h) = (bm.bmWidth, bm.bmHeight);
                if w <= 0 || h <= 0 {
                    None
                } else {
                    let mut bmi = BITMAPINFO::default();
                    bmi.bmiHeader = BITMAPINFOHEADER {
                        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                        biWidth: w,
                        biHeight: -h,
                        biPlanes: 1,
                        biBitCount: 32,
                        biCompression: BI_RGB.0,
                        ..Default::default()
                    };
                    let mut px = vec![0u8; (w * h * 4) as usize];
                    let dc = CreateCompatibleDC(None);
                    let lines = GetDIBits(dc, info.hbmColor, 0, h as u32, Some(px.as_mut_ptr() as *mut _), &mut bmi, DIB_RGB_COLORS);
                    let _ = DeleteDC(dc);
                    if lines == 0 {
                        None
                    } else {
                        let has_alpha = px.chunks_exact(4).any(|p| p[3] != 0);
                        for p in px.chunks_exact_mut(4) {
                            p.swap(0, 2);
                            if !has_alpha {
                                p[3] = 255;
                            }
                        }
                        let mut out = Vec::new();
                        let encoded = {
                            let mut enc = png::Encoder::new(&mut out, w as u32, h as u32);
                            enc.set_color(png::ColorType::Rgba);
                            enc.set_depth(png::BitDepth::Eight);
                            enc.write_header().ok().and_then(|mut wr| wr.write_image_data(&px).ok())
                        };
                        encoded.map(|_| format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(&out)))
                    }
                }
            } else {
                None
            };
            if !info.hbmColor.is_invalid() {
                let _ = DeleteObject(info.hbmColor.into());
            }
            if !info.hbmMask.is_invalid() {
                let _ = DeleteObject(info.hbmMask.into());
            }
            let _ = DestroyIcon(large);
            result
        }
    }
}
