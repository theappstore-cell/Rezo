use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use tauri::window::Color;
use tauri::{AppHandle, LogicalSize, Manager, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent};

use crate::engine::Engine;
use crate::state::rooms::Widget;

pub const COMPACT: (f64, f64) = (460.0, 680.0);
pub const WIDE: (f64, f64) = (1320.0, 780.0);
const DESTROY_AFTER: Duration = Duration::from_secs(30);
const MIN_SIZE: (f64, f64) = (360.0, 480.0);

static WIDE_MODE: AtomicBool = AtomicBool::new(false);
static PINNED: AtomicBool = AtomicBool::new(false);
static HIDE_EPOCH: AtomicU64 = AtomicU64::new(0);
static ANCHOR: Mutex<Option<(f64, f64, f64)>> = Mutex::new(None);
static LAST_BLUR_HIDE: Mutex<Option<Instant>> = Mutex::new(None);
static LAST_SHOW: Mutex<Option<Instant>> = Mutex::new(None);

pub fn main_visible(app: &AppHandle) -> bool {
    app.get_webview_window("main").and_then(|w| w.is_visible().ok()).unwrap_or(false)
}

fn engine(app: &AppHandle) -> Option<Arc<Engine>> {
    app.try_state::<Arc<Engine>>().map(|e| e.inner().clone())
}

pub fn init_pinned(p: bool) {
    PINNED.store(p, Ordering::Relaxed);
}

// Pinned: the main window stays above the other windows and no longer hides when it loses focus.
pub fn set_pinned(app: &AppHandle, p: bool) {
    PINNED.store(p, Ordering::Relaxed);
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.set_always_on_top(p);
        if p {
            let _ = w.set_focus();
        }
    }
}

// Interface size (browser zoom of the main window); the minimum window size grows with it.
pub fn apply_zoom(app: &AppHandle, zoom: f32) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.set_zoom(zoom as f64);
        let _ = w.set_min_size(Some(LogicalSize::new(MIN_SIZE.0 * zoom as f64, MIN_SIZE.1 * zoom as f64)));
    }
}

fn ensure_window(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    if let Some(w) = app.get_webview_window("main") {
        return Ok(w);
    }
    let (w, h) = if WIDE_MODE.load(Ordering::Relaxed) { WIDE } else { COMPACT };
    let win = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
        .title("Rezo")
        .inner_size(w, h)
        .min_inner_size(MIN_SIZE.0, MIN_SIZE.1)
        .decorations(false)
        .resizable(true)
        .visible(false)
        .skip_taskbar(!WIDE_MODE.load(Ordering::Relaxed))
        .always_on_top(PINNED.load(Ordering::Relaxed))
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
    if let Some(z) = engine(app).map(|e| e.store.read(|c| c.ui_zoom)).filter(|z| (*z - 1.0).abs() > 0.01) {
        apply_zoom(app, z);
    }
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
    if let Some(e) = engine(app) {
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
    let keep = engine(app).map_or(false, |e| e.store.read(|c| c.settings.keep_ui_alive));
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

// ---- Desktop widgets: one small borderless, transparent window per widget ----

pub const WIDGET_PREFIX: &str = "widget-";

// Size at scale 1 before the page measures its content and asks for its exact size.
fn widget_base_size(kind: &str) -> (f64, f64) {
    match kind {
        "player" => (330.0, 96.0),
        "volume" => (280.0, 84.0),
        "rooms" => (240.0, 180.0),
        "scenes" => (260.0, 110.0),
        _ => (240.0, 70.0),
    }
}

// Opens, updates or closes the widget windows so that they match the configuration.
pub fn sync_widgets(app: &AppHandle) {
    let Some(engine) = engine(app) else { return };
    let widgets = engine.widgets();
    for (label, win) in app.webview_windows() {
        if let Some(id) = label.strip_prefix(WIDGET_PREFIX) {
            if !widgets.iter().any(|w| w.id == id && w.visible) {
                let _ = win.destroy();
            }
        }
    }
    for (i, w) in widgets.iter().filter(|w| w.visible).enumerate() {
        match app.get_webview_window(&format!("{WIDGET_PREFIX}{}", w.id)) {
            Some(win) => apply_widget(&win, w),
            None => {
                if let Err(e) = build_widget(app, &engine, w, i) {
                    log::error!("widget {} : {e}", w.id);
                }
            }
        }
    }
    engine.set_widget_meters(widgets.iter().any(|w| w.visible && w.kind == "meter"));
}

fn apply_widget(win: &WebviewWindow, w: &Widget) {
    let top = w.layer == "top";
    let bottom = w.layer == "bottom" || w.layer == "desktop";
    let _ = win.set_always_on_top(top);
    let _ = win.set_always_on_bottom(bottom);
    #[cfg(windows)]
    desktop::attach(win, w.layer == "desktop");
    let _ = win.set_ignore_cursor_events(w.click_through);
}

fn build_widget(app: &AppHandle, engine: &Arc<Engine>, w: &Widget, index: usize) -> tauri::Result<()> {
    let (bw, bh) = widget_base_size(&w.kind);
    let scale = w.scale as f64;
    let builder = WebviewWindowBuilder::new(app, format!("{WIDGET_PREFIX}{}", w.id), WebviewUrl::App("index.html".into()))
        .title("Rezo")
        .inner_size(bw * scale, bh * scale)
        .decorations(false)
        .resizable(false)
        .skip_taskbar(true)
        .shadow(false)
        .focused(false)
        .visible(false);
    #[cfg(not(target_os = "macos"))]
    let builder = builder.transparent(true).background_color(Color(0, 0, 0, 0));
    let win = builder.build()?;

    let size = win.outer_size().unwrap_or_default();
    let saved = w.x.zip(w.y).filter(|(x, y)| on_screen(&win, *x + 16, *y + 16));
    let (x, y) = saved.unwrap_or_else(|| default_widget_position(&win, size.width as i32, size.height as i32, index));
    let _ = win.set_position(PhysicalPosition::new(x, y));
    apply_widget(&win, w);

    let id = w.id.clone();
    let eng = engine.clone();
    win.on_window_event(move |ev| {
        if let WindowEvent::Moved(p) = ev {
            eng.widget_moved(&id, p.x, p.y);
        }
    });
    let _ = win.show();
    Ok(())
}

fn on_screen(win: &WebviewWindow, x: i32, y: i32) -> bool {
    win.available_monitors().unwrap_or_default().iter().any(|m| {
        let (p, s) = (m.position(), m.size());
        x >= p.x && y >= p.y && x < p.x + s.width as i32 && y < p.y + s.height as i32
    })
}

fn default_widget_position(win: &WebviewWindow, w: i32, h: i32, index: usize) -> (i32, i32) {
    let Some(mon) = win.primary_monitor().ok().flatten() else { return (40, 40 + index as i32 * (h + 12)) };
    let (p, s) = (mon.position(), mon.size());
    let x = p.x + s.width as i32 - w - 24;
    let y = p.y + s.height as i32 - h - 72 - index as i32 * (h + 12);
    (x, y.max(p.y + 8))
}

// The page measured its content: fit the window to it (logical pixels, bounded).
pub fn fit_widget(app: &AppHandle, id: &str, width: f64, height: f64) {
    if let Some(win) = app.get_webview_window(&format!("{WIDGET_PREFIX}{id}")) {
        let w = width.clamp(80.0, 900.0).ceil();
        let h = height.clamp(32.0, 900.0).ceil();
        let _ = win.set_size(LogicalSize::new(w, h));
    }
}

// Tray "Desktop widgets": hide them all, or show them all again.
pub fn toggle_all_widgets(app: &AppHandle) {
    let Some(engine) = engine(app) else { return };
    let any = engine.widgets().iter().any(|w| w.visible);
    for w in engine.widgets() {
        let _ = engine.widget_update(&w.id, &serde_json::json!({ "visible": !any }));
    }
    sync_widgets(app);
}

#[cfg(windows)]
mod desktop {
    use tauri::WebviewWindow;
    use windows::core::{w, BOOL, PCWSTR};
    use windows::Win32::Foundation::{HWND, LPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{EnumWindows, FindWindowExW, FindWindowW, SetWindowLongPtrW, GWLP_HWNDPARENT};

    // The window that hosts the desktop icons: Progman, or a WorkerW once Windows has animated the wallpaper.
    fn host() -> Option<HWND> {
        unsafe extern "system" fn each(hwnd: HWND, lp: LPARAM) -> BOOL {
            if FindWindowExW(Some(hwnd), None, w!("SHELLDLL_DefView"), PCWSTR::null()).is_ok() {
                *(lp.0 as *mut Option<HWND>) = Some(hwnd);
                return BOOL(0);
            }
            BOOL(1)
        }
        unsafe {
            let progman = FindWindowW(w!("Progman"), PCWSTR::null()).ok()?;
            if FindWindowExW(Some(progman), None, w!("SHELLDLL_DefView"), PCWSTR::null()).is_ok() {
                return Some(progman);
            }
            let mut found: Option<HWND> = None;
            let _ = EnumWindows(Some(each), LPARAM(&mut found as *mut Option<HWND> as isize));
            found.or(Some(progman))
        }
    }

    // "On the desktop": owned by the desktop window, so it stays visible when "Show desktop" (Win+D) hides the
    // other windows, and it is kept at the bottom of the stack.
    pub fn attach(win: &WebviewWindow, on: bool) {
        let Ok(h) = win.hwnd() else { return };
        let hwnd = HWND(h.0 as _);
        let owner = if on { host() } else { None };
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_HWNDPARENT, owner.map_or(0, |o| o.0 as isize));
        }
    }
}
