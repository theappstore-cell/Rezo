// Notification-area icon and its menu: media keys, rooms, scenes, master volume and widgets without opening the window.
use std::sync::Arc;

use parking_lot::Mutex;
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

use crate::engine::Engine;
use crate::ui;

const VOLUME_STEP_DB: f32 = 3.0;

// What the menu shows, to rebuild it only when something visible changed.
static LAST: Mutex<String> = Mutex::new(String::new());

struct Labels {
    open: &'static str,
    play: &'static str,
    next: &'static str,
    previous: &'static str,
    rooms: &'static str,
    no_rooms: &'static str,
    scenes: &'static str,
    no_scenes: &'static str,
    volume: &'static str,
    up: &'static str,
    down: &'static str,
    mute: &'static str,
    widgets: &'static str,
    mixer: &'static str,
    quit: &'static str,
}

fn labels(lang: &str) -> Labels {
    if lang == "en" {
        Labels {
            open: "Open Rezo",
            play: "Play / pause",
            next: "Next track",
            previous: "Previous track",
            rooms: "Rooms",
            no_rooms: "No speaker in a room",
            scenes: "Scenes",
            no_scenes: "No saved mix",
            volume: "Master volume",
            up: "Louder (+3 dB)",
            down: "Quieter (-3 dB)",
            mute: "Mute all speakers",
            widgets: "Desktop widgets",
            mixer: "Audio mixer…",
            quit: "Quit",
        }
    } else {
        Labels {
            open: "Ouvrir Rezo",
            play: "Lecture / pause",
            next: "Piste suivante",
            previous: "Piste précédente",
            rooms: "Pièces",
            no_rooms: "Aucune enceinte dans une pièce",
            scenes: "Scènes",
            no_scenes: "Aucun mix enregistré",
            volume: "Volume général",
            up: "Plus fort (+3 dB)",
            down: "Moins fort (-3 dB)",
            mute: "Couper toutes les enceintes",
            widgets: "Widgets de bureau",
            mixer: "Mixeur audio…",
            quit: "Quitter",
        }
    }
}

struct MenuModel {
    lang: String,
    rooms: Vec<(String, String, bool)>,
    scenes: Vec<(String, String)>,
    master_mute: bool,
    widgets: usize,
    widgets_shown: bool,
}

fn model(engine: &Engine) -> MenuModel {
    engine.store.read(|c| MenuModel {
        lang: c.settings.language.clone(),
        rooms: c
            .rooms
            .iter()
            .filter(|r| c.speakers.iter().any(|s| s.room_id == r.id))
            .map(|r| {
                let on = c.speakers.iter().any(|s| s.room_id == r.id && s.active);
                (r.id.clone(), room_name(&r.id, &r.name, &c.settings.language), on)
            })
            .collect(),
        scenes: c.scenes.iter().map(|s| (s.id.clone(), s.name.clone())).collect(),
        master_mute: c.master_mute,
        widgets: c.widgets.len(),
        widgets_shown: c.widgets.iter().any(|w| w.visible),
    })
}

// Rooms created on first launch keep their French name until renamed, as in the interface.
fn room_name(id: &str, name: &str, lang: &str) -> String {
    const DEFAULTS: [(&str, &str, &str); 5] = [
        ("salon", "Salon", "Living room"),
        ("cuisine", "Cuisine", "Kitchen"),
        ("bureau", "Bureau", "Office"),
        ("chambre", "Chambre", "Bedroom"),
        ("terrasse", "Terrasse", "Terrace"),
    ];
    match DEFAULTS.iter().find(|(i, fr, _)| *i == id && *fr == name) {
        Some((_, _, en)) if lang == "en" => en.to_string(),
        _ => name.to_string(),
    }
}

fn signature(m: &MenuModel) -> String {
    format!("{}|{:?}|{:?}|{}|{}|{}", m.lang, m.rooms, m.scenes, m.master_mute, m.widgets, m.widgets_shown)
}

fn make_menu(app: &AppHandle, m: &MenuModel) -> tauri::Result<Menu<tauri::Wry>> {
    let l = labels(&m.lang);
    let menu = Menu::new(app)?;
    menu.append(&MenuItem::with_id(app, "show", l.open, true, None::<&str>)?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    if cfg!(windows) {
        menu.append(&MenuItem::with_id(app, "media:toggle", l.play, true, None::<&str>)?)?;
        menu.append(&MenuItem::with_id(app, "media:next", l.next, true, None::<&str>)?)?;
        menu.append(&MenuItem::with_id(app, "media:previous", l.previous, true, None::<&str>)?)?;
        menu.append(&PredefinedMenuItem::separator(app)?)?;
    }

    let rooms = Submenu::new(app, l.rooms, true)?;
    if m.rooms.is_empty() {
        rooms.append(&MenuItem::new(app, l.no_rooms, false, None::<&str>)?)?;
    }
    for (id, name, on) in &m.rooms {
        rooms.append(&CheckMenuItem::with_id(app, format!("room:{id}"), name, true, *on, None::<&str>)?)?;
    }
    menu.append(&rooms)?;

    let scenes = Submenu::new(app, l.scenes, true)?;
    if m.scenes.is_empty() {
        scenes.append(&MenuItem::new(app, l.no_scenes, false, None::<&str>)?)?;
    }
    for (id, name) in &m.scenes {
        scenes.append(&MenuItem::with_id(app, format!("scene:{id}"), name, true, None::<&str>)?)?;
    }
    menu.append(&scenes)?;

    let volume = Submenu::new(app, l.volume, true)?;
    volume.append(&MenuItem::with_id(app, "vol:up", l.up, true, None::<&str>)?)?;
    volume.append(&MenuItem::with_id(app, "vol:down", l.down, true, None::<&str>)?)?;
    volume.append(&CheckMenuItem::with_id(app, "vol:mute", l.mute, true, m.master_mute, None::<&str>)?)?;
    menu.append(&volume)?;

    menu.append(&PredefinedMenuItem::separator(app)?)?;
    if m.widgets > 0 {
        menu.append(&CheckMenuItem::with_id(app, "widgets", l.widgets, true, m.widgets_shown, None::<&str>)?)?;
    }
    if cfg!(windows) {
        menu.append(&MenuItem::with_id(app, "mixer", l.mixer, true, None::<&str>)?)?;
    }
    menu.append(&MenuItem::with_id(app, "quit", l.quit, true, None::<&str>)?)?;
    Ok(menu)
}

pub fn build(app: &AppHandle, engine: &Engine) -> tauri::Result<()> {
    let m = model(engine);
    *LAST.lock() = signature(&m);
    TrayIconBuilder::with_id("main")
        .icon(Image::from_bytes(include_bytes!("../icons/tray.png"))?)
        .tooltip("Rezo")
        .menu(&make_menu(app, &m)?)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, ev| on_menu(app, ev.id.as_ref()))
        .on_tray_icon_event(|tray, ev| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, rect, .. } = ev {
                let p = rect.position.to_physical::<f64>(1.0);
                let s = rect.size.to_physical::<f64>(1.0);
                ui::toggle_main(tray.app_handle(), Some((p.x + s.width / 2.0, p.y, p.y + s.height)));
            }
        })
        .build(app)?;
    Ok(())
}

// Rebuilds the menu when rooms, scenes, mute or widgets changed (or always with `force`, after a language change).
pub fn refresh(app: &AppHandle, engine: &Engine, force: bool) {
    let m = model(engine);
    let sig = signature(&m);
    {
        let mut last = LAST.lock();
        if !force && *last == sig {
            return;
        }
        *last = sig;
    }
    if let (Some(tray), Ok(menu)) = (app.tray_by_id("main"), make_menu(app, &m)) {
        let _ = tray.set_menu(Some(menu));
    }
}

fn on_menu(app: &AppHandle, id: &str) {
    let Some(engine) = app.try_state::<Arc<Engine>>().map(|e| e.inner().clone()) else { return };
    match id {
        "show" => ui::show_main(app, None),
        "mixer" => ui::open_mixer(app),
        "quit" => app.exit(0),
        "vol:up" | "vol:down" => {
            let step = if id == "vol:up" { VOLUME_STEP_DB } else { -VOLUME_STEP_DB };
            let db = engine.store.read(|c| c.master_db);
            engine.set_master(((db + step) * 2.0).round() / 2.0);
        }
        "vol:mute" => {
            let muted = engine.store.read(|c| c.master_mute);
            engine.set_master_mute(!muted);
        }
        "widgets" => ui::toggle_all_widgets(app),
        _ => {
            if let Some(cmd) = id.strip_prefix("media:") {
                let cmd = cmd.to_string();
                tauri::async_runtime::spawn_blocking(move || {
                    if let Err(e) = crate::media::command(&cmd, None) {
                        log::warn!("commande média {cmd} : {e}");
                    }
                });
            } else if let Some(room) = id.strip_prefix("room:") {
                let room = room.to_string();
                tauri::async_runtime::spawn(async move {
                    let on = !model(&engine).rooms.iter().any(|(r, _, on)| *r == room && *on);
                    engine.set_room_send(&room, on).await;
                    engine.notify_changed();
                });
            } else if let Some(scene) = id.strip_prefix("scene:") {
                let scene = scene.to_string();
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = engine.apply_scene(&scene).await {
                        log::warn!("scène {scene} : {e}");
                    }
                    engine.notify_changed();
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn default_rooms_follow_the_language_until_renamed() {
        assert_eq!(super::room_name("salon", "Salon", "en"), "Living room");
        assert_eq!(super::room_name("salon", "Salon", "fr"), "Salon");
        assert_eq!(super::room_name("salon", "Séjour", "en"), "Séjour");
        assert_eq!(super::room_name("garage", "Garage", "en"), "Garage");
    }
}
