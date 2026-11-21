mod audio;
mod commands;
mod engine;
mod network;
mod state;
mod ui;

use std::sync::Arc;
use std::time::Duration;

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, RunEvent};

use engine::Engine;

pub fn tray_menu(app: &AppHandle, lang: &str) -> tauri::Result<Menu<tauri::Wry>> {
    let (open, mixer, quit) = if lang == "en" {
        ("Open Rezo", "Audio mixer…", "Quit")
    } else {
        ("Ouvrir Rezo", "Mixeur audio…", "Quitter")
    };
    let show = MenuItem::with_id(app, "show", open, true, None::<&str>)?;
    let mixer = MenuItem::with_id(app, "mixer", mixer, true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", quit, true, None::<&str>)?;
    Menu::with_items(app, &[&show, &mixer, &quit])
}

fn build_tray(app: &AppHandle, lang: &str) -> tauri::Result<()> {
    let menu = tray_menu(app, lang)?;
    TrayIconBuilder::with_id("main")
        .icon(Image::from_bytes(include_bytes!("../icons/tray.png"))?)
        .tooltip("Rezo")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, ev| match ev.id.as_ref() {
            "show" => ui::show_main(app, None),
            "mixer" => ui::open_mixer(app),
            "quit" => app.exit(0),
            _ => {}
        })
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

pub fn run() {
    let mut logger = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"));
    #[cfg(not(feature = "console"))]
    let log_path = std::env::var_os("APPDATA").map(|a| std::path::PathBuf::from(a).join("Rezo").join("rezo.log"));
    #[cfg(not(feature = "console"))]
    if let Some(path) = &log_path {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        // Append only: a second instance exits in the single-instance plugin and must not wipe the running log.
        if let Ok(f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            logger.target(env_logger::Target::Pipe(Box::new(f)));
        }
    }
    logger.init();
    std::panic::set_hook(Box::new(|info| log::error!("panic : {info}")));

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .max_blocking_threads(2)
        .thread_name("rezo-rt")
        .enable_all()
        .build()
        .expect("runtime tokio");
    tauri::async_runtime::set(rt.handle().clone());
    Box::leak(Box::new(rt));

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| ui::show_main(app, None)))
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec!["--hidden"])))
        .setup(move |app| {
            #[cfg(not(feature = "console"))]
            if let Some(path) = &log_path {
                let _ = std::fs::OpenOptions::new().write(true).open(path).and_then(|f| f.set_len(0));
            }
            let dir = match std::env::var_os("REZO_CONFIG_DIR") {
                Some(d) => std::path::PathBuf::from(d),
                None => app.path().config_dir()?.join("Rezo"),
            };
            let engine = Engine::new(dir);
            app.manage(engine.clone());
            engine.start(app.handle().clone());
            let lang = engine.store.read(|c| c.settings.language.clone());
            build_tray(app.handle(), &lang)?;
            if !std::env::args().any(|a| a == "--hidden") {
                ui::show_main(app.handle(), None);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::set_param,
            commands::set_room_param,
            commands::set_master,
            commands::set_local_pc,
            commands::set_source,
            commands::list_apps,
            commands::set_send,
            commands::set_room_send,
            commands::set_device_volume,
            commands::set_use_line_in,
            commands::sync_speakers,
            commands::scan_devices,
            commands::add_manual_device,
            commands::rename_speaker,
            commands::set_speaker_room,
            commands::remove_speaker,
            commands::get_receiver_info,
            commands::create_room,
            commands::update_room,
            commands::delete_room,
            commands::save_scene,
            commands::apply_scene,
            commands::delete_scene,
            commands::rename_scene,
            commands::set_midi_map,
            commands::set_settings,
            commands::get_autostart,
            commands::set_autostart,
            commands::set_window_mode,
            commands::set_pinned,
            commands::hide_window,
            commands::quit_app,
            commands::mixer_state,
            commands::mixer_set_device_volume,
            commands::mixer_set_device_mute,
            commands::mixer_set_app_volume,
            commands::mixer_set_app_mute,
            commands::mixer_set_default,
            commands::open_mixer,
            commands::set_language,
        ])
        .build(tauri::generate_context!())
        .expect("erreur au démarrage de Tauri");

    app.run(|app, event| match event {
        RunEvent::ExitRequested { api, code, .. } => {
            if code.is_none() {
                api.prevent_exit();
            }
        }
        RunEvent::Exit => {
            if let Some(e) = app.try_state::<Arc<Engine>>() {
                let e = e.inner().clone();
                tauri::async_runtime::block_on(async move {
                    let _ = tokio::time::timeout(Duration::from_secs(4), e.shutdown()).await;
                });
            }
        }
        _ => {}
    });
}
