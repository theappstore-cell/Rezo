use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};
use futures_util::future::join_all;
use parking_lot::{Mutex, RwLock};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::audio::capture::{self, CaptureHandle, CaptureTarget};
use crate::audio::router::{ChannelRuntime, ChannelShared, Router};
use crate::audio::dsp::DelayLine;
use crate::audio::router::DEFAULT_DELAY_FRAMES;
use crate::audio::{output_switch, sessions, BLOCK_SAMPLES, SAMPLE_RATE};
use crate::network::discovery::{self, NativeAnnounce};
use crate::network::sonos_upnp::{self as upnp, DeviceInfo};
use crate::network::udp_streamer::{self, UdpConfig, UdpHandle};
use crate::network::{self, http_server, PcmTap};
use crate::state::rooms::*;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SpeakerView {
    #[serde(flatten)]
    pub speaker: Speaker,
    pub running: bool,
    pub online: bool,
    pub streaming: bool,
    pub error: Option<String>,
    pub group: Option<String>,
    pub latency_ms: Option<u32>,
    pub device_volume: Option<u32>,
    pub synced_to: Option<String>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SyncReport {
    pub grouped: Vec<String>,
    pub delayed: Vec<(String, u32)>,
    pub ungrouped: Vec<String>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SceneView {
    pub id: String,
    pub name: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CaptureView {
    pub running: bool,
    pub error: Option<String>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FullState {
    pub master_db: f32,
    pub source: Source,
    pub local_pc: bool,
    pub speakers: Vec<SpeakerView>,
    pub rooms: Vec<Room>,
    pub scenes: Vec<SceneView>,
    pub midi_map: Vec<MidiBinding>,
    pub settings: Settings,
    pub http_port: u16,
    pub capture: CaptureView,
    pub pc_muted: bool,
}

#[derive(Serialize, Clone)]
pub struct Meters {
    pub m: [f32; 2],
    pub c: HashMap<String, [f32; 2]>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReceiverInfo {
    pub url: String,
    pub qr_svg: String,
    pub ips: Vec<String>,
}

enum SinkKind {
    Udp(UdpHandle, Arc<AtomicU32>),
    Tap,
    // Sonos playing its own analog input: no stream from Rezo, and the PC output that feeds the jack must stay on.
    LineIn,
    // Sonos grouped by Sync with the speaker of that id: it plays the coordinator's audio, sample-aligned.
    Follower(String),
}

enum Prepared {
    Ready,
    Http(upnp::Target),
    LineIn(upnp::Target),
}

impl SinkKind {
    fn streams(&self) -> bool {
        matches!(self, SinkKind::Udp(..) | SinkKind::Tap)
    }
}

struct Sink {
    kind: SinkKind,
    shared: Arc<ChannelShared>,
}

#[derive(Default, Clone)]
struct Rt {
    online: bool,
    last_seen: Option<Instant>,
    error: Option<String>,
    group: Option<String>,
    client_buf_ms: u32,
    device_volume: Option<u32>,
    volume_set_at: Option<Instant>,
}

// Measured with a microphone on a Sonos Play:1 (capture to sound), see PREROLL_LOW_LATENCY.
const SONOS_LATENCY_LOW_MS: u32 = 300;
const SONOS_LATENCY_SAFE_MS: u32 = 420;
const RECEIVER_BUFFER_MS: f32 = 20.0;

pub struct Engine {
    pub store: Arc<Store>,
    router: Router,
    http: reqwest::Client,
    app: OnceLock<AppHandle>,
    capture: Mutex<Option<CaptureHandle>>,
    sinks: Mutex<HashMap<String, Sink>>,
    taps: RwLock<HashMap<String, Arc<PcmTap>>>,
    rt: Mutex<HashMap<String, Rt>>,
    web_tokens: Mutex<HashMap<String, u64>>,
    token_seq: AtomicU64,
    http_port: AtomicU16,
    pc_muted: AtomicBool,
    output_notify: tokio::sync::Notify,
    switch_lock: Mutex<()>,
    changed: AtomicBool,
    change_notify: tokio::sync::Notify,
    mdns: Mutex<Option<mdns_sd::ServiceDaemon>>,
    pump: std::thread::Thread,
    delay_caps: Mutex<HashMap<String, usize>>,
    ui_visible: AtomicBool,
    ui_notify: tokio::sync::Notify,
    scan_wake: tokio::sync::Notify,
    last_scan: Mutex<Instant>,
    scan_interval_secs: AtomicU64,
    productive_ifaces: Mutex<Vec<std::net::Ipv4Addr>>,
    desc_cache: Mutex<HashMap<String, (DeviceInfo, Instant)>>,
    subs: Mutex<HashMap<String, Sub>>,
    stopping: Mutex<HashSet<String>>,
    last_restart: Mutex<HashMap<String, Instant>>,
    volume_pending: Mutex<HashMap<String, u32>>,
    stats_on: bool,
}

struct Sub {
    sid: String,
    renew_at: Instant,
}

#[derive(Default, Clone, Copy)]
struct StatsSample {
    blocks: u64,
    busy_ns: u64,
    quiet: u64,
    total: u64,
}

// Delay for each path so that all of them play with the slowest one (bounded by the delay knob's range).
fn alignment_delays(latencies: &[u32]) -> Vec<u32> {
    let slowest = latencies.iter().copied().max().unwrap_or(0);
    latencies.iter().map(|l| (slowest - l).min(500)).collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn faster_paths_wait_for_the_slowest() {
        assert_eq!(super::alignment_delays(&[300, 42, 46]), vec![0, 258, 254]);
        assert_eq!(super::alignment_delays(&[2000, 40]), vec![0, 500]);
        assert!(super::alignment_delays(&[]).is_empty());
    }
}

fn fnv(s: &str) -> u32 {
    let mut h: u32 = 0x811c9dc5;
    for b in s.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(16777619);
    }
    h
}

impl Engine {
    pub fn new(config_dir: PathBuf) -> Arc<Engine> {
        let store = Arc::new(Store::load(config_dir));
        let router = Router::start();
        let pump = network::spawn_tap_pump();
        let (master, ) = store.read(|c| (c.master_db, ));
        router.shared.master_db.store(master);
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(4))
            .pool_max_idle_per_host(1)
            .build()
            .expect("client HTTP");
        Arc::new(Engine {
            store,
            router,
            http,
            app: OnceLock::new(),
            capture: Mutex::new(None),
            sinks: Mutex::new(HashMap::new()),
            taps: RwLock::new(HashMap::new()),
            rt: Mutex::new(HashMap::new()),
            web_tokens: Mutex::new(HashMap::new()),
            token_seq: AtomicU64::new(1),
            http_port: AtomicU16::new(0),
            pc_muted: AtomicBool::new(false),
            output_notify: tokio::sync::Notify::new(),
            switch_lock: Mutex::new(()),
            changed: AtomicBool::new(false),
            change_notify: tokio::sync::Notify::new(),
            mdns: Mutex::new(None),
            pump,
            delay_caps: Mutex::new(HashMap::new()),
            ui_visible: AtomicBool::new(false),
            ui_notify: tokio::sync::Notify::new(),
            scan_wake: tokio::sync::Notify::new(),
            last_scan: Mutex::new(Instant::now()),
            scan_interval_secs: AtomicU64::new(30),
            productive_ifaces: Mutex::new(Vec::new()),
            desc_cache: Mutex::new(HashMap::new()),
            subs: Mutex::new(HashMap::new()),
            stopping: Mutex::new(HashSet::new()),
            last_restart: Mutex::new(HashMap::new()),
            volume_pending: Mutex::new(HashMap::new()),
            stats_on: std::env::var_os("REZO_STATS").is_some(),
        })
    }

    pub fn start(self: &Arc<Self>, app: AppHandle) {
        let _ = self.app.set(app);
        self.router.set_input(None);

        let me = self.clone();
        tauri::async_runtime::spawn(async move {
            me.recover_output_switch();
            let preferred = me.store.read(|c| c.settings.http_port);
            match http_server::serve(me.clone(), preferred).await {
                Ok(p) => me.http_port.store(p, Ordering::Relaxed),
                Err(e) => log::error!("serveur HTTP : {e}"),
            }
            me.notify_changed();
            tauri::async_runtime::spawn(me.clone().meter_loop());
            tauri::async_runtime::spawn(me.clone().housekeeping_loop());
            tauri::async_runtime::spawn(me.clone().discovery_loop());
            tauri::async_runtime::spawn(me.clone().watchdog_loop());
            tauri::async_runtime::spawn(me.clone().output_guard_loop());
            tauri::async_runtime::spawn(me.clone().volume_loop());
        });
    }

    pub fn preroll_chunks(&self) -> u32 {
        if self.store.read(|c| c.settings.sonos_low_latency) {
            upnp::PREROLL_LOW_LATENCY
        } else {
            upnp::PREROLL_SAFE
        }
    }

    fn streaming_sinks(&self) -> bool {
        self.sinks.lock().values().any(|s| s.kind.streams())
    }

    fn line_in_active(&self) -> bool {
        self.sinks.lock().values().any(|s| matches!(s.kind, SinkKind::LineIn))
    }

    fn tap_sink_ids(&self) -> HashSet<String> {
        self.sinks.lock().iter().filter(|(_, s)| matches!(s.kind, SinkKind::Tap)).map(|(id, _)| id.clone()).collect()
    }

    fn emit<S: Serialize + Clone>(&self, event: &str, payload: S) {
        if let Some(app) = self.app.get() {
            let _ = app.emit(event, payload);
        }
    }

    pub fn notify_changed(&self) {
        self.changed.store(true, Ordering::Release);
        self.change_notify.notify_one();
    }

    pub fn set_ui_visible(&self, visible: bool) {
        self.ui_visible.store(visible, Ordering::Release);
        crate::audio::set_timer_resolution(visible);
        self.ui_notify.notify_waiters();
        if visible && self.last_scan.lock().elapsed() > Duration::from_secs(20) {
            self.scan_wake.notify_one();
        }
    }

    async fn housekeeping_loop(self: Arc<Self>) {
        let mut last_stats = Instant::now();
        let mut prev = StatsSample::default();
        loop {
            let _ = tokio::time::timeout(Duration::from_secs(if self.stats_on { 1 } else { 5 }), self.change_notify.notified()).await;
            if self.changed.load(Ordering::Acquire) {
                tokio::time::sleep(Duration::from_millis(100)).await;
                self.changed.store(false, Ordering::Release);
                self.emit("state", self.snapshot());
            }
            self.store.flush();
            self.router.reap();
            if self.stats_on && last_stats.elapsed() >= Duration::from_secs(10) {
                self.log_stats(&mut prev, last_stats.elapsed());
                last_stats = Instant::now();
            }
        }
    }

    async fn meter_loop(self: Arc<Self>) {
        loop {
            if !self.ui_visible.load(Ordering::Acquire) {
                let woken = self.ui_notify.notified();
                tokio::pin!(woken);
                woken.as_mut().enable();
                if !self.ui_visible.load(Ordering::Acquire) {
                    woken.await;
                }
                continue;
            }
            tokio::time::sleep(Duration::from_millis(16)).await;
            let sh = &self.router.shared;
            let m = [sh.peak_in_l.take(), sh.peak_in_r.take()];
            let c: HashMap<String, [f32; 2]> = self
                .sinks
                .lock()
                .iter()
                .map(|(id, s)| {
                    let (l, r) = s.shared.take_peaks();
                    (id.clone(), [l, r])
                })
                .collect();
            self.emit("meters", Meters { m, c });
        }
    }

    fn log_stats(&self, prev: &mut StatsSample, dt: Duration) {
        let sh = &self.router.shared;
        let cur = StatsSample {
            blocks: sh.blocks.load(Ordering::Relaxed),
            busy_ns: sh.busy_ns.load(Ordering::Relaxed),
            quiet: sh.quiet_channel_blocks.load(Ordering::Relaxed),
            total: sh.total_channel_blocks.load(Ordering::Relaxed),
        };
        let secs = dt.as_secs_f64().max(0.001);
        let blocks = cur.blocks - prev.blocks;
        let busy_pct = (cur.busy_ns - prev.busy_ns) as f64 / (secs * 1e9) * 100.0;
        let tot = (cur.total - prev.total).max(1);
        let quiet_pct = (cur.quiet - prev.quiet) as f64 * 100.0 / tot as f64;
        let max_us = sh.max_block_ns.swap(0, Ordering::Relaxed) as f64 / 1000.0;
        let taps: Vec<String> = self.taps.read().iter().map(|(id, t)| format!("{id}={}", t.available())).collect();
        let delay_kb: usize = self.delay_caps.lock().values().map(|f| DelayLine::bytes_for(*f) / 1024).sum();
        log::info!(
            "stats: {:.0} blocs/s, routeur {:.3} % d'un cœur, pire bloc {:.0} µs, chemin silence {:.0} %, blocs perdus {}, lignes à retard {} Ko, files [{}]",
            blocks as f64 / secs,
            busy_pct,
            max_us,
            quiet_pct,
            sh.dropped_blocks.load(Ordering::Relaxed),
            delay_kb,
            taps.join(" ")
        );
        *prev = cur;
    }

    // `via_stream`: false for a Sonos on its line-in or following a group, whose delay Rezo does not know.
    fn latency_estimate(sp: &Speaker, via_stream: bool, settings: &Settings, client_buf_ms: u32) -> Option<u32> {
        match sp.protocol {
            Protocol::Native => {
                let capture_period = if settings.pin_engine_period { 3.0 } else { 10.0 };
                Some((capture_period + settings.udp_frame_ms + 2.5 + 4.0 + RECEIVER_BUFFER_MS).round() as u32)
            }
            Protocol::Web if client_buf_ms > 0 => Some(client_buf_ms + 15),
            Protocol::Sonos if via_stream => Some(if settings.sonos_low_latency { SONOS_LATENCY_LOW_MS } else { SONOS_LATENCY_SAFE_MS }),
            _ => None,
        }
    }

    pub fn snapshot(&self) -> FullState {
        let (running, followers, line_ins): (HashSet<String>, HashMap<String, String>, HashSet<String>) = {
            let sinks = self.sinks.lock();
            (
                sinks.keys().cloned().collect(),
                sinks.iter().filter_map(|(id, s)| match &s.kind { SinkKind::Follower(c) => Some((id.clone(), c.clone())), _ => None }).collect(),
                sinks.iter().filter(|(_, s)| matches!(s.kind, SinkKind::LineIn)).map(|(id, _)| id.clone()).collect(),
            )
        };
        let rt = self.rt.lock().clone();
        let streaming: HashMap<String, bool> =
            self.taps.read().iter().map(|(k, t)| (k.clone(), t.connected.load(Ordering::Relaxed))).collect();
        let (running_cap, cap_err) = match &*self.capture.lock() {
            Some(h) => (h.status.running.load(Ordering::Relaxed), h.status.error.lock().clone()),
            None => (false, None),
        };

        self.store.read(|c| {
            FullState {
                master_db: c.master_db,
                source: c.source.clone(),
                local_pc: c.local_pc,
                speakers: c
                    .speakers
                    .iter()
                    .map(|s| {
                        let r = rt.get(&s.id).cloned().unwrap_or_default();
                        let is_running = running.contains(&s.id);
                        let follows = followers.get(&s.id);
                        let plays_line_in = line_ins.contains(&s.id);
                        let latency_ms = Self::latency_estimate(s, follows.is_none() && !plays_line_in && !s.plays_line_in(), &c.settings, r.client_buf_ms);
                        let device_volume = match s.protocol {
                            Protocol::Sonos | Protocol::Upnp => r.device_volume,
                            Protocol::Native | Protocol::Web => Some(s.receiver_volume.min(100)),
                        };
                        SpeakerView {
                            running: is_running,
                            online: r.online,
                            streaming: match s.protocol {
                                Protocol::Native => is_running,
                                _ if follows.is_some() || plays_line_in => is_running,
                                _ => streaming.get(&s.id).copied().unwrap_or(false),
                            },
                            error: r.error,
                            group: r.group,
                            latency_ms,
                            device_volume,
                            synced_to: follows.map(|cid| c.speaker(cid).map(|x| x.name.clone()).unwrap_or_else(|| cid.clone())),
                            speaker: s.clone(),
                        }
                    })
                    .collect(),
                rooms: c.rooms.clone(),
                scenes: c.scenes.iter().map(|s| SceneView { id: s.id.clone(), name: s.name.clone() }).collect(),
                midi_map: c.midi_map.clone(),
                settings: c.settings.clone(),
                http_port: self.http_port.load(Ordering::Relaxed),
                capture: CaptureView { running: running_cap, error: cap_err },
                pc_muted: self.pc_muted.load(Ordering::Relaxed),
            }
        })
    }

    pub fn restart_capture(&self) {
        let need = self.streaming_sinks();
        let mut cap = self.capture.lock();
        if let Some(h) = cap.take() {
            h.stop();
        }
        if need {
            *cap = Some(self.start_capture());
        } else {
            self.router.set_input(None);
        }
    }

    fn start_capture(&self) -> CaptureHandle {
        let (source, pin) = self.store.read(|c| (c.source.clone(), c.settings.pin_engine_period));
        let (prod, cons) = rtrb::RingBuffer::new(BLOCK_SAMPLES * 160);
        self.router.set_input(Some(cons));
        let target = match source {
            Source::System => CaptureTarget::System,
            Source::App { pid, .. } => CaptureTarget::Process { pid },
        };
        capture::start(target, prod, self.router.waker(), pin)
    }

    // No loopback stream while idle: an open stream keeps the output device (and Bluetooth links) awake.
    fn sync_capture(&self) {
        let need = self.streaming_sinks();
        let mut cap = self.capture.lock();
        if need && cap.is_none() {
            *cap = Some(self.start_capture());
        } else if !need {
            if let Some(h) = cap.take() {
                h.stop();
                self.router.set_input(None);
                log::info!("plus aucune enceinte active : capture arrêtée, périphérique audio libéré");
            }
        }
    }

    pub fn output_switch_engaged(&self) -> bool {
        self.pc_muted.load(Ordering::Relaxed)
    }

    pub fn list_apps(&self) -> Vec<sessions::AudioApp> {
        sessions::list_audio_apps()
    }

    pub fn set_source(&self, source: Source) {
        self.store.write(|c| c.source = source);
        self.restart_capture();
        self.update_output_switch();
        self.notify_changed();
    }

    pub fn set_master(&self, db: f32) {
        let db = if db.is_finite() { db.clamp(DB_MIN, 6.0) } else { 0.0 };
        self.router.shared.master_db.store(db);
        self.store.write(|c| c.master_db = db);
    }

    pub fn set_param(&self, id: &str, key: &str, value: &serde_json::Value) -> Result<()> {
        let params = self.store.write(|c| -> Result<ChannelParams> {
            let sp = c.speaker_mut(id).ok_or_else(|| anyhow!("enceinte inconnue"))?;
            sp.params.set(key, value)?;
            Ok(sp.params.clone())
        })?;
        if let Some(s) = self.sinks.lock().get(id) {
            s.shared.load(&params);
        }
        if key == "delayMs" {
            self.ensure_delay_capacity();
        }
        Ok(())
    }

    fn ensure_delay_capacity(&self) {
        let delays: Vec<(String, f32)> =
            self.sinks.lock().iter().filter(|(_, s)| s.kind.streams()).map(|(id, s)| (id.clone(), s.shared.delay_ms.load())).collect();
        let base = delays.iter().map(|d| d.1).fold(0.0f32, f32::min);
        let step = DEFAULT_DELAY_FRAMES;
        let mut caps = self.delay_caps.lock();
        for (id, d) in delays {
            let need = (((d - base) * (SAMPLE_RATE as f32 / 1000.0)).ceil().max(0.0) as usize) + 2 * crate::audio::BLOCK_FRAMES;
            let have = caps.get(&id).copied().unwrap_or(0);
            if need > have {
                let cap = need.div_ceil(step) * step;
                self.router.set_delay(&id, DelayLine::new(cap));
                caps.insert(id, cap);
            }
        }
    }

    pub fn set_room_param(&self, room_id: &str, key: &str, value: &serde_json::Value) -> Result<()> {
        self.store.write(|c| -> Result<()> {
            let r = c.rooms.iter_mut().find(|r| r.id == room_id).ok_or_else(|| anyhow!("pièce inconnue"))?;
            match key {
                "volumeDb" => r.volume_db = value.as_f64().ok_or_else(|| anyhow!("nombre attendu"))?.clamp(-90.0, 6.0) as f32,
                "mute" => r.mute = value.as_bool().ok_or_else(|| anyhow!("booléen attendu"))?,
                "solo" => r.solo = value.as_bool().ok_or_else(|| anyhow!("booléen attendu"))?,
                _ => bail!("paramètre de pièce inconnu : {key}"),
            }
            Ok(())
        })?;
        self.refresh_room(room_id);
        Ok(())
    }

    fn refresh_room(&self, room_id: &str) {
        let (room, members): (Option<Room>, Vec<String>) = self.store.read(|c| {
            (
                c.room(room_id).cloned(),
                c.speakers.iter().filter(|s| s.room_id == room_id).map(|s| s.id.clone()).collect(),
            )
        });
        if let Some(r) = room {
            let sinks = self.sinks.lock();
            for id in members {
                if let Some(s) = sinks.get(&id) {
                    s.shared.set_room(r.volume_db, r.mute, r.solo);
                }
            }
        }
    }

    pub fn set_local_pc(&self, on: bool) {
        self.store.write(|c| c.local_pc = on);
        self.update_output_switch();
        self.notify_changed();
    }

    pub fn access_key(&self) -> String {
        self.store.read(|c| c.access_key.clone())
    }

    pub fn key_ok(&self, key: &str) -> bool {
        let k = self.access_key();
        k.len() == key.len() && k.bytes().zip(key.bytes()).fold(0u8, |acc, (a, b)| acc | (a ^ b)) == 0
    }

    fn target_for(&self, sp: &Speaker) -> Result<upnp::Target> {
        let local = network::local_ip_for(&sp.address).ok_or_else(|| anyhow!("interface réseau introuvable pour {}", sp.address))?;
        Ok(upnp::Target {
            id: sp.id.clone(),
            ip: sp.address.clone(),
            port: sp.port,
            control: sp.control_url.clone().ok_or_else(|| anyhow!("URL de contrôle AVTransport inconnue"))?,
            is_sonos: sp.protocol == Protocol::Sonos,
            uuid: sp.uuid.clone(),
            url: upnp::stream_url(&local, self.http_port.load(Ordering::Relaxed), &self.access_key(), &sp.id),
            title: sp.name.clone(),
            event: sp.event_url.clone(),
        })
    }

    pub fn tap(&self, id: &str) -> Option<Arc<PcmTap>> {
        self.taps.read().get(id).cloned()
    }

    pub fn tap_is_current(&self, id: &str, tap: &Arc<PcmTap>) -> bool {
        self.taps.read().get(id).map_or(false, |t| Arc::ptr_eq(t, tap))
    }

    fn prepare(&self, id: &str) -> Result<Prepared> {
        if self.sinks.lock().contains_key(id) {
            return Ok(Prepared::Ready);
        }
        let (sp, settings, room) = self.store.read(|c| {
            let sp = c.speaker(id).cloned();
            let room = sp.as_ref().and_then(|s| c.room(&s.room_id).cloned());
            (sp, c.settings.clone(), room)
        });
        let sp = sp.ok_or_else(|| anyhow!("enceinte inconnue"))?;
        let shared = ChannelShared::new(&sp.params);
        if let Some(r) = &room {
            shared.set_room(r.volume_db, r.mute, r.solo);
        }

        if sp.plays_line_in() {
            let target = self.target_for(&sp)?;
            self.sinks.lock().insert(id.to_string(), Sink { kind: SinkKind::LineIn, shared });
            return Ok(Prepared::LineIn(target));
        }

        let mut target = None;
        let (kind, wake, prod) = match sp.protocol {
            Protocol::Native => {
                let addr: SocketAddr = format!("{}:{}", sp.address, sp.port)
                    .parse()
                    .map_err(|_| anyhow!("adresse invalide : {}:{}", sp.address, sp.port))?;
                let (prod, cons) = rtrb::RingBuffer::new(BLOCK_SAMPLES * 40);
                let volume = Arc::new(AtomicU32::new(sp.receiver_volume.min(100)));
                let h = udp_streamer::spawn(
                    cons,
                    UdpConfig {
                        addr,
                        frame_ms: settings.udp_frame_ms,
                        bitrate_kbps: settings.bitrate_kbps,
                        redundancy: settings.redundancy,
                        ssrc: fnv(&sp.id),
                        volume: volume.clone(),
                    },
                )?;
                let wake = Some(h.thread.clone());
                (SinkKind::Udp(h, volume), wake, prod)
            }
            Protocol::Sonos | Protocol::Upnp => {
                target = Some(self.target_for(&sp)?);
                let (prod, cons) = rtrb::RingBuffer::new(BLOCK_SAMPLES * 100);
                self.taps.write().insert(id.to_string(), Arc::new(PcmTap::new(cons, 100)));
                (SinkKind::Tap, Some(self.pump.clone()), prod)
            }
            Protocol::Web => {
                let (prod, cons) = rtrb::RingBuffer::new(BLOCK_SAMPLES * 40);
                self.taps.write().insert(id.to_string(), Arc::new(PcmTap::new(cons, sp.receiver_volume)));
                (SinkKind::Tap, Some(self.pump.clone()), prod)
            }
        };

        self.router.add_channel(ChannelRuntime::new(id.to_string(), shared.clone(), prod, wake, DEFAULT_DELAY_FRAMES));
        self.delay_caps.lock().insert(id.to_string(), DEFAULT_DELAY_FRAMES);
        self.sinks.lock().insert(id.to_string(), Sink { kind, shared });
        self.ensure_delay_capacity();
        self.sync_capture();
        Ok(target.map_or(Prepared::Ready, Prepared::Http))
    }

    fn teardown(&self, id: &str) {
        self.router.remove_channel(id);
        let sink = self.sinks.lock().remove(id);
        self.taps.write().remove(id);
        self.delay_caps.lock().remove(id);
        self.subs.lock().remove(id);
        self.ensure_delay_capacity();
        self.router.reap();
        if let Some(Sink { kind: SinkKind::Udp(h, _), .. }) = sink {
            h.stop();
        }
        self.sync_capture();
    }

    // Lock order: never take `sinks` or `rt` inside a `store.read/write` closure, or the reverse.
    fn running_ids(&self) -> HashSet<String> {
        self.sinks.lock().keys().cloned().collect()
    }

    fn online_ids(&self) -> HashSet<String> {
        self.rt.lock().iter().filter(|(_, r)| r.online).map(|(id, _)| id.clone()).collect()
    }

    fn set_error(&self, id: &str, msg: Option<String>) {
        self.rt.lock().entry(id.to_string()).or_default().error = msg;
    }

    fn fail(&self, id: &str, msg: &str) {
        log::warn!("activation de {id} impossible : {msg}");
        self.set_error(id, Some(msg.to_string()));
        self.store.write(|c| {
            if let Some(s) = c.speaker_mut(id) {
                s.active = false;
            }
        });
    }

    pub async fn activate_many(self: &Arc<Self>, ids: Vec<String>) {
        let mut targets = vec![];
        let mut line_ins = vec![];
        let mut started = vec![];
        for id in &ids {
            if self.sinks.lock().contains_key(id) {
                continue;
            }
            match self.prepare(id) {
                Ok(p) => {
                    started.push(id.clone());
                    match p {
                        Prepared::Http(t) => targets.push(t),
                        Prepared::LineIn(t) => line_ins.push(t),
                        Prepared::Ready => {}
                    }
                }
                Err(e) => self.fail(id, &e.to_string()),
            }
        }
        let http_ids: Vec<String> = targets.iter().map(|t| t.id.clone()).collect();
        if !line_ins.is_empty() {
            let me = self;
            let results = join_all(line_ins.iter().map(|t| async move { (t.id.clone(), upnp::play_line_in(&me.http, t).await) })).await;
            for (id, r) in results {
                match r {
                    Ok(()) => log::info!("{id} : entrée ligne en lecture"),
                    Err(e) => {
                        self.teardown(&id);
                        started.retain(|x| x != &id);
                        self.fail(&id, &e.to_string());
                    }
                }
            }
        }
        if !targets.is_empty() {
            for (id, r) in upnp::start_synchronized(&self.http, targets).await {
                if let Err(e) = r {
                    self.teardown(&id);
                    started.retain(|x| x != &id);
                    self.fail(&id, &e.to_string());
                }
            }
        }
        self.store.write(|c| {
            for id in &started {
                if let Some(s) = c.speaker_mut(id) {
                    s.active = true;
                }
            }
        });
        for id in &started {
            self.set_error(id, None);
        }
        let upnp_ids: Vec<String> = http_ids.into_iter().filter(|id| started.contains(id)).collect();
        let upnp_started = !upnp_ids.is_empty();
        for id in upnp_ids.iter().cloned() {
            self.last_restart.lock().insert(id.clone(), Instant::now());
            self.spawn_subscribe(id);
        }
        if upnp_started && !self.pc_muted.load(Ordering::Relaxed) {
            let me = self.clone();
            tauri::async_runtime::spawn(async move {
                let t0 = Instant::now();
                while t0.elapsed() < Duration::from_millis(3500) {
                    let pulling = upnp_ids.iter().any(|id| me.tap(id).map_or(false, |tap| tap.connected.load(Ordering::Relaxed)));
                    if pulling {
                        tokio::time::sleep(Duration::from_millis(1200)).await;
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                me.update_output_switch();
                me.notify_changed();
            });
        } else {
            self.update_output_switch();
        }
        self.notify_changed();
    }

    pub async fn deactivate_many(self: &Arc<Self>, ids: Vec<String>) {
        let running = self.running_ids();
        if self.pc_muted.load(Ordering::Relaxed) && running.iter().all(|id| ids.contains(id)) {
            self.release_output_switch();
            self.notify_changed();
        }
        // Grouped members leave their group instead of stopping it; members of a stopped coordinator that are
        // still wanted get a stream of their own back.
        let (leaving, orphans): (Vec<String>, Vec<String>) = {
            let sinks = self.sinks.lock();
            let leaving = ids.iter().filter(|id| matches!(sinks.get(*id).map(|s| &s.kind), Some(SinkKind::Follower(_)))).cloned().collect();
            let orphans = sinks
                .iter()
                .filter_map(|(fid, s)| match &s.kind {
                    SinkKind::Follower(c) if ids.contains(c) && !ids.contains(fid) => Some(fid.clone()),
                    _ => None,
                })
                .collect();
            (leaving, orphans)
        };
        let (stops, leaves): (Vec<upnp::Target>, Vec<upnp::Target>) = self.store.read(|c| {
            let targets = |list: &mut dyn Iterator<Item = &String>| -> Vec<upnp::Target> {
                list.filter_map(|id| c.speaker(id))
                    .filter(|s| matches!(s.protocol, Protocol::Sonos | Protocol::Upnp))
                    .filter_map(|s| self.target_for(s).ok())
                    .collect()
            };
            let stops = targets(&mut ids.iter().filter(|id| running.contains(*id) && !leaving.contains(*id)));
            let leaves = targets(&mut leaving.iter().chain(orphans.iter()));
            (stops, leaves)
        });
        self.stopping.lock().extend(ids.iter().cloned());
        let unsubs: Vec<(upnp::Target, String)> = {
            let mut subs = self.subs.lock();
            stops.iter().filter_map(|t| subs.remove(&t.id).map(|s| (t.clone(), s.sid))).collect()
        };
        join_all(leaves.iter().map(|t| async move {
            let _ = tokio::time::timeout(Duration::from_millis(1500), upnp::leave_group(&self.http, t)).await;
        }))
        .await;
        join_all(stops.iter().map(|t| async move {
            let _ = tokio::time::timeout(Duration::from_millis(1500), upnp::stop(&self.http, t)).await;
        }))
        .await;
        join_all(unsubs.iter().map(|(t, sid)| async move {
            let _ = tokio::time::timeout(Duration::from_millis(1000), upnp::unsubscribe(&self.http, t, sid)).await;
        }))
        .await;

        for id in ids.iter().chain(orphans.iter()) {
            self.teardown(id);
        }
        {
            let mut stopping = self.stopping.lock();
            for id in &ids {
                stopping.remove(id);
            }
        }
        self.store.write(|c| {
            for id in &ids {
                if let Some(s) = c.speaker_mut(id) {
                    s.active = false;
                }
            }
        });
        if !orphans.is_empty() {
            self.activate_many(orphans).await;
        }
        self.update_output_switch();
        self.notify_changed();
    }

    pub async fn set_send(self: &Arc<Self>, id: &str, on: bool) {
        if on {
            self.activate_many(vec![id.to_string()]).await;
        } else {
            self.deactivate_many(vec![id.to_string()]).await;
        }
    }

    pub async fn set_room_send(self: &Arc<Self>, room_id: &str, on: bool) {
        let online = self.online_ids();
        let ids: Vec<String> = self.store.read(|c| {
            c.speakers
                .iter()
                .filter(|s| s.room_id == room_id)
                .filter(|s| !on || online.contains(&s.id))
                .map(|s| s.id.clone())
                .collect()
        });
        if on {
            self.activate_many(ids).await;
        } else {
            self.deactivate_many(ids).await;
        }
    }

    pub async fn reactivate_wanted(self: &Arc<Self>) {
        let running = self.running_ids();
        let online = self.online_ids();
        let ids: Vec<String> = self.store.read(|c| {
            c.speakers
                .iter()
                .filter(|s| s.active && !running.contains(&s.id) && online.contains(&s.id))
                .map(|s| s.id.clone())
                .collect()
        });
        if !ids.is_empty() {
            log::info!("reconnexion automatique de {} enceinte(s)", ids.len());
            self.activate_many(ids).await;
        }
    }

    fn recover_output_switch(&self) {
        let (was, prev, dev) = self.store.read(|c| (c.pc_muted_by_us, c.pc_prev_mute, c.pc_muted_device.clone()));
        if was {
            log::info!("reprise après arrêt anormal : restauration des haut-parleurs du PC");
            let r = match &dev {
                Some(id) => output_switch::set_mute(id, prev),
                None => output_switch::set_default_render_mute(prev),
            };
            self.store.write(|c| {
                c.pc_muted_by_us = false;
                c.pc_muted_device = None;
                if let (Err(_), Some(id)) = (&r, dev) {
                    c.pc_pending_restore.push(PendingRestore { device_id: id, mute: prev });
                }
            });
        }
        self.retry_pending_restores();
        self.store.flush();
    }

    fn retry_pending_restores(&self) {
        let pending = self.store.read(|c| c.pc_pending_restore.clone());
        if pending.is_empty() {
            return;
        }
        let done: Vec<String> = pending
            .iter()
            .filter(|p| output_switch::set_mute(&p.device_id, p.mute).is_ok())
            .map(|p| p.device_id.clone())
            .collect();
        if !done.is_empty() {
            log::info!("Smart Output : {} sortie(s) restaurée(s) à leur retour", done.len());
            self.store.write(|c| c.pc_pending_restore.retain(|p| !done.contains(&p.device_id)));
            self.store.flush();
        }
    }

    pub fn update_output_switch(&self) {
        let any_active = self.streaming_sinks() && !self.line_in_active();
        let want = self.store.read(|c| c.settings.smart_output && !c.local_pc && matches!(c.source, Source::System)) && any_active;
        {
            let _g = self.switch_lock.lock();
            let engaged = self.pc_muted.load(Ordering::Relaxed);
            if want && !engaged {
                self.engage_output_switch();
            } else if !want && engaged {
                self.release_locked();
            }
        }
        self.output_notify.notify_one();
    }

    fn engage_output_switch(&self) {
        match output_switch::mute_default() {
            Ok(m) => {
                self.store.write(|c| {
                    c.pc_muted_by_us = true;
                    c.pc_prev_mute = m.prev;
                    c.pc_muted_device = Some(m.device_id.clone());
                    c.pc_pending_restore.retain(|p| p.device_id != m.device_id);
                });
                self.store.flush();
                self.pc_muted.store(true, Ordering::Relaxed);
                log::info!("Smart Output : sortie du PC coupée ({})", m.device_id);
            }
            Err(e) => log::warn!("Smart Output : coupure du PC impossible ({e})"),
        }
    }

    fn release_output_switch(&self) {
        let _g = self.switch_lock.lock();
        if self.pc_muted.load(Ordering::Relaxed) {
            self.release_locked();
        }
    }

    fn release_locked(&self) {
        let (prev, dev) = self.store.read(|c| (c.pc_prev_mute, c.pc_muted_device.clone()));
        let r = match &dev {
            Some(id) => output_switch::set_mute(id, prev),
            None => output_switch::set_default_render_mute(prev),
        };
        if let Err(e) = &r {
            log::warn!("Smart Output : restauration différée ({e})");
        }
        self.store.write(|c| {
            c.pc_muted_by_us = false;
            c.pc_muted_device = None;
            if let (Err(_), Some(id)) = (&r, dev) {
                c.pc_pending_restore.retain(|p| p.device_id != id);
                c.pc_pending_restore.push(PendingRestore { device_id: id, mute: prev });
            }
        });
        self.store.flush();
        self.pc_muted.store(false, Ordering::Relaxed);
        if r.is_ok() {
            log::info!("Smart Output : sortie du PC rétablie");
        }
        self.output_notify.notify_one();
    }

    async fn output_guard_loop(self: Arc<Self>) {
        loop {
            let engaged = self.pc_muted.load(Ordering::Relaxed);
            let pending = self.store.read(|c| !c.pc_pending_restore.is_empty());
            if !engaged && !pending {
                self.output_notify.notified().await;
                continue;
            }
            let _ = tokio::time::timeout(Duration::from_secs(2), self.output_notify.notified()).await;
            if self.pc_muted.load(Ordering::Relaxed) {
                let muted = self.store.read(|c| c.pc_muted_device.clone());
                let cur = tauri::async_runtime::spawn_blocking(output_switch::default_render_id).await;
                if let (Ok(Ok(cur)), Some(m)) = (cur, muted) {
                    if cur != m {
                        log::info!("Smart Output : la sortie par défaut a changé, coupure déplacée");
                        self.release_output_switch();
                        self.update_output_switch();
                        self.notify_changed();
                    }
                }
            }
            let me = self.clone();
            let _ = tauri::async_runtime::spawn_blocking(move || me.retry_pending_restores()).await;
        }
    }

    async fn discovery_loop(self: Arc<Self>) {
        let me = self.clone();
        match discovery::browse(move |a| me.on_native(a)) {
            Ok(d) => *self.mdns.lock() = Some(d),
            Err(e) => log::warn!("mDNS indisponible : {e}"),
        }
        let mut n = 0u32;
        let mut interval = 30u64;
        loop {
            self.scan_interval_secs.store(interval, Ordering::Relaxed);
            let (_, changed) = self.scan_inner(n % 10 == 0).await;
            *self.last_scan.lock() = Instant::now();
            self.reactivate_wanted().await;
            n += 1;
            let wanted_missing = {
                let running = self.running_ids();
                self.store.read(|c| c.speakers.iter().any(|s| s.active && !running.contains(&s.id)))
            };
            interval = if changed || wanted_missing || n < 3 { 30 } else { (interval * 2).min(120) };
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(interval)) => {}
                _ = self.scan_wake.notified() => { interval = 30; }
            }
        }
    }

    pub async fn scan(self: &Arc<Self>) -> usize {
        let r = self.scan_inner(true).await.0;
        *self.last_scan.lock() = Instant::now();
        r
    }

    async fn scan_inner(self: &Arc<Self>, all_ifaces: bool) -> (usize, bool) {
        let ifaces: Vec<std::net::Ipv4Addr> = {
            let productive = self.productive_ifaces.lock().clone();
            if all_ifaces || productive.is_empty() { network::lan_ipv4_addrs() } else { productive }
        };
        let (devices, productive) = upnp::discover(&self.http, Duration::from_millis(1200), ifaces, &self.desc_cache).await;
        if !productive.is_empty() {
            *self.productive_ifaces.lock() = productive;
        }
        let own: Vec<String> = if std::env::var_os("REZO_ALLOW_LOCAL_RENDERERS").is_some() {
            vec![]
        } else {
            network::lan_ipv4_addrs().iter().map(|a| a.to_string()).collect()
        };
        let mut seen = 0;
        let mut changed = false;
        for d in devices.iter().filter(|d| !own.contains(&d.ip)) {
            changed |= self.upsert_device(d);
            seen += 1;
        }

        if let Some(s) = devices.iter().find(|d| d.is_sonos) {
            if let Ok(members) = upnp::zone_members(&self.http, &s.ip).await {
                let hidden: Vec<String> = members.iter().filter(|m| m.invisible).map(|m| m.uuid.clone()).collect();
                self.store.write(|c| c.speakers.retain(|sp| !(hidden.contains(&sp.id) && !sp.active)));
                let mut rt = self.rt.lock();
                for m in members.iter().filter(|m| !m.invisible) {
                    let e = rt.entry(m.uuid.clone()).or_default();
                    let group = (m.group_size > 1).then(|| m.group_name.clone());
                    if e.group != group {
                        changed = true;
                    }
                    e.group = group;
                }
            }
        }

        {
            let stale = Duration::from_secs((self.scan_interval_secs.load(Ordering::Relaxed) * 5 / 2).max(100));
            let web: Vec<String> = self.store.read(|c| c.speakers.iter().filter(|s| s.protocol == Protocol::Web).map(|s| s.id.clone()).collect());
            let mut rt = self.rt.lock();
            for (id, e) in rt.iter_mut() {
                if !web.contains(id) && e.online && e.last_seen.map_or(true, |t| t.elapsed() > stale) {
                    e.online = false;
                    changed = true;
                }
            }
        }
        self.notify_changed();
        (seen, changed)
    }

    fn mark_seen(&self, id: &str) {
        let mut rt = self.rt.lock();
        let e = rt.entry(id.to_string()).or_default();
        e.online = true;
        e.last_seen = Some(Instant::now());
    }

    fn upsert_device(&self, d: &DeviceInfo) -> bool {
        let protocol = if d.is_sonos { Protocol::Sonos } else { Protocol::Upnp };
        let was_online = self.rt.lock().get(&d.udn).map_or(false, |r| r.online);
        let changed = self.store.write(|c| {
            if let Some(sp) = c.speaker_mut(&d.udn) {
                let moved = sp.address != d.ip || sp.port != d.port;
                sp.event_url = d.event_url.clone();
                sp.address = d.ip.clone();
                sp.port = d.port;
                sp.control_url = Some(d.av_control_url.clone());
                sp.rendering_url = d.rendering_url.clone();
                sp.line_in = d.line_in;
                sp.model = d.model.clone();
                sp.protocol = protocol;
                moved
            } else {
                let name = d.room_name.clone().unwrap_or_else(|| d.friendly_name.clone());
                let room_id = if d.is_sonos { c.room_by_name_or_create(&name) } else { String::new() };
                c.speakers.push(Speaker {
                    id: d.udn.clone(),
                    name,
                    room_id,
                    protocol,
                    address: d.ip.clone(),
                    port: d.port,
                    model: d.model.clone(),
                    uuid: d.is_sonos.then(|| d.udn.clone()),
                    control_url: Some(d.av_control_url.clone()),
                    event_url: d.event_url.clone(),
                    rendering_url: d.rendering_url.clone(),
                    line_in: d.line_in,
                    ..Speaker::default()
                });
                true
            }
        });
        self.mark_seen(&d.udn);
        changed || !was_online
    }

    fn on_native(self: &Arc<Self>, a: NativeAnnounce) {
        let id = format!("native-{}", a.id);
        self.store.write(|c| {
            if let Some(sp) = c.speaker_mut(&id) {
                sp.address = a.ip.to_string();
                sp.port = a.port;
            } else {
                c.speakers.push(Speaker {
                    id: id.clone(),
                    name: a.name.clone(),
                    room_id: String::new(),
                    protocol: Protocol::Native,
                    address: a.ip.to_string(),
                    port: a.port,
                    model: Some("Récepteur Rezo".into()),
                    ..Speaker::default()
                });
            }
        });
        self.mark_seen(&id);
        self.notify_changed();
        let me = self.clone();
        tauri::async_runtime::spawn(async move { me.reactivate_wanted().await });
    }

    pub async fn add_manual(self: &Arc<Self>, ip: &str, kind: &str, port: Option<u16>) -> Result<()> {
        let ip: std::net::IpAddr = ip.trim().parse().map_err(|_| anyhow!("adresse IP invalide"))?;
        if kind == "native" {
            let port = port.unwrap_or(5004);
            self.on_native(NativeAnnounce { id: format!("{ip}-{port}"), name: format!("Récepteur {ip}"), ip, port });
            return Ok(());
        }
        let mut last = anyhow!("aucun appareil UPnP/Sonos trouvé à cette adresse");
        for (p, path) in [(1400u16, "/xml/device_description.xml"), (49152, "/description.xml"), (49153, "/description.xml"), (8080, "/description.xml")] {
            match upnp::describe(&self.http, &format!("http://{ip}:{p}{path}")).await {
                Ok(d) => {
                    let _ = self.upsert_device(&d);
                    self.notify_changed();
                    return Ok(());
                }
                Err(e) => last = e,
            }
        }
        Err(last)
    }

    pub fn web_attach(self: &Arc<Self>, client_id: &str, name: &str) -> u64 {
        let id = format!("web-{client_id}");
        let token = self.token_seq.fetch_add(1, Ordering::Relaxed);
        let wanted = self.store.write(|c| {
            if let Some(s) = c.speaker(&id) {
                s.active
            } else {
                c.speakers.push(Speaker {
                    id: id.clone(),
                    name: name.to_string(),
                    protocol: Protocol::Web,
                    model: Some("Navigateur".into()),
                    active: true,
                    ..Speaker::default()
                });
                true
            }
        });
        self.mark_seen(&id);
        self.web_tokens.lock().insert(id.clone(), token);
        if wanted {
            let me = self.clone();
            tauri::async_runtime::spawn(async move { me.activate_many(vec![id]).await });
        }
        self.notify_changed();
        token
    }

    pub fn web_detach(&self, client_id: &str, token: u64) {
        let id = format!("web-{client_id}");
        let mut tokens = self.web_tokens.lock();
        if tokens.get(&id) == Some(&token) {
            tokens.remove(&id);
            drop(tokens);
            self.teardown(&id);
            self.rt.lock().entry(id).or_default().online = false;
            self.update_output_switch();
            self.notify_changed();
        }
    }

    pub fn web_stats(&self, client_id: &str, buf_ms: u32) {
        let id = format!("web-{client_id}");
        if let Some(e) = self.rt.lock().get_mut(&id) {
            e.client_buf_ms = buf_ms;
        }
        if let Some(t) = self.tap(&id) {
            t.client_buf_ms.store(buf_ms, Ordering::Relaxed);
        }
    }

    pub fn receiver_info(&self, ip: Option<String>) -> Result<ReceiverInfo> {
        let ips: Vec<String> = network::lan_ipv4_addrs().iter().map(|a| a.to_string()).collect();
        let chosen = ip.or_else(|| ips.first().cloned()).ok_or_else(|| anyhow!("aucune adresse réseau locale détectée"))?;
        let url = format!("http://{chosen}:{}/r/{}", self.http_port.load(Ordering::Relaxed), self.access_key());
        let code = qrcode::QrCode::new(url.as_bytes())?;
        let qr_svg = code
            .render::<qrcode::render::svg::Color>()
            .min_dimensions(220, 220)
            .quiet_zone(true)
            .dark_color(qrcode::render::svg::Color("#0b0d10"))
            .light_color(qrcode::render::svg::Color("#ffffff"))
            .build();
        Ok(ReceiverInfo { url, qr_svg, ips })
    }

    fn spawn_subscribe(self: &Arc<Self>, id: String) {
        let me = self.clone();
        tauri::async_runtime::spawn(async move {
            let Some(sp) = me.store.read(|c| c.speaker(&id).cloned()) else { return };
            let Ok(t) = me.target_for(&sp) else { return };
            if t.event.is_none() {
                return;
            }
            let Some(local) = network::local_ip_for(&sp.address) else { return };
            let callback = format!("http://{local}:{}/s/{}/event/{id}", me.http_port.load(Ordering::Relaxed), me.access_key());
            match upnp::subscribe(&me.http, &t, &callback).await {
                Ok((sid, secs)) => {
                    log::info!("{id} : abonné aux événements UPnP ({secs} s)");
                    let renew = Duration::from_secs(secs.saturating_sub(120).max(60));
                    me.subs.lock().insert(id, Sub { sid, renew_at: Instant::now() + renew });
                }
                Err(e) => log::info!("{id} : événements UPnP indisponibles ({e}) — sondage de secours"),
            }
        });
    }

    pub fn on_transport_event(self: &Arc<Self>, id: &str, state: Option<String>, uri: Option<String>) {
        let streamed = matches!(self.sinks.lock().get(id).map(|s| &s.kind), Some(SinkKind::Tap));
        if !streamed || self.stopping.lock().contains(id) {
            return;
        }
        let stopped = matches!(state.as_deref(), Some("STOPPED" | "PAUSED_PLAYBACK" | "NO_MEDIA_PRESENT"));
        let hijacked = uri.as_deref().map_or(false, |u| !u.is_empty() && !u.contains(&format!("/stream/{id}.wav")));
        if !(stopped || hijacked) {
            return;
        }
        {
            let mut last = self.last_restart.lock();
            if last.get(id).map_or(false, |t| t.elapsed() < Duration::from_secs(5)) {
                return;
            }
            last.insert(id.to_string(), Instant::now());
        }
        log::info!("{id} : lecture interrompue (état {state:?}), relance");
        let me = self.clone();
        let id = id.to_string();
        tauri::async_runtime::spawn(async move {
            let Some(sp) = me.store.read(|c| c.speaker(&id).cloned()) else { return };
            if let Ok(t) = me.target_for(&sp) {
                let _ = upnp::ensure_playing(&me.http, &t).await;
            }
        });
    }

    async fn watchdog_loop(self: Arc<Self>) {
        let mut last_poll: HashMap<String, Instant> = HashMap::new();
        loop {
            let running = self.tap_sink_ids();
            let upnp_running: Vec<Speaker> = self.store.read(|c| {
                c.speakers.iter().filter(|s| running.contains(&s.id) && matches!(s.protocol, Protocol::Sonos | Protocol::Upnp)).cloned().collect()
            });
            let all_subscribed = { let subs = self.subs.lock(); upnp_running.iter().all(|s| subs.contains_key(&s.id)) };
            tokio::time::sleep(Duration::from_secs(if all_subscribed { 60 } else { 10 })).await;

            let due: Vec<(String, String)> = {
                let subs = self.subs.lock();
                subs.iter().filter(|(_, s)| s.renew_at <= Instant::now()).map(|(id, s)| (id.clone(), s.sid.clone())).collect()
            };
            for (id, sid) in due {
                let Some(sp) = self.store.read(|c| c.speaker(&id).cloned()) else { continue };
                let Ok(t) = self.target_for(&sp) else { continue };
                match upnp::renew(&self.http, &t, &sid).await {
                    Ok(secs) => {
                        if let Some(s) = self.subs.lock().get_mut(&id) {
                            s.renew_at = Instant::now() + Duration::from_secs(secs.saturating_sub(120).max(60));
                        }
                    }
                    Err(_) => {
                        self.subs.lock().remove(&id);
                        self.spawn_subscribe(id);
                    }
                }
            }

            let running = self.tap_sink_ids();
            let targets: Vec<upnp::Target> = {
                let subs = self.subs.lock();
                self.store.read(|c| {
                    c.speakers
                        .iter()
                        .filter(|s| running.contains(&s.id) && matches!(s.protocol, Protocol::Sonos | Protocol::Upnp))
                        .filter(|s| {
                            let every = if subs.contains_key(&s.id) { 120 } else { 10 };
                            last_poll.get(&s.id).map_or(true, |t| t.elapsed() >= Duration::from_secs(every))
                        })
                        .filter_map(|s| self.target_for(s).ok())
                        .collect()
                })
            };
            if targets.is_empty() {
                continue;
            }
            for t in &targets {
                last_poll.insert(t.id.clone(), Instant::now());
            }
            let me = &self;
            let results = join_all(targets.iter().map(|t| async move { (t.id.clone(), upnp::ensure_playing(&me.http, t).await) })).await;
            for (id, r) in results {
                match r {
                    Ok(true) => log::info!("{id} : lecture relancée par la veille"),
                    Ok(false) => {}
                    Err(e) => log::debug!("{id} : veille — {e}"),
                }
            }
        }
    }

    pub async fn set_device_volume(self: &Arc<Self>, id: &str, volume: u32) -> Result<()> {
        let volume = volume.min(100);
        let protocol = self.store.read(|c| c.speaker(id).map(|s| s.protocol)).ok_or_else(|| anyhow!("enceinte inconnue"))?;
        match protocol {
            Protocol::Native | Protocol::Web => {
                self.store.write(|c| {
                    if let Some(s) = c.speaker_mut(id) {
                        s.receiver_volume = volume;
                    }
                });
                if let Some(Sink { kind: SinkKind::Udp(_, v), .. }) = self.sinks.lock().get(id) {
                    v.store(volume, Ordering::Relaxed);
                }
                if let Some(t) = self.tap(id) {
                    t.volume.store(volume, Ordering::Relaxed);
                }
            }
            Protocol::Sonos | Protocol::Upnp => {
                {
                    let mut rt = self.rt.lock();
                    let e = rt.entry(id.to_string()).or_default();
                    e.device_volume = Some(volume);
                    e.volume_set_at = Some(Instant::now());
                }
                // While a fader moves only the latest value matters: one request in flight per speaker.
                if self.volume_pending.lock().insert(id.to_string(), volume).is_none() {
                    let me = self.clone();
                    let id = id.to_string();
                    tauri::async_runtime::spawn(async move { me.flush_volume(id).await });
                }
            }
        }
        self.notify_changed();
        Ok(())
    }

    async fn flush_volume(self: Arc<Self>, id: String) {
        loop {
            let Some(v) = self.volume_pending.lock().get(&id).copied() else { return };
            if let Some(sp) = self.store.read(|c| c.speaker(&id).cloned()) {
                if let Some(path) = sp.rendering_path() {
                    if let Err(e) = upnp::set_volume(&self.http, &sp.address, sp.port, &path, v).await {
                        log::warn!("{id} : réglage du volume impossible ({e})");
                    }
                }
            }
            let mut pending = self.volume_pending.lock();
            if pending.get(&id) == Some(&v) {
                pending.remove(&id);
                return;
            }
        }
    }

    // Reads the volume of Sonos/UPnP speakers while the window is visible, so changes made in the Sonos app show up.
    async fn volume_loop(self: Arc<Self>) {
        loop {
            if !self.ui_visible.load(Ordering::Acquire) {
                let woken = self.ui_notify.notified();
                tokio::pin!(woken);
                woken.as_mut().enable();
                if !self.ui_visible.load(Ordering::Acquire) {
                    woken.await;
                }
                continue;
            }
            let online = self.online_ids();
            let busy: HashSet<String> = {
                let pending = self.volume_pending.lock();
                let rt = self.rt.lock();
                rt.iter()
                    .filter(|(id, e)| pending.contains_key(*id) || e.volume_set_at.map_or(false, |t| t.elapsed() < Duration::from_secs(3)))
                    .map(|(id, _)| id.clone())
                    .collect()
            };
            let targets: Vec<(String, String, u16, String)> = self.store.read(|c| {
                c.speakers
                    .iter()
                    .filter(|s| matches!(s.protocol, Protocol::Sonos | Protocol::Upnp) && online.contains(&s.id) && !busy.contains(&s.id))
                    .filter_map(|s| s.rendering_path().map(|p| (s.id.clone(), s.address.clone(), s.port, p)))
                    .collect()
            });
            let me = &self;
            let results = join_all(targets.iter().map(|(id, ip, port, path)| async move {
                (id.clone(), tokio::time::timeout(Duration::from_millis(1500), upnp::get_volume(&me.http, ip, *port, path)).await)
            }))
            .await;
            let mut changed = false;
            {
                let mut rt = self.rt.lock();
                for (id, r) in results {
                    let e = rt.entry(id).or_default();
                    if e.volume_set_at.map_or(false, |t| t.elapsed() < Duration::from_secs(3)) {
                        continue;
                    }
                    if let Ok(Ok(v)) = r {
                        if e.device_volume != Some(v) {
                            e.device_volume = Some(v);
                            changed = true;
                        }
                    }
                }
            }
            if changed {
                self.notify_changed();
            }
            let _ = tokio::time::timeout(Duration::from_secs(4), self.ui_notify.notified()).await;
        }
    }

    pub async fn set_use_line_in(self: &Arc<Self>, id: &str, on: bool) -> Result<()> {
        self.store.write(|c| -> Result<()> {
            let sp = c.speaker_mut(id).ok_or_else(|| anyhow!("enceinte inconnue"))?;
            if on && !(sp.protocol == Protocol::Sonos && sp.line_in) {
                bail!("cette enceinte n'a pas d'entrée ligne");
            }
            sp.use_line_in = on;
            Ok(())
        })?;
        if self.sinks.lock().contains_key(id) {
            self.deactivate_many(vec![id.to_string()]).await;
            self.activate_many(vec![id.to_string()]).await;
        }
        self.notify_changed();
        Ok(())
    }

    // First press: active Sonos players join one Sonos group (the player keeps them sample-aligned, as when
    // grouped in the Sonos app) and the other speakers are delayed to the slowest path with per-type estimates.
    // Second press, while a group exists: every member gets its own stream back.
    pub async fn sync_speakers(self: &Arc<Self>) -> SyncReport {
        let mut report = SyncReport { grouped: vec![], delayed: vec![], ungrouped: vec![] };
        let followers: Vec<String> =
            self.sinks.lock().iter().filter(|(_, s)| matches!(s.kind, SinkKind::Follower(_))).map(|(id, _)| id.clone()).collect();
        if !followers.is_empty() {
            let (targets, names): (Vec<upnp::Target>, Vec<String>) = self.store.read(|c| {
                let sps: Vec<&Speaker> = followers.iter().filter_map(|id| c.speaker(id)).collect();
                (sps.iter().filter_map(|s| self.target_for(s).ok()).collect(), sps.iter().map(|s| s.name.clone()).collect())
            });
            join_all(targets.iter().map(|t| async move {
                let _ = tokio::time::timeout(Duration::from_millis(1500), upnp::leave_group(&self.http, t)).await;
            }))
            .await;
            for id in &followers {
                self.teardown(id);
            }
            self.activate_many(followers).await;
            report.ungrouped = names;
            log::info!("Sync : groupe Sonos dissous");
            self.notify_changed();
            return report;
        }

        let kinds: HashMap<String, bool> = self.sinks.lock().iter().map(|(id, s)| (id.clone(), s.kind.streams())).collect();
        let sonos: Vec<Speaker> = self.store.read(|c| {
            c.speakers.iter().filter(|s| s.protocol == Protocol::Sonos && s.uuid.is_some() && kinds.contains_key(&s.id)).cloned().collect()
        });
        if sonos.len() >= 2 {
            let coord = sonos.iter().find(|s| s.plays_line_in()).or_else(|| sonos.iter().find(|s| s.line_in)).unwrap_or(&sonos[0]).clone();
            let coord_uuid = coord.uuid.clone().unwrap_or_default();
            for m in sonos.iter().filter(|s| s.id != coord.id) {
                let Ok(t) = self.target_for(m) else { continue };
                let sub = self.subs.lock().remove(&m.id);
                if let Some(sub) = sub {
                    let _ = tokio::time::timeout(Duration::from_millis(1000), upnp::unsubscribe(&self.http, &t, &sub.sid)).await;
                }
                self.teardown(&m.id);
                match upnp::join_group(&self.http, &t, &coord_uuid).await {
                    Ok(()) => {
                        let shared = ChannelShared::new(&m.params);
                        self.sinks.lock().insert(m.id.clone(), Sink { kind: SinkKind::Follower(coord.id.clone()), shared });
                        report.grouped.push(m.name.clone());
                    }
                    Err(e) => {
                        log::warn!("Sync : {} n'a pas pu rejoindre {} ({e})", m.name, coord.name);
                        self.activate_many(vec![m.id.clone()]).await;
                    }
                }
            }
            if !report.grouped.is_empty() {
                report.grouped.insert(0, coord.name.clone());
                log::info!("Sync : groupe Sonos {}", report.grouped.join(" + "));
            }
        }

        let kinds: HashMap<String, bool> = self.sinks.lock().iter().map(|(id, s)| (id.clone(), s.kind.streams())).collect();
        let rt = self.rt.lock().clone();
        let estimates: Vec<(String, String, f32, u32)> = self.store.read(|c| {
            c.speakers
                .iter()
                .filter(|s| kinds.get(&s.id).copied().unwrap_or(false))
                .filter_map(|s| {
                    let buf = rt.get(&s.id).map_or(0, |r| r.client_buf_ms);
                    Self::latency_estimate(s, true, &c.settings, buf).map(|l| (s.id.clone(), s.name.clone(), s.params.delay_ms, l))
                })
                .collect()
        });
        if estimates.len() >= 2 {
            let latencies: Vec<u32> = estimates.iter().map(|e| e.3).collect();
            for ((id, name, current, _), d) in estimates.into_iter().zip(alignment_delays(&latencies)) {
                if (current - d as f32).abs() > 0.5 {
                    let _ = self.set_param(&id, "delayMs", &serde_json::json!(d));
                }
                if d > 0 {
                    report.delayed.push((name, d));
                }
            }
        }
        self.update_output_switch();
        self.notify_changed();
        report
    }

    pub fn rename_speaker(&self, id: &str, name: &str) {
        self.store.write(|c| {
            if let Some(s) = c.speaker_mut(id) {
                s.name = name.trim().chars().take(60).collect();
            }
        });
    }

    pub fn set_speaker_room(&self, id: &str, room_id: &str) {
        self.store.write(|c| {
            if let Some(s) = c.speaker_mut(id) {
                s.room_id = room_id.to_string();
            }
        });
        let room = self.store.read(|c| c.room(room_id).cloned());
        if let Some(s) = self.sinks.lock().get(id) {
            match room {
                Some(r) => s.shared.set_room(r.volume_db, r.mute, r.solo),
                None => s.shared.set_room(0.0, false, false),
            }
        }
    }

    pub async fn remove_speaker(self: &Arc<Self>, id: &str) {
        self.deactivate_many(vec![id.to_string()]).await;
        self.store.write(|c| c.speakers.retain(|s| s.id != id));
        self.rt.lock().remove(id);
        self.notify_changed();
    }

    pub fn create_room(&self, name: &str, icon: &str, color: &str) -> String {
        self.store.write(|c| {
            let id = c.unique_room_id(&slug(name));
            c.rooms.push(Room { id: id.clone(), name: name.trim().chars().take(40).collect(), icon: icon.into(), color: color.into(), ..Room::default() });
            id
        })
    }

    pub fn update_room(&self, id: &str, name: Option<String>, icon: Option<String>, color: Option<String>) {
        self.store.write(|c| {
            if let Some(r) = c.rooms.iter_mut().find(|r| r.id == id) {
                if let Some(n) = name {
                    r.name = n.trim().chars().take(40).collect();
                }
                if let Some(i) = icon {
                    r.icon = i;
                }
                if let Some(col) = color {
                    r.color = col;
                }
            }
        });
    }

    pub fn delete_room(&self, id: &str) {
        self.store.write(|c| {
            c.rooms.retain(|r| r.id != id);
            for s in c.speakers.iter_mut().filter(|s| s.room_id == id) {
                s.room_id.clear();
            }
        });
    }

    pub fn save_scene(&self, name: &str) -> String {
        self.store.write(|c| {
            let base = slug(name);
            let mut id = if base.is_empty() { "scene".to_string() } else { base };
            let mut n = 2;
            while c.scenes.iter().any(|s| s.id == id) {
                id = format!("{id}-{n}");
                n += 1;
            }
            let scene = Scene {
                id: id.clone(),
                name: name.trim().chars().take(40).collect(),
                master_db: c.master_db,
                speakers: c.speakers.iter().map(|s| (s.id.clone(), SceneSpeaker { active: s.active, params: s.params.clone() })).collect(),
                rooms: c.rooms.iter().map(|r| (r.id.clone(), SceneRoom { volume_db: r.volume_db, mute: r.mute, solo: r.solo })).collect(),
            };
            c.scenes.push(scene);
            id
        })
    }

    pub fn delete_scene(&self, id: &str) {
        self.store.write(|c| c.scenes.retain(|s| s.id != id));
    }

    pub fn rename_scene(&self, id: &str, name: &str) {
        self.store.write(|c| {
            if let Some(s) = c.scenes.iter_mut().find(|s| s.id == id) {
                s.name = name.trim().chars().take(40).collect();
            }
        });
    }

    pub async fn apply_scene(self: &Arc<Self>, id: &str) -> Result<()> {
        let scene = self.store.read(|c| c.scenes.iter().find(|s| s.id == id).cloned()).ok_or_else(|| anyhow!("scène inconnue"))?;
        self.set_master(scene.master_db);
        self.store.write(|c| {
            for (rid, sr) in &scene.rooms {
                if let Some(r) = c.rooms.iter_mut().find(|r| &r.id == rid) {
                    r.volume_db = sr.volume_db;
                    r.mute = sr.mute;
                    r.solo = sr.solo;
                }
            }
            for (sid, ss) in &scene.speakers {
                if let Some(s) = c.speaker_mut(sid) {
                    s.params = ss.params.clone();
                }
            }
        });
        let room_ids: Vec<String> = self.store.read(|c| c.rooms.iter().map(|r| r.id.clone()).collect());
        for rid in room_ids {
            self.refresh_room(&rid);
        }
        {
            let shared: HashMap<String, Arc<ChannelShared>> =
                self.sinks.lock().iter().map(|(k, s)| (k.clone(), s.shared.clone())).collect();
            self.store.read(|c| {
                for sid in scene.speakers.keys() {
                    if let (Some(sh), Some(sp)) = (shared.get(sid), c.speaker(sid)) {
                        sh.load(&sp.params);
                    }
                }
            });
        }
        let running = self.running_ids();
        let known: HashSet<String> = self.store.read(|c| c.speakers.iter().map(|s| s.id.clone()).collect());
        let to_on: Vec<String> = scene.speakers.iter().filter(|(id, s)| s.active && known.contains(*id) && !running.contains(*id)).map(|(id, _)| id.clone()).collect();
        let to_off: Vec<String> = scene.speakers.iter().filter(|(id, s)| !s.active && running.contains(*id)).map(|(id, _)| id.clone()).collect();
        let offline_on: Vec<String> = {
            let rt = self.rt.lock();
            to_on.iter().filter(|id| !rt.get(*id).map_or(false, |r| r.online)).cloned().collect()
        };
        self.store.write(|c| {
            for id in &offline_on {
                if let Some(s) = c.speaker_mut(id) {
                    s.active = true;
                }
            }
        });
        let online_on: Vec<String> = to_on.into_iter().filter(|id| !offline_on.contains(id)).collect();
        self.deactivate_many(to_off).await;
        self.activate_many(online_on).await;
        self.notify_changed();
        Ok(())
    }

    pub fn set_midi_map(&self, map: Vec<MidiBinding>) {
        self.store.write(|c| c.midi_map = map);
    }

    pub async fn set_settings(self: &Arc<Self>, new: Settings) {
        let old = self.store.write(|c| {
            let mut new = new.clone();
            new.language = c.settings.language.clone();
            std::mem::replace(&mut c.settings, new)
        });
        if old.pin_engine_period != new.pin_engine_period {
            self.restart_capture();
        }
        if old.udp_frame_ms != new.udp_frame_ms || old.bitrate_kbps != new.bitrate_kbps || old.redundancy != new.redundancy {
            let running = self.running_ids();
            let ids: Vec<String> = self.store.read(|c| {
                c.speakers.iter().filter(|s| s.protocol == Protocol::Native && running.contains(&s.id)).map(|s| s.id.clone()).collect()
            });
            for id in &ids {
                self.teardown(id);
            }
            self.activate_many(ids).await;
        }
        self.update_output_switch();
        self.notify_changed();
    }

    pub async fn shutdown(self: &Arc<Self>) {
        if self.pc_muted.load(Ordering::Relaxed) {
            self.release_output_switch();
        }
        let running = self.running_ids();
        let stops: Vec<upnp::Target> = self.store.read(|c| {
            c.speakers
                .iter()
                .filter(|s| running.contains(&s.id) && matches!(s.protocol, Protocol::Sonos | Protocol::Upnp))
                .filter_map(|s| self.target_for(s).ok())
                .collect()
        });
        join_all(stops.iter().map(|t| async move {
            let _ = tokio::time::timeout(Duration::from_millis(1500), upnp::stop(&self.http, t)).await;
        }))
        .await;
        let ids: Vec<String> = running.into_iter().collect();
        for id in ids {
            self.teardown(&id);
        }
        if self.pc_muted.load(Ordering::Relaxed) {
            self.release_output_switch();
        }
        if let Some(h) = self.capture.lock().take() {
            h.stop();
        }
        self.store.write(|_| {});
        self.store.flush();
    }
}
