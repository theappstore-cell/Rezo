use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use tauri::window::Color;
use tauri::{AppHandle, LogicalSize, Manager, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent};

use crate::engine::Engine;

pub const COMPACT: (f64, f64) = (460.0, 680.0);
pub const WIDE: (f64, f64) = (1320.0, 780.0);
const DESTROY_AFTER: Duration = Duration::from_secs(30);

static WIDE_MODE: AtomicBool = AtomicBool::new(false);
static PINNED: AtomicBool = AtomicBool::new(false);
static HIDE_EPOCH: AtomicU64 = AtomicU64::new(0);
static ANCHOR: Mutex<Option<(f64, f64, f64)>> = Mutex::new(None);
static LAST_BLUR_HIDE: Mutex<Option<Instant>> = Mutex::new(None);
static LAST_SHOW: Mutex<Option<Instant>> = Mutex::new(None);

pub fn main_visible(app: &AppHandle) -> bool {
    app.get_webview_window("main").and_then(|w| w.is_visible().ok()).unwrap_or(false)
}

pub fn set_pinned(p: bool) {
    PINNED.store(p, Ordering::Relaxed);
}

fn ensure_window(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    if let Some(w) = app.get_webview_window("main") {
        return Ok(w);
    }
    let (w, h) = if WIDE_MODE.load(Ordering::Relaxed) { WIDE } else { COMPACT };
    let win = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
        .title("Rezo")
        .inner_size(w, h)
        .min_inner_size(420.0, 520.0)
        .decorations(false)
        .resizable(true)
        .visible(false)
        .skip_taskbar(!WIDE_MODE.load(Ordering::Relaxed))
        .shadow(true)
        .background_color(Color(241, 241, 238, 255))
        .build()?;
    let app2 = app.clone();
    win.on_window_event(move |ev| match ev {
        WindowEvent::Focused(false) => on_blur(&app2),
        WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            hide_main(&app2);
        }
        _ => {}
    });
    Ok(win)
}

fn on_blur(app: &AppHandle) {
    if WIDE_MODE.load(Ordering::Relaxed) || PINNED.load(Ordering::Relaxed) {
        return;
    }
    if LAST_SHOW.lock().map_or(false, |t| t.elapsed() < Duration::from_millis(1500)) {
        return;
    }
    *LAST_BLUR_HIDE.lock() = Some(Instant::now());
    hide_main(app);
}

fn place(win: &WebviewWindow) {
    let Ok(size) = win.outer_size() else { return };
    let (sw, sh) = (size.width as f64, size.height as f64);
    let anchor = *ANCHOR.lock();

    let pick = |x: f64, y: f64| {
        let mons = win.available_monitors().unwrap_or_default();
        mons.iter()
            .find(|m| {
                let (p, s) = (m.position(), m.size());
                x >= p.x as f64 && x < p.x as f64 + s.width as f64 && y >= p.y as f64 && y < p.y as f64 + s.height as f64
            })
            .cloned()
            .or_else(|| win.primary_monitor().ok().flatten())
    };

    let (cx, top, _) = anchor.unwrap_or((f64::NAN, f64::NAN, f64::NAN));
    let mon = if anchor.is_some() { pick(cx, top) } else { win.primary_monitor().ok().flatten() };
    let Some(mon) = mon else { return };
    let (mx, my) = (mon.position().x as f64, mon.position().y as f64);
    let (mw, mh) = (mon.size().width as f64, mon.size().height as f64);

    let (mut x, mut y) = match anchor {
        Some((cx, top, bottom)) => {
            let y = if top > my + mh / 2.0 { top - sh - 12.0 } else { bottom + 12.0 };
            (cx - sw / 2.0, y)
        }
        None => (mx + mw - sw - 16.0, my + mh - sh - 56.0),
    };
    x = x.clamp(mx + 8.0, (mx + mw - sw - 8.0).max(mx + 8.0));
    y = y.clamp(my + 8.0, (my + mh - sh - 8.0).max(my + 8.0));
    let _ = win.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
}

pub fn show_main(app: &AppHandle, anchor: Option<(f64, f64, f64)>) {
    HIDE_EPOCH.fetch_add(1, Ordering::AcqRel);
    *LAST_SHOW.lock() = Some(Instant::now());
    if let Some(a) = anchor {
        *ANCHOR.lock() = Some(a);
    }
    let Ok(win) = ensure_window(app) else {
        log::error!("création de la fenêtre impossible");
        return;
    };
    if WIDE_MODE.load(Ordering::Relaxed) {
        let _ = win.center();
    } else {
        place(&win);
    }
    let _ = win.show();
    let _ = win.unminimize();
    let _ = win.set_focus();
    set_visible(app, true);
}

fn set_visible(app: &AppHandle, visible: bool) {
    if let Some(e) = app.try_state::<Arc<Engine>>() {
        e.set_ui_visible(visible);
    }
}

pub fn toggle_main(app: &AppHandle, anchor: Option<(f64, f64, f64)>) {
    if LAST_BLUR_HIDE.lock().map_or(false, |t| t.elapsed() < Duration::from_millis(350)) {
        return;
    }
    if main_visible(app) {
        hide_main(app);
    } else {
        show_main(app, anchor);
    }
}

pub fn hide_main(app: &AppHandle) {
    let Some(win) = app.get_webview_window("main") else { return };
    let _ = win.hide();
    set_visible(app, false);
    let keep = app.try_state::<Arc<Engine>>().map_or(false, |e| e.store.read(|c| c.settings.keep_ui_alive));
    if keep {
        return;
    }
    let epoch = HIDE_EPOCH.fetch_add(1, Ordering::AcqRel) + 1;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(DESTROY_AFTER).await;
        if HIDE_EPOCH.load(Ordering::Acquire) == epoch {
            if let Some(w) = app.get_webview_window("main") {
                if !w.is_visible().unwrap_or(true) {
                    log::info!("UI masquée depuis 30 s : WebView détruite pour libérer la mémoire");
                    let _ = w.destroy();
                }
            }
        }
    });
}

pub fn set_mode(app: &AppHandle, wide: bool) {
    WIDE_MODE.store(wide, Ordering::Relaxed);
    let Some(win) = app.get_webview_window("main") else { return };
    let (w, h) = if wide { WIDE } else { COMPACT };
    let _ = win.set_skip_taskbar(!wide);
    let _ = win.set_size(LogicalSize::new(w, h));
    if wide {
        let _ = win.center();
    } else {
        place(&win);
    }
    let _ = win.set_focus();
}

pub fn open_mixer(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("mixer") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        return;
    }
    let built = WebviewWindowBuilder::new(app, "mixer", WebviewUrl::App("index.html".into()))
        .title("Rezo — Mixeur audio")
        .inner_size(780.0, 640.0)
        .min_inner_size(560.0, 420.0)
        .resizable(true)
        .center()
        .background_color(Color(241, 241, 238, 255))
        .build();
    if let Err(e) = built {
        log::error!("fenêtre du mixeur : {e}");
    }
}
