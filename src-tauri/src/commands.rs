use std::sync::Arc;

use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_autostart::ManagerExt;

use crate::audio::sessions::AudioApp;
use crate::engine::{Engine, FullState, ReceiverInfo, SyncReport};
use crate::state::rooms::{MidiBinding, Settings, Source};
use crate::ui;

type Eng<'a> = State<'a, Arc<Engine>>;
type Res<T> = Result<T, String>;

fn e2s<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

#[tauri::command]
pub fn get_state(engine: Eng) -> FullState {
    engine.snapshot()
}

#[tauri::command]
pub fn set_param(engine: Eng, id: String, key: String, value: Value) -> Res<()> {
    engine.set_param(&id, &key, &value).map_err(e2s)
}

#[tauri::command]
pub fn set_room_param(engine: Eng, id: String, key: String, value: Value) -> Res<()> {
    engine.set_room_param(&id, &key, &value).map_err(e2s)
}

#[tauri::command]
pub fn set_master(engine: Eng, db: f32) {
    engine.set_master(db);
}

#[tauri::command]
pub fn set_master_mute(engine: Eng, on: bool) {
    engine.set_master_mute(on);
}

#[tauri::command]
pub async fn identify_speaker(engine: Eng<'_>, id: String) -> Res<()> {
    let e = engine.inner().clone();
    e.identify(&id).await.map_err(e2s)
}

#[tauri::command]
pub async fn reconnect_speaker(engine: Eng<'_>, id: String) -> Res<FullState> {
    let e = engine.inner().clone();
    e.reconnect(&id).await;
    Ok(e.snapshot())
}

#[tauri::command]
pub fn reorder(engine: Eng, speakers: Option<Vec<String>>, rooms: Option<Vec<String>>) {
    engine.reorder(speakers, rooms);
}

#[tauri::command]
pub fn set_speaker_style(engine: Eng, id: String, color: Option<String>, icon: Option<String>) {
    engine.set_speaker_style(&id, color, icon);
}

#[tauri::command]
pub fn set_plan_position(engine: Eng, id: String, x: Option<f32>, y: Option<f32>) {
    engine.set_plan_position(&id, x.zip(y));
}

#[tauri::command]
pub fn get_plan_image(engine: Eng) -> Option<String> {
    engine.plan_image()
}

#[tauri::command]
pub fn set_plan_image(engine: Eng, data: Option<String>) -> Res<()> {
    engine.set_plan_image(data).map_err(e2s)
}

#[tauri::command]
pub fn set_ui_zoom(app: AppHandle, engine: Eng, zoom: f32) -> f32 {
    let z = engine.set_ui_zoom(zoom);
    ui::apply_zoom(&app, z);
    z
}

// Window-creating commands are async: a synchronous command that builds a webview deadlocks on Windows.
#[tauri::command]
pub async fn widget_add(app: AppHandle, engine: Eng<'_>, kind: String) -> Res<FullState> {
    engine.widget_add(&kind);
    ui::sync_widgets(&app);
    Ok(engine.snapshot())
}

#[tauri::command]
pub async fn widget_update(app: AppHandle, engine: Eng<'_>, id: String, patch: Value) -> Res<FullState> {
    engine.widget_update(&id, &patch).map_err(e2s)?;
    ui::sync_widgets(&app);
    Ok(engine.snapshot())
}

#[tauri::command]
pub async fn widget_remove(app: AppHandle, engine: Eng<'_>, id: String) -> Res<FullState> {
    engine.widget_remove(&id);
    ui::sync_widgets(&app);
    Ok(engine.snapshot())
}

// Sent by a widget page once it has laid out its content.
#[tauri::command]
pub fn widget_fit(app: AppHandle, id: String, width: f64, height: f64) {
    ui::fit_widget(&app, &id, width, height);
}

// A widget's settings button: open the main window on the Settings dialog.
#[tauri::command]
pub async fn show_settings(app: AppHandle) {
    let existed = app.get_webview_window("main").is_some();
    ui::show_main(&app, None);
    if existed {
        let _ = app.emit_to("main", "open-settings", ());
    } else {
        // The window was just created: give its page time to load and listen.
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
            let _ = app.emit_to("main", "open-settings", ());
        });
    }
}

#[tauri::command]
pub fn set_local_pc(engine: Eng, on: bool) {
    engine.set_local_pc(on);
}

#[tauri::command]
pub fn set_source(engine: Eng, source: Source) {
    engine.set_source(source);
}

#[tauri::command]
pub async fn list_apps(engine: Eng<'_>) -> Res<Vec<AudioApp>> {
    let e = engine.inner().clone();
    tauri::async_runtime::spawn_blocking(move || e.list_apps()).await.map_err(e2s)
}

#[tauri::command]
pub async fn set_send(engine: Eng<'_>, id: String, on: bool) -> Res<FullState> {
    let e = engine.inner().clone();
    e.set_send(&id, on).await;
    Ok(e.snapshot())
}

#[tauri::command]
pub async fn set_room_send(engine: Eng<'_>, id: String, on: bool) -> Res<FullState> {
    let e = engine.inner().clone();
    e.set_room_send(&id, on).await;
    Ok(e.snapshot())
}

#[tauri::command]
pub async fn set_device_volume(engine: Eng<'_>, id: String, volume: u32) -> Res<()> {
    let e = engine.inner().clone();
    e.set_device_volume(&id, volume).await.map_err(e2s)
}

#[tauri::command]
pub async fn set_use_line_in(engine: Eng<'_>, id: String, on: bool) -> Res<FullState> {
    let e = engine.inner().clone();
    e.set_use_line_in(&id, on).await.map_err(e2s)?;
    Ok(e.snapshot())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncReply {
    report: SyncReport,
    state: FullState,
}

#[tauri::command]
pub async fn sync_speakers(engine: Eng<'_>) -> Res<SyncReply> {
    let e = engine.inner().clone();
    let report = e.sync_speakers().await;
    Ok(SyncReply { report, state: e.snapshot() })
}

#[tauri::command]
pub async fn media_state() -> Res<crate::media::MediaState> {
    tauri::async_runtime::spawn_blocking(crate::media::state).await.map_err(e2s)?.map_err(e2s)
}

#[tauri::command]
pub async fn media_command(cmd: String, position_ms: Option<u64>) -> Res<()> {
    tauri::async_runtime::spawn_blocking(move || crate::media::command(&cmd, position_ms)).await.map_err(e2s)?.map_err(e2s)
}

#[tauri::command]
pub async fn check_update() -> Res<crate::updater::UpdateInfo> {
    crate::updater::check().await.map_err(e2s)
}

#[tauri::command]
pub async fn install_update(app: AppHandle) -> Res<()> {
    crate::updater::install(app).await.map_err(e2s)
}

// Opens the releases page only: the interface cannot ask for any other URL.
#[tauri::command]
pub fn open_release_page() -> Res<()> {
    const PAGE: &str = "https://github.com/theappstore-cell/Rezo/releases/latest";
    #[cfg(windows)]
    let r = {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("explorer.exe").arg(PAGE).creation_flags(0x0800_0000).spawn()
    };
    #[cfg(target_os = "macos")]
    let r = std::process::Command::new("open").arg(PAGE).spawn();
    #[cfg(all(not(windows), not(target_os = "macos")))]
    let r = std::process::Command::new("xdg-open").arg(PAGE).spawn();
    r.map(|_| ()).map_err(e2s)
}

#[tauri::command]
pub async fn scan_devices(engine: Eng<'_>) -> Res<FullState> {
    let e = engine.inner().clone();
    e.scan().await;
    e.reactivate_wanted().await;
    Ok(e.snapshot())
}

#[tauri::command]
pub async fn add_manual_device(engine: Eng<'_>, ip: String, kind: String, port: Option<u16>) -> Res<FullState> {
    let e = engine.inner().clone();
    e.add_manual(&ip, &kind, port).await.map_err(e2s)?;
    Ok(e.snapshot())
}

#[tauri::command]
pub fn rename_speaker(engine: Eng, id: String, name: String) {
    engine.rename_speaker(&id, &name);
}

#[tauri::command]
pub fn set_speaker_room(engine: Eng, id: String, room_id: String) {
    engine.set_speaker_room(&id, &room_id);
}

#[tauri::command]
pub async fn remove_speaker(engine: Eng<'_>, id: String) -> Res<FullState> {
    let e = engine.inner().clone();
    e.remove_speaker(&id).await;
    Ok(e.snapshot())
}

#[tauri::command]
pub fn get_receiver_info(engine: Eng, ip: Option<String>) -> Res<ReceiverInfo> {
    engine.receiver_info(ip).map_err(e2s)
}

#[tauri::command]
pub fn create_room(engine: Eng, name: String, icon: String, color: String) -> String {
    engine.create_room(&name, &icon, &color)
}

#[tauri::command]
pub fn update_room(engine: Eng, id: String, name: Option<String>, icon: Option<String>, color: Option<String>) {
    engine.update_room(&id, name, icon, color);
}

#[tauri::command]
pub fn delete_room(engine: Eng, id: String) {
    engine.delete_room(&id);
}

#[tauri::command]
pub fn save_scene(engine: Eng, name: String) -> String {
    engine.save_scene(&name)
}

#[tauri::command]
pub async fn apply_scene(engine: Eng<'_>, id: String) -> Res<FullState> {
    let e = engine.inner().clone();
    e.apply_scene(&id).await.map_err(e2s)?;
    Ok(e.snapshot())
}

#[tauri::command]
pub fn delete_scene(engine: Eng, id: String) {
    engine.delete_scene(&id);
}

#[tauri::command]
pub fn rename_scene(engine: Eng, id: String, name: String) {
    engine.rename_scene(&id, &name);
}

#[tauri::command]
pub fn set_midi_map(engine: Eng, map: Vec<MidiBinding>) {
    engine.set_midi_map(map);
}

#[tauri::command]
pub async fn set_settings(engine: Eng<'_>, settings: Settings) -> Res<FullState> {
    let e = engine.inner().clone();
    e.set_settings(settings).await;
    Ok(e.snapshot())
}

#[tauri::command]
pub fn get_autostart(app: AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[tauri::command]
pub fn set_autostart(app: AppHandle, on: bool) -> Res<()> {
    let m = app.autolaunch();
    (if on { m.enable() } else { m.disable() }).map_err(e2s)
}

#[tauri::command]
pub fn set_window_mode(app: AppHandle, wide: bool) {
    ui::set_mode(&app, wide);
}

#[tauri::command]
pub fn set_pinned(app: AppHandle, engine: Eng, pinned: bool) {
    engine.set_pinned(pinned);
    ui::set_pinned(&app, pinned);
}

#[tauri::command]
pub fn hide_window(app: AppHandle) {
    ui::hide_main(&app);
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    app.exit(0);
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MixerReply {
    #[serde(flatten)]
    state: crate::audio::mixer::MixerState,
    default_locked: bool,
}

#[tauri::command]
pub async fn mixer_state(engine: Eng<'_>) -> Res<MixerReply> {
    let locked = engine.output_switch_engaged();
    let state = tauri::async_runtime::spawn_blocking(crate::audio::mixer::snapshot)
        .await
        .map_err(e2s)?
        .map_err(e2s)?;
    Ok(MixerReply { state, default_locked: locked })
}

#[tauri::command]
pub async fn mixer_set_device_volume(id: String, volume: f32) -> Res<()> {
    tauri::async_runtime::spawn_blocking(move || crate::audio::mixer::set_device_volume(&id, volume))
        .await
        .map_err(e2s)?
        .map_err(e2s)
}

#[tauri::command]
pub async fn mixer_set_device_mute(id: String, mute: bool) -> Res<()> {
    tauri::async_runtime::spawn_blocking(move || crate::audio::mixer::set_device_mute(&id, mute))
        .await
        .map_err(e2s)?
        .map_err(e2s)
}

#[tauri::command]
pub async fn mixer_set_app_volume(device_id: String, key: String, volume: f32) -> Res<()> {
    tauri::async_runtime::spawn_blocking(move || crate::audio::mixer::set_app_volume(&device_id, &key, volume))
        .await
        .map_err(e2s)?
        .map_err(e2s)
}

#[tauri::command]
pub async fn mixer_set_app_mute(device_id: String, key: String, mute: bool) -> Res<()> {
    tauri::async_runtime::spawn_blocking(move || crate::audio::mixer::set_app_mute(&device_id, &key, mute))
        .await
        .map_err(e2s)?
        .map_err(e2s)
}

#[tauri::command]
pub async fn mixer_set_default(engine: Eng<'_>, id: String) -> Res<()> {
    if engine.output_switch_engaged() {
        return Err("Smart Output est actif (haut-parleurs du PC coupés) : désactivez-le ou arrêtez les envois avant de changer de sortie.".into());
    }
    tauri::async_runtime::spawn_blocking(move || crate::audio::mixer::set_default_device(&id))
        .await
        .map_err(e2s)?
        .map_err(e2s)
}

#[tauri::command]
pub fn set_language(app: AppHandle, engine: Eng<'_>, lang: String) {
    let lang = if lang == "en" { "en" } else { "fr" }.to_string();
    engine.store.write(|c| c.settings.language = lang.clone());
    crate::tray::refresh(&app, &engine, true);
}

#[tauri::command]
pub async fn open_mixer(app: AppHandle) {
    ui::open_mixer(&app);
}
