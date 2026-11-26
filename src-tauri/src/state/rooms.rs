use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

pub const DB_MIN: f32 = -90.0;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum ChannelMode {
    #[default]
    Stereo,
    Mono,
    Left,
    Right,
}

impl ChannelMode {
    pub fn as_u8(self) -> u8 {
        match self {
            ChannelMode::Stereo => 0,
            ChannelMode::Mono => 1,
            ChannelMode::Left => 2,
            ChannelMode::Right => 3,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ChannelParams {
    pub gain_db: f32,
    pub volume_db: f32,
    pub mute: bool,
    pub solo: bool,
    pub low_db: f32,
    pub mid_db: f32,
    pub high_db: f32,
    pub pan: f32,
    pub mode: ChannelMode,
    pub phase_invert: bool,
    pub delay_ms: f32,
    pub bass_db: f32,
    pub presence_db: f32,
    pub loudness: f32,
    pub width: f32,
    pub comp_on: bool,
    pub comp_thresh_db: f32,
    pub comp_ratio: f32,
    pub comp_makeup_db: f32,
    pub gate_on: bool,
    pub gate_thresh_db: f32,
    // Six-band equaliser (60 Hz, 150 Hz, 400 Hz, 1 kHz, 2.4 kHz, 15 kHz); off restores 0 dB but keeps the curve.
    pub eq_enabled: bool,
    pub eq_preset: String,
    pub eq_gains: [f32; 6],
}

impl Default for ChannelParams {
    fn default() -> Self {
        Self {
            gain_db: 0.0,
            volume_db: 0.0,
            mute: false,
            solo: false,
            low_db: 0.0,
            mid_db: 0.0,
            high_db: 0.0,
            pan: 0.0,
            mode: ChannelMode::Stereo,
            phase_invert: false,
            delay_ms: 0.0,
            bass_db: 0.0,
            presence_db: 0.0,
            loudness: 0.0,
            width: 1.0,
            comp_on: false,
            comp_thresh_db: -18.0,
            comp_ratio: 3.0,
            comp_makeup_db: 0.0,
            gate_on: false,
            gate_thresh_db: -55.0,
            eq_enabled: true,
            eq_preset: "flat".into(),
            eq_gains: [0.0; 6],
        }
    }
}

impl ChannelParams {
    pub fn clamp(&mut self) {
        let c = |v: f32, lo: f32, hi: f32| if v.is_finite() { v.clamp(lo, hi) } else { 0.0 };
        self.gain_db = c(self.gain_db, -12.0, 12.0);
        self.volume_db = c(self.volume_db, DB_MIN, 6.0);
        self.low_db = c(self.low_db, -12.0, 12.0);
        self.mid_db = c(self.mid_db, -12.0, 12.0);
        self.high_db = c(self.high_db, -12.0, 12.0);
        self.pan = c(self.pan, -1.0, 1.0);
        self.bass_db = c(self.bass_db, 0.0, 12.0);
        self.presence_db = c(self.presence_db, -6.0, 6.0);
        self.loudness = c(self.loudness, 0.0, 1.0);
        self.width = if self.width.is_finite() { self.width.clamp(0.0, 2.0) } else { 1.0 };
        self.comp_thresh_db = if self.comp_thresh_db.is_finite() { self.comp_thresh_db.clamp(-40.0, 0.0) } else { -18.0 };
        self.comp_ratio = if self.comp_ratio.is_finite() { self.comp_ratio.clamp(1.0, 10.0) } else { 3.0 };
        self.comp_makeup_db = c(self.comp_makeup_db, 0.0, 12.0);
        self.gate_thresh_db = if self.gate_thresh_db.is_finite() { self.gate_thresh_db.clamp(-80.0, -20.0) } else { -55.0 };
        self.delay_ms = (c(self.delay_ms, -500.0, 500.0) * 10.0).round() / 10.0;
        for g in self.eq_gains.iter_mut() {
            *g = (c(*g, -12.0, 12.0) * 10.0).round() / 10.0;
        }
        if self.eq_preset.is_empty() || self.eq_preset.len() > 32 {
            self.eq_preset = "custom".into();
        }
    }

    pub fn set(&mut self, key: &str, v: &serde_json::Value) -> Result<()> {
        let f = || v.as_f64().map(|x| x as f32).ok_or_else(|| anyhow::anyhow!("nombre attendu pour {key}"));
        let b = || v.as_bool().ok_or_else(|| anyhow::anyhow!("booléen attendu pour {key}"));
        match key {
            "gainDb" => self.gain_db = f()?,
            "volumeDb" => self.volume_db = f()?,
            "mute" => self.mute = b()?,
            "solo" => self.solo = b()?,
            "lowDb" => self.low_db = f()?,
            "midDb" => self.mid_db = f()?,
            "highDb" => self.high_db = f()?,
            "pan" => self.pan = f()?,
            "phaseInvert" => self.phase_invert = b()?,
            "delayMs" => self.delay_ms = f()?,
            "bassDb" => self.bass_db = f()?,
            "presenceDb" => self.presence_db = f()?,
            "loudness" => self.loudness = f()?,
            "width" => self.width = f()?,
            "compOn" => self.comp_on = b()?,
            "compThreshDb" => self.comp_thresh_db = f()?,
            "compRatio" => self.comp_ratio = f()?,
            "compMakeupDb" => self.comp_makeup_db = f()?,
            "gateOn" => self.gate_on = b()?,
            "gateThreshDb" => self.gate_thresh_db = f()?,
            "eqEnabled" => self.eq_enabled = b()?,
            "eqPreset" => self.eq_preset = v.as_str().ok_or_else(|| anyhow::anyhow!("texte attendu pour {key}"))?.to_string(),
            "eqGains" => {
                let a = v.as_array().filter(|a| a.len() == 6).ok_or_else(|| anyhow::anyhow!("6 gains attendus"))?;
                for (g, x) in self.eq_gains.iter_mut().zip(a) {
                    *g = x.as_f64().ok_or_else(|| anyhow::anyhow!("gain invalide"))? as f32;
                }
            }
            "mode" => {
                self.mode = serde_json::from_value(v.clone())
                    .map_err(|_| anyhow::anyhow!("mode invalide"))?
            }
            _ => anyhow::bail!("paramètre inconnu : {key}"),
        }
        self.clamp();
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    Native,
    Sonos,
    Upnp,
    Web,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Speaker {
    pub id: String,
    pub name: String,
    pub room_id: String,
    pub protocol: Protocol,
    pub address: String,
    pub port: u16,
    pub model: Option<String>,
    pub uuid: Option<String>,
    pub control_url: Option<String>,
    pub event_url: Option<String>,
    pub rendering_url: Option<String>,
    pub line_in: bool,
    pub use_line_in: bool,
    pub receiver_volume: u32,
    pub active: bool,
    pub params: ChannelParams,
    // Personal touches: a colour and an icon of its own (else the room's), and its place on the floor plan (0..1).
    pub color: Option<String>,
    pub icon: Option<String>,
    pub plan_x: Option<f32>,
    pub plan_y: Option<f32>,
}

impl Default for Speaker {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            room_id: String::new(),
            protocol: Protocol::Native,
            address: String::new(),
            port: 0,
            model: None,
            uuid: None,
            control_url: None,
            event_url: None,
            rendering_url: None,
            line_in: false,
            use_line_in: false,
            receiver_volume: 100,
            active: false,
            params: ChannelParams::default(),
            color: None,
            icon: None,
            plan_x: None,
            plan_y: None,
        }
    }
}

impl Speaker {
    // Sonos players expose RenderingControl at a fixed path; other renderers only through their description.
    pub fn rendering_path(&self) -> Option<String> {
        self.rendering_url.clone().or_else(|| (self.protocol == Protocol::Sonos).then(|| "/MediaRenderer/RenderingControl/Control".into()))
    }

    pub fn plays_line_in(&self) -> bool {
        self.protocol == Protocol::Sonos && self.line_in && self.use_line_in && self.uuid.is_some()
    }

    // Colours are "#rrggbb" and icons short names from the interface's list; anything else is dropped.
    pub fn sanitize_style(&mut self) {
        if !self.color.as_deref().map_or(true, is_hex_color) {
            self.color = None;
        }
        if !self.icon.as_deref().map_or(true, |i| !i.is_empty() && i.len() <= 24 && i.chars().all(|c| c.is_ascii_alphanumeric())) {
            self.icon = None;
        }
        let pos = |v: Option<f32>| v.filter(|x| x.is_finite()).map(|x| x.clamp(0.0, 1.0));
        self.plan_x = pos(self.plan_x);
        self.plan_y = pos(self.plan_y);
        if self.plan_x.is_none() || self.plan_y.is_none() {
            self.plan_x = None;
            self.plan_y = None;
        }
    }
}

pub fn is_hex_color(c: &str) -> bool {
    c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|ch| ch.is_ascii_hexdigit())
}

pub fn clamp_zoom(z: f32) -> f32 {
    if z.is_finite() { (z.clamp(0.7, 1.6) * 20.0).round() / 20.0 } else { 1.0 }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Room {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub color: String,
    pub volume_db: f32,
    pub mute: bool,
    pub solo: bool,
}

impl Default for Room {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            icon: "Speaker".into(),
            color: "#1c7ed6".into(),
            volume_db: 0.0,
            mute: false,
            solo: false,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SceneSpeaker {
    pub active: bool,
    pub params: ChannelParams,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SceneRoom {
    pub volume_db: f32,
    pub mute: bool,
    pub solo: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Scene {
    pub id: String,
    pub name: String,
    pub master_db: f32,
    pub speakers: HashMap<String, SceneSpeaker>,
    pub rooms: HashMap<String, SceneRoom>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct MidiBinding {
    pub control: String,
    pub status: u8,
    pub data1: u8,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Source {
    System,
    App { pid: u32, name: String },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub udp_frame_ms: f32,
    pub bitrate_kbps: u32,
    pub redundancy: bool,
    pub smart_output: bool,
    pub pin_engine_period: bool,
    pub keep_ui_alive: bool,
    pub http_port: u16,
    pub language: String,
    pub sonos_low_latency: bool,
    pub check_updates: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            udp_frame_ms: 5.0,
            bitrate_kbps: 160,
            redundancy: true,
            smart_output: false,
            pin_engine_period: false,
            keep_ui_alive: false,
            http_port: 49494,
            language: String::new(),
            sonos_low_latency: true,
            check_updates: true,
        }
    }
}

pub const WIDGET_KINDS: [&str; 5] = ["player", "volume", "rooms", "scenes", "meter"];
pub const WIDGET_LAYERS: [&str; 4] = ["top", "normal", "bottom", "desktop"];

// A small borderless window left on the desktop, like a Rainmeter skin. Kind and layer stay strings so that an
// unknown value from a newer version cannot make the whole configuration unreadable.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Widget {
    pub id: String,
    pub kind: String,
    // Top-left corner in physical pixels; None places it near the bottom-right corner of the main screen.
    pub x: Option<i32>,
    pub y: Option<i32>,
    // "top" above every window, "normal", "bottom" below every window, "desktop" on the desktop (stays visible
    // when "Show desktop" hides the windows).
    pub layer: String,
    pub opacity: f32,
    pub scale: f32,
    pub locked: bool,
    pub click_through: bool,
    pub visible: bool,
}

impl Default for Widget {
    fn default() -> Self {
        Self {
            id: String::new(),
            kind: "player".into(),
            x: None,
            y: None,
            layer: "normal".into(),
            opacity: 0.72,
            scale: 1.0,
            locked: false,
            click_through: false,
            visible: true,
        }
    }
}

impl Widget {
    pub fn clamp(&mut self) {
        if !WIDGET_KINDS.contains(&self.kind.as_str()) {
            self.kind = "player".into();
        }
        if !WIDGET_LAYERS.contains(&self.layer.as_str()) {
            self.layer = "normal".into();
        }
        self.opacity = if self.opacity.is_finite() { (self.opacity.clamp(0.0, 1.0) * 100.0).round() / 100.0 } else { 0.72 };
        self.scale = if self.scale.is_finite() { (self.scale.clamp(0.6, 2.0) * 100.0).round() / 100.0 } else { 1.0 };
        // A click-through widget cannot be dragged: keep it locked so it never looks movable.
        if self.click_through {
            self.locked = true;
        }
    }

    pub fn apply(&mut self, patch: &serde_json::Value) -> Result<()> {
        let obj = patch.as_object().ok_or_else(|| anyhow::anyhow!("objet attendu"))?;
        for (k, v) in obj {
            let b = || v.as_bool().ok_or_else(|| anyhow::anyhow!("booléen attendu pour {k}"));
            let f = || v.as_f64().map(|x| x as f32).ok_or_else(|| anyhow::anyhow!("nombre attendu pour {k}"));
            match k.as_str() {
                "layer" => self.layer = v.as_str().ok_or_else(|| anyhow::anyhow!("texte attendu"))?.to_string(),
                "opacity" => self.opacity = f()?,
                "scale" => self.scale = f()?,
                "locked" => self.locked = b()?,
                "clickThrough" => {
                    self.click_through = b()?;
                    if !self.click_through {
                        self.locked = false;
                    }
                }
                "visible" => self.visible = b()?,
                _ => anyhow::bail!("réglage de widget inconnu : {k}"),
            }
        }
        self.clamp();
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    pub version: u32,
    pub master_db: f32,
    pub source: Source,
    pub local_pc: bool,
    pub speakers: Vec<Speaker>,
    pub rooms: Vec<Room>,
    pub scenes: Vec<Scene>,
    pub midi_map: Vec<MidiBinding>,
    pub settings: Settings,
    pub pc_muted_by_us: bool,
    pub pc_prev_mute: bool,
    pub pc_muted_device: Option<String>,
    pub pc_pending_restore: Vec<PendingRestore>,
    pub access_key: String,
    // Silences every speaker without touching the master level.
    pub master_mute: bool,
    // Main window kept above the other windows and visible when it loses focus.
    pub window_pinned: bool,
    pub ui_zoom: f32,
    pub widgets: Vec<Widget>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PendingRestore {
    pub device_id: String,
    pub mute: bool,
}

pub const CONFIG_VERSION: u32 = 2;

impl Default for Config {
    fn default() -> Self {
        let room = |id: &str, name: &str, icon: &str, color: &str| Room {
            id: id.into(),
            name: name.into(),
            icon: icon.into(),
            color: color.into(),
            ..Room::default()
        };
        Self {
            version: CONFIG_VERSION,
            master_db: 0.0,
            source: Source::System,
            local_pc: false,
            speakers: vec![],
            rooms: vec![
                room("salon", "Salon", "Sofa", "#d9480f"),
                room("cuisine", "Cuisine", "CookingPot", "#5c940d"),
                room("bureau", "Bureau", "Monitor", "#1c7ed6"),
                room("chambre", "Chambre", "BedDouble", "#9c36b5"),
                room("terrasse", "Terrasse", "TreePalm", "#c2255c"),
            ],
            scenes: vec![],
            midi_map: vec![],
            settings: Settings::default(),
            pc_muted_by_us: false,
            pc_prev_mute: false,
            pc_muted_device: None,
            pc_pending_restore: vec![],
            access_key: String::new(),
            master_mute: false,
            window_pinned: false,
            ui_zoom: 1.0,
            widgets: vec![],
        }
    }
}

impl Config {
    pub fn speaker(&self, id: &str) -> Option<&Speaker> {
        self.speakers.iter().find(|s| s.id == id)
    }
    pub fn speaker_mut(&mut self, id: &str) -> Option<&mut Speaker> {
        self.speakers.iter_mut().find(|s| s.id == id)
    }
    pub fn room(&self, id: &str) -> Option<&Room> {
        self.rooms.iter().find(|r| r.id == id)
    }

    pub fn room_by_name_or_create(&mut self, name: &str) -> String {
        if let Some(r) = self.rooms.iter().find(|r| r.name.eq_ignore_ascii_case(name)) {
            return r.id.clone();
        }
        const PALETTE: [&str; 8] = [
            "#d9480f", "#5c940d", "#1c7ed6", "#9c36b5", "#c2255c", "#b08900", "#0b7285", "#6b5b4b",
        ];
        let lower = name.to_lowercase();
        let icon = if lower.contains("salon") || lower.contains("living") {
            "Sofa"
        } else if lower.contains("cuisine") || lower.contains("kitchen") {
            "CookingPot"
        } else if lower.contains("bureau") || lower.contains("office") {
            "Monitor"
        } else if lower.contains("chambre") || lower.contains("bed") {
            "BedDouble"
        } else if lower.contains("terrasse") || lower.contains("jardin") || lower.contains("patio") {
            "TreePalm"
        } else {
            "Speaker"
        };
        let id = self.unique_room_id(&slug(name));
        let color = PALETTE[self.rooms.len() % PALETTE.len()].to_string();
        self.rooms.push(Room { id: id.clone(), name: name.into(), icon: icon.into(), color, ..Room::default() });
        id
    }

    pub fn unique_room_id(&self, base: &str) -> String {
        let base = if base.is_empty() { "piece" } else { base };
        let mut id = base.to_string();
        let mut n = 2;
        while self.rooms.iter().any(|r| r.id == id) {
            id = format!("{base}-{n}");
            n += 1;
        }
        id
    }
}

pub fn slug(s: &str) -> String {
    let mut out = String::new();
    for ch in s.to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

pub struct Store {
    path: PathBuf,
    cfg: RwLock<Config>,
    dirty: AtomicBool,
}

impl Store {
    pub fn load(dir: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("config.json");
        let cfg = std::fs::read_to_string(&path)
            .ok()
            .and_then(|txt| match serde_json::from_str::<Config>(&txt) {
                Ok(c) => Some(c),
                Err(e) => {
                    log::error!("config.json illisible ({e}) : sauvegarde en .bak et réinitialisation");
                    let _ = std::fs::copy(&path, dir.join("config.json.bak"));
                    None
                }
            })
            .unwrap_or_default();
        let mut cfg = cfg;
        let migrated = cfg.version < CONFIG_VERSION;
        if cfg.version < 2 {
            cfg.settings.pin_engine_period = false;
        }
        cfg.version = CONFIG_VERSION;
        let mut dirty = migrated;
        cfg.ui_zoom = clamp_zoom(cfg.ui_zoom);
        for w in cfg.widgets.iter_mut() {
            w.clamp();
        }
        cfg.widgets.retain(|w| !w.id.is_empty());
        for sp in cfg.speakers.iter_mut() {
            sp.sanitize_style();
        }
        if cfg.access_key.len() < 32 {
            cfg.access_key = new_access_key();
            dirty = true;
        }
        Self { path, cfg: RwLock::new(cfg), dirty: AtomicBool::new(dirty) }
    }

    pub fn dir(&self) -> PathBuf {
        self.path.parent().map(|d| d.to_path_buf()).unwrap_or_default()
    }

    pub fn read<R>(&self, f: impl FnOnce(&Config) -> R) -> R {
        f(&self.cfg.read())
    }

    pub fn write<R>(&self, f: impl FnOnce(&mut Config) -> R) -> R {
        let r = f(&mut self.cfg.write());
        self.dirty.store(true, Ordering::Release);
        r
    }

    pub fn flush(&self) {
        if self.dirty.swap(false, Ordering::AcqRel) {
            let json = match serde_json::to_vec_pretty(&*self.cfg.read()) {
                Ok(j) => j,
                Err(e) => return log::error!("sérialisation config : {e}"),
            };
            let tmp = self.path.with_extension("json.tmp");
            if std::fs::write(&tmp, json).and_then(|_| std::fs::rename(&tmp, &self.path)).is_err() {
                log::error!("écriture de {:?} impossible", self.path);
                self.dirty.store(true, Ordering::Release);
            }
        }
    }
}

// Secret placed in every URL served to the LAN (streams, browser receiver, UPnP callbacks): without it the
// HTTP server answers 404, so other devices on the network cannot listen to the PC.
pub fn new_access_key() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let mut out = String::with_capacity(32);
    for i in 0..2u64 {
        let mut h = RandomState::new().build_hasher();
        h.write_u64(i);
        h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
        out.push_str(&format!("{:016x}", h.finish()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eq_settings_are_validated_and_survive_a_reload() {
        let mut p = ChannelParams::default();
        p.set("eqGains", &serde_json::json!([13.0, -20.0, 1.26, 0.0, -0.04, 2.5])).unwrap();
        assert_eq!(p.eq_gains, [12.0, -12.0, 1.3, 0.0, -0.0, 2.5]);
        p.set("eqPreset", &serde_json::json!("custom")).unwrap();
        p.set("eqEnabled", &serde_json::json!(false)).unwrap();
        assert!(p.set("eqGains", &serde_json::json!([1.0, 2.0])).is_err());
        let back: ChannelParams = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        assert_eq!(back, p);
        let old: ChannelParams = serde_json::from_str(r#"{"gainDb": 1.0}"#).unwrap();
        assert!(old.eq_enabled && old.eq_preset == "flat" && old.eq_gains == [0.0; 6], "anciennes configs");
    }

    #[test]
    fn widgets_are_validated_and_unknown_values_do_not_break_the_config() {
        let cfg: Config = serde_json::from_str(
            r#"{"widgets":[{"id":"w1","kind":"hologram","layer":"sky","opacity":4,"scale":0.1},{"id":"w2","kind":"meter"}]}"#,
        )
        .unwrap();
        let mut w = cfg.widgets[0].clone();
        w.clamp();
        assert_eq!((w.kind.as_str(), w.layer.as_str(), w.opacity, w.scale), ("player", "normal", 1.0, 0.6));
        assert_eq!(cfg.widgets[1].layer, "normal");
        assert!(cfg.widgets[1].visible && !cfg.widgets[1].locked);
        assert_eq!(cfg.ui_zoom, 1.0, "older configs get the default zoom");

        let mut w = Widget { id: "w3".into(), ..Widget::default() };
        w.apply(&serde_json::json!({"layer": "desktop", "opacity": 0.333, "clickThrough": true})).unwrap();
        assert_eq!((w.layer.as_str(), w.opacity, w.click_through, w.locked), ("desktop", 0.33, true, true));
        w.apply(&serde_json::json!({"clickThrough": false})).unwrap();
        assert!(!w.locked, "turning click-through off makes the widget movable again");
        assert!(w.apply(&serde_json::json!({"x": 10})).is_err(), "position is set by dragging only");
    }

    #[test]
    fn speaker_style_and_plan_position_are_sanitised() {
        let mut sp = Speaker {
            color: Some("red; background:url(x)".into()),
            icon: Some("Tv".into()),
            plan_x: Some(1.7),
            plan_y: Some(f32::NAN),
            ..Speaker::default()
        };
        sp.sanitize_style();
        assert_eq!((sp.color, sp.icon.as_deref(), sp.plan_x, sp.plan_y), (None, Some("Tv"), None, None));
        let mut sp = Speaker { color: Some("#1C7ed6".into()), plan_x: Some(-0.2), plan_y: Some(0.5), ..Speaker::default() };
        sp.sanitize_style();
        assert_eq!((sp.color.as_deref(), sp.plan_x, sp.plan_y), (Some("#1C7ed6"), Some(0.0), Some(0.5)));
        assert_eq!((clamp_zoom(3.0), clamp_zoom(1.13), clamp_zoom(f32::NAN)), (1.6, 1.15, 1.0));
    }
}