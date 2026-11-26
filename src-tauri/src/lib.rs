mod audio;
mod commands;
mod engine;
mod media;
mod network;
mod state;
mod tray;
mod ui;
mod updater;

use std::sync::Arc;
use std::time::Duration;

use tauri::{Manager, RunEvent};

use engine::Engine;

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
    // After a self-update, wait for the previous process so the single-instance check lets this one start.
    updater::after_update();

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
            ui::init_pinned(engine.store.read(|c| c.window_pinned));
            tray::build(app.handle(), &engine)?;
            ui::sync_widgets(app.handle());
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
            commands::set_master_mute,
            commands::identify_speaker,
            commands::reconnect_speaker,
            commands::reorder,
            commands::set_speaker_style,
            commands::set_plan_position,
            commands::get_plan_image,
            commands::set_plan_image,
            commands::set_ui_zoom,
            commands::widget_add,
            commands::widget_update,
            commands::widget_remove,
            commands::widget_fit,
            commands::show_settings,
            commands::set_local_pc,
            commands::set_source,
            commands::list_apps,
            commands::set_send,
            commands::set_room_send,
            commands::set_device_volume,
            commands::set_use_line_in,
            commands::sync_speakers,
            commands::check_update,
            commands::install_update,
            commands::open_release_page,
            commands::media_state,
            commands::media_command,
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
