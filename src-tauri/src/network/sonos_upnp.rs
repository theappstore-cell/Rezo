use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use bytes::{Bytes, BytesMut};
use futures_util::future::join_all;
use socket2::{Domain, Protocol, Socket, Type};
use tokio::net::UdpSocket;

use super::xml;
use super::{wait_for_samples, PcmTap};
use crate::audio::encoder::{f32_to_s16le, wav_streaming_header};
use crate::engine::Engine;

const ST_ZONEPLAYER: &str = "urn:schemas-upnp-org:device:ZonePlayer:1";
const ST_RENDERER: &str = "urn:schemas-upnp-org:device:MediaRenderer:1";
const AVT: &str = "urn:schemas-upnp-org:service:AVTransport:1";
const ZGT: &str = "urn:schemas-upnp-org:service:ZoneGroupTopology:1";
const RC: &str = "urn:schemas-upnp-org:service:RenderingControl:1";

// Silence sent ahead of live audio, in 5 ms chunks. Measured on a Sonos with a microphone: 200 ms of pre-roll
// gives 420 ms from capture to sound, 50 ms gives 300 ms; below that the player waits for its own minimum.
pub const PREROLL_LOW_LATENCY: u32 = 10;
pub const PREROLL_SAFE: u32 = 40;
const WAV_CHUNK_SAMPLES: usize = 1920;

#[derive(Clone, Debug)]
pub struct DeviceInfo {
    pub udn: String,
    pub ip: String,
    pub port: u16,
    pub friendly_name: String,
    pub room_name: Option<String>,
    pub model: Option<String>,
    pub is_sonos: bool,
    pub av_control_url: String,
    pub event_url: Option<String>,
    pub rendering_url: Option<String>,
    pub line_in: bool,
}

fn ssdp_socket(iface: Ipv4Addr) -> Result<UdpSocket> {
    let s = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    s.set_reuse_address(true)?;
    s.bind(&SocketAddr::new(IpAddr::V4(iface), 0).into())?;
    s.set_multicast_if_v4(&iface)?;
    s.set_multicast_ttl_v4(2)?;
    s.set_nonblocking(true)?;
    Ok(UdpSocket::from_std(s.into())?)
}

fn msearch(st: &str) -> String {
    format!("M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nMAN: \"ssdp:discover\"\r\nMX: 1\r\nST: {st}\r\nUSER-AGENT: Rezo/0.1 UPnP/1.1\r\n\r\n")
}

pub async fn ssdp_locations(window: Duration, ifaces: Vec<Ipv4Addr>) -> Vec<(String, Ipv4Addr)> {
    let tasks = ifaces.into_iter().map(|iface| async move {
        let mut found: Vec<(String, Ipv4Addr)> = vec![];
        let sock = match ssdp_socket(iface) {
            Ok(s) => s,
            Err(e) => {
                log::debug!("SSDP : interface {iface} ignorée ({e})");
                return found;
            }
        };
        let dest: SocketAddr = "239.255.255.250:1900".parse().unwrap();
        let deadline = tokio::time::Instant::now() + window;
        let mut next_send = tokio::time::Instant::now();
        let mut sends = 0;
        let mut buf = [0u8; 2048];
        loop {
            let now = tokio::time::Instant::now();
            if now >= deadline {
                break;
            }
            if sends < 2 && now >= next_send {
                for st in [ST_ZONEPLAYER, ST_RENDERER] {
                    let _ = sock.send_to(msearch(st).as_bytes(), dest).await;
                }
                sends += 1;
                next_send = now + Duration::from_millis(300);
            }
            let wait = (deadline - now).min(Duration::from_millis(100));
            if let Ok(Ok((n, _))) = tokio::time::timeout(wait, sock.recv_from(&mut buf)).await {
                let text = String::from_utf8_lossy(&buf[..n]);
                for line in text.lines() {
                    if line.len() > 9 && line[..9].eq_ignore_ascii_case("location:") {
                        let loc = line[9..].trim().to_string();
                        if !found.iter().any(|(l, _)| l == &loc) {
                            found.push((loc, iface));
                        }
                    }
                }
            }
        }
        found
    });
    join_all(tasks).await.into_iter().flatten().collect()
}

// Service URLs may be relative to the description (`AVTransport/control`), to its URLBase, or absolute: keep the
// path the device expects on its own host and port.
fn resolve_path(base: &reqwest::Url, u: Option<String>) -> Option<String> {
    let joined = base.join(u?.trim()).ok()?;
    let mut p = joined.path().to_string();
    if let Some(q) = joined.query() {
        p.push('?');
        p.push_str(q);
    }
    Some(p)
}

fn rooted(path: &str) -> String {
    if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    }
}

pub async fn describe(client: &reqwest::Client, location: &str) -> Result<DeviceInfo> {
    let url = reqwest::Url::parse(location)?;
    let ip = url.host_str().ok_or_else(|| anyhow!("LOCATION sans hôte"))?.to_string();
    let port = url.port().unwrap_or(80);
    let body = client.get(location).send().await?.error_for_status()?.text().await?;
    let base = xml::tag_text(&body, "URLBase").and_then(|b| reqwest::Url::parse(&b).ok()).unwrap_or_else(|| url.clone());
    let av = resolve_path(&base, xml::service_control_url(&body, "AVTransport")).ok_or_else(|| anyhow!("pas de service AVTransport"))?;
    let manufacturer = xml::tag_text(&body, "manufacturer").unwrap_or_default();
    let is_sonos = manufacturer.to_lowercase().contains("sonos") || body.contains(ST_ZONEPLAYER);
    let udn = xml::tag_text(&body, "UDN").ok_or_else(|| anyhow!("pas d'UDN"))?;
    Ok(DeviceInfo {
        udn: udn.trim_start_matches("uuid:").to_string(),
        ip,
        port,
        friendly_name: xml::tag_text(&body, "friendlyName").unwrap_or_else(|| "Enceinte".into()),
        room_name: xml::tag_text(&body, "roomName"),
        model: xml::tag_text(&body, "modelName"),
        is_sonos,
        av_control_url: av,
        event_url: resolve_path(&base, xml::service_event_url(&body, "AVTransport")),
        rendering_url: resolve_path(&base, xml::service_control_url(&body, "service:RenderingControl:")),
        line_in: is_sonos && body.contains("service:AudioIn:"),
    })
}

const DESC_TTL: Duration = Duration::from_secs(15 * 60);

pub async fn discover(
    client: &reqwest::Client,
    window: Duration,
    ifaces: Vec<Ipv4Addr>,
    cache: &parking_lot::Mutex<HashMap<String, (DeviceInfo, Instant)>>,
) -> (Vec<DeviceInfo>, Vec<Ipv4Addr>) {
    let raw = ssdp_locations(window, ifaces).await;
    let mut productive: Vec<Ipv4Addr> = raw.iter().map(|(_, i)| *i).collect();
    productive.sort();
    productive.dedup();
    let mut locs: Vec<String> = raw.into_iter().map(|(l, _)| l).collect();
    locs.sort();
    locs.dedup();

    let mut devs: Vec<DeviceInfo> = vec![];
    let mut todo: Vec<String> = vec![];
    {
        let c = cache.lock();
        for l in &locs {
            match c.get(l) {
                Some((d, at)) if at.elapsed() < DESC_TTL => devs.push(d.clone()),
                _ => todo.push(l.clone()),
            }
        }
    }
    let fetched = join_all(todo.iter().map(|l| describe(client, l))).await;
    {
        let mut c = cache.lock();
        for (l, r) in todo.iter().zip(fetched) {
            if let Ok(d) = r {
                c.insert(l.clone(), (d.clone(), Instant::now()));
                devs.push(d);
            }
        }
    }
    devs.sort_by(|a, b| a.udn.cmp(&b.udn));
    devs.dedup_by(|a, b| a.udn == b.udn);
    (devs, productive)
}

pub async fn soap(
    client: &reqwest::Client,
    ip: &str,
    port: u16,
    path: &str,
    service: &str,
    action: &str,
    args: &[(&str, &str)],
) -> Result<String> {
    let mut inner = String::new();
    for (k, v) in args {
        inner.push_str(&format!("<{k}>{}</{k}>", xml::escape(v)));
    }
    let body = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?><s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\"><s:Body><u:{action} xmlns:u=\"{service}\">{inner}</u:{action}></s:Body></s:Envelope>"
    );
    let url = format!("http://{ip}:{port}{}", rooted(path));
    let resp = client
        .post(&url)
        .header("Content-Type", "text/xml; charset=\"utf-8\"")
        .header("SOAPACTION", format!("\"{service}#{action}\""))
        .body(body)
        .send()
        .await?;
    let status = resp.status();
    let text = resp.text().await?;
    if !status.is_success() {
        let code = xml::tag_text(&text, "errorCode").unwrap_or_else(|| status.as_str().to_string());
        bail!("{action} refusé par {ip} (erreur UPnP {code})");
    }
    Ok(text)
}

#[derive(Clone, Debug)]
pub struct ZoneMember {
    pub uuid: String,
    pub invisible: bool,
    pub is_coordinator: bool,
    pub group_size: usize,
    pub group_name: String,
}

pub async fn zone_members(client: &reqwest::Client, ip: &str) -> Result<Vec<ZoneMember>> {
    let body = soap(client, ip, 1400, "/ZoneGroupTopology/Control", ZGT, "GetZoneGroupState", &[]).await?;
    let state = xml::tag_text(&body, "ZoneGroupState").ok_or_else(|| anyhow!("ZoneGroupState absent"))?;

    struct Raw {
        uuid: String,
        name: String,
        invisible: bool,
        coord: String,
        group: usize,
    }
    let mut raws: Vec<Raw> = vec![];
    let (mut coord, mut group) = (String::new(), 0usize);
    for frag in state.split('<') {
        if frag.starts_with("ZoneGroup ") {
            coord = xml::attr(frag, "Coordinator").unwrap_or_default();
            group += 1;
        } else if frag.starts_with("ZoneGroupMember ") {
            raws.push(Raw {
                uuid: xml::attr(frag, "UUID").unwrap_or_default(),
                name: xml::attr(frag, "ZoneName").unwrap_or_default(),
                invisible: xml::attr(frag, "Invisible").as_deref() == Some("1"),
                coord: coord.clone(),
                group,
            });
        }
    }
    let mut out = vec![];
    for r in &raws {
        let visible_in_group = raws.iter().filter(|x| x.group == r.group && !x.invisible).count();
        let group_name = raws.iter().find(|x| x.uuid == r.coord).map(|x| x.name.clone()).unwrap_or_else(|| r.name.clone());
        out.push(ZoneMember {
            uuid: r.uuid.clone(),
            invisible: r.invisible,
            is_coordinator: r.uuid == r.coord,
            group_size: visible_in_group,
            group_name,
        });
    }
    Ok(out)
}

#[derive(Clone, Debug)]
pub struct Target {
    pub id: String,
    pub ip: String,
    pub port: u16,
    pub control: String,
    pub is_sonos: bool,
    pub uuid: Option<String>,
    pub url: String,
    pub title: String,
    pub event: Option<String>,
}

pub fn stream_url(local_ip: &IpAddr, port: u16, key: &str, id: &str) -> String {
    format!("http://{local_ip}:{port}/s/{key}/stream/{id}.wav")
}

fn didl(t: &Target) -> String {
    format!(
        "<DIDL-Lite xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:upnp=\"urn:schemas-upnp-org:metadata-1-0/upnp/\" xmlns:r=\"urn:schemas-rinconnetworks-com:metadata-1-0/\" xmlns=\"urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/\"><item id=\"rezo-{id}\" parentID=\"0\" restricted=\"true\"><dc:title>Rezo - {title}</dc:title><upnp:class>object.item.audioItem.musicTrack</upnp:class><res protocolInfo=\"http-get:*:audio/wav:*\">{url}</res></item></DIDL-Lite>",
        id = xml::escape(&t.id),
        title = xml::escape(&t.title),
        url = xml::escape(&t.url),
    )
}

async fn av(client: &reqwest::Client, t: &Target, action: &str, args: &[(&str, &str)]) -> Result<String> {
    soap(client, &t.ip, t.port, &t.control, AVT, action, args).await
}

async fn prepare(client: &reqwest::Client, t: &Target, standalone_needed: bool) -> Result<()> {
    if standalone_needed {
        av(client, t, "BecomeCoordinatorOfStandaloneGroup", &[("InstanceID", "0")]).await?;
    }
    let _ = av(client, t, "Stop", &[("InstanceID", "0")]).await;
    av(client, t, "SetAVTransportURI", &[("InstanceID", "0"), ("CurrentURI", &t.url), ("CurrentURIMetaData", &didl(t))]).await?;
    Ok(())
}

async fn play(client: &reqwest::Client, t: &Target) -> Result<()> {
    av(client, t, "Play", &[("InstanceID", "0"), ("Speed", "1")]).await?;
    Ok(())
}

pub async fn start_synchronized(client: &reqwest::Client, targets: Vec<Target>) -> Vec<(String, Result<()>)> {
    let mut standalone: Vec<String> = vec![];
    if let Some(t) = targets.iter().find(|t| t.is_sonos) {
        if let Ok(members) = zone_members(client, &t.ip).await {
            for m in members {
                if m.group_size > 1 && !m.is_coordinator {
                    standalone.push(m.uuid);
                }
            }
        }
    }

    let prep = join_all(targets.iter().map(|t| {
        let need = t.uuid.as_ref().map_or(false, |u| standalone.contains(u));
        async move { (t.id.clone(), prepare(client, t, need).await) }
    }))
    .await;

    let ready: Vec<&Target> = targets.iter().filter(|t| prep.iter().any(|(id, r)| id == &t.id && r.is_ok())).collect();
    let played = join_all(ready.iter().map(|t| async move { (t.id.clone(), play(client, t).await) })).await;

    prep.into_iter()
        .map(|(id, r)| match r {
            Err(e) => (id, Err(e)),
            Ok(()) => {
                let res = played.iter().find(|(pid, _)| pid == &id).map(|(_, r)| r.as_ref().map(|_| ()).map_err(|e| anyhow!("{e}")));
                (id, res.unwrap_or(Ok(())))
            }
        })
        .collect()
}

pub async fn stop(client: &reqwest::Client, t: &Target) -> Result<()> {
    av(client, t, "Stop", &[("InstanceID", "0")]).await?;
    Ok(())
}

pub async fn ensure_playing(client: &reqwest::Client, t: &Target) -> Result<bool> {
    let info = av(client, t, "GetTransportInfo", &[("InstanceID", "0")]).await?;
    let state = xml::tag_text(&info, "CurrentTransportState").unwrap_or_default();
    let media = av(client, t, "GetMediaInfo", &[("InstanceID", "0")]).await?;
    let uri = xml::tag_text(&media, "CurrentURI").unwrap_or_default();
    let ours = uri.contains(&format!("/stream/{}.wav", t.id));
    if ours && (state == "PLAYING" || state == "TRANSITIONING") {
        return Ok(false);
    }
    if ours && play(client, t).await.is_ok() {
        return Ok(true);
    }
    prepare(client, t, false).await?;
    play(client, t).await?;
    Ok(true)
}

pub fn line_in_uri(uuid: &str) -> String {
    format!("x-rincon-stream:{uuid}")
}

pub fn group_uri(coordinator_uuid: &str) -> String {
    format!("x-rincon:{coordinator_uuid}")
}

// The player plays its own analog input: no HTTP buffer in the path, the lowest delay a Sonos offers.
pub async fn play_line_in(client: &reqwest::Client, t: &Target) -> Result<()> {
    let uuid = t.uuid.as_deref().ok_or_else(|| anyhow!("UUID Sonos inconnu"))?;
    let _ = av(client, t, "BecomeCoordinatorOfStandaloneGroup", &[("InstanceID", "0")]).await;
    av(client, t, "SetAVTransportURI", &[("InstanceID", "0"), ("CurrentURI", &line_in_uri(uuid)), ("CurrentURIMetaData", "")]).await?;
    play(client, t).await
}

// A member follows its coordinator sample for sample, like a group made in the Sonos app.
pub async fn join_group(client: &reqwest::Client, member: &Target, coordinator_uuid: &str) -> Result<()> {
    av(client, member, "SetAVTransportURI", &[("InstanceID", "0"), ("CurrentURI", &group_uri(coordinator_uuid)), ("CurrentURIMetaData", "")]).await?;
    Ok(())
}

pub async fn leave_group(client: &reqwest::Client, member: &Target) -> Result<()> {
    av(client, member, "BecomeCoordinatorOfStandaloneGroup", &[("InstanceID", "0")]).await?;
    Ok(())
}

pub async fn get_volume(client: &reqwest::Client, ip: &str, port: u16, path: &str) -> Result<u32> {
    let body = soap(client, ip, port, path, RC, "GetVolume", &[("InstanceID", "0"), ("Channel", "Master")]).await?;
    xml::tag_text(&body, "CurrentVolume").and_then(|v| v.parse().ok()).ok_or_else(|| anyhow!("volume illisible"))
}

pub async fn set_volume(client: &reqwest::Client, ip: &str, port: u16, path: &str, volume: u32) -> Result<()> {
    let v = volume.min(100).to_string();
    soap(client, ip, port, path, RC, "SetVolume", &[("InstanceID", "0"), ("Channel", "Master"), ("DesiredVolume", &v)]).await?;
    Ok(())
}

fn event_request(client: &reqwest::Client, t: &Target, method: &str) -> Result<reqwest::RequestBuilder> {
    let path = t.event.as_deref().ok_or_else(|| anyhow!("pas de service d'événements"))?;
    let m = reqwest::Method::from_bytes(method.as_bytes())?;
    Ok(client.request(m, format!("http://{}:{}{}", t.ip, t.port, rooted(path))))
}

fn timeout_secs(resp: &reqwest::Response) -> u64 {
    resp.headers()
        .get("TIMEOUT")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.trim().strip_prefix("Second-"))
        .and_then(|s| s.parse().ok())
        .unwrap_or(300)
}

pub async fn subscribe(client: &reqwest::Client, t: &Target, callback: &str) -> Result<(String, u64)> {
    let resp = event_request(client, t, "SUBSCRIBE")?
        .header("CALLBACK", format!("<{callback}>"))
        .header("NT", "upnp:event")
        .header("TIMEOUT", "Second-1800")
        .send()
        .await?;
    if !resp.status().is_success() {
        bail!("SUBSCRIBE refusé ({})", resp.status());
    }
    let sid = resp.headers().get("SID").and_then(|v| v.to_str().ok()).map(str::to_string).ok_or_else(|| anyhow!("SID absent"))?;
    Ok((sid, timeout_secs(&resp)))
}

pub async fn renew(client: &reqwest::Client, t: &Target, sid: &str) -> Result<u64> {
    let resp = event_request(client, t, "SUBSCRIBE")?.header("SID", sid).header("TIMEOUT", "Second-1800").send().await?;
    if !resp.status().is_success() {
        bail!("renouvellement refusé ({})", resp.status());
    }
    Ok(timeout_secs(&resp))
}

pub async fn unsubscribe(client: &reqwest::Client, t: &Target, sid: &str) -> Result<()> {
    let _ = event_request(client, t, "UNSUBSCRIBE")?.header("SID", sid).send().await?;
    Ok(())
}

pub fn parse_last_change(body: &str) -> Option<(Option<String>, Option<String>)> {
    let last = xml::tag_text(body, "LastChange")?;
    let val = |tag: &str| xml::open_tags(&last, tag).first().and_then(|t| xml::attr(t, "val"));
    Some((val("TransportState"), val("CurrentTrackURI")))
}

pub async fn event_handler(
    State(engine): State<Arc<Engine>>,
    Path((key, id)): Path<(String, String)>,
    method: axum::http::Method,
    body: String,
) -> StatusCode {
    if !engine.key_ok(&key) {
        return StatusCode::NOT_FOUND;
    }
    if method.as_str() != "NOTIFY" {
        return StatusCode::METHOD_NOT_ALLOWED;
    }
    if let Some((state, uri)) = parse_last_change(&body) {
        engine.on_transport_event(&id, state, uri);
    }
    StatusCode::OK
}

struct WavStream {
    engine: Arc<Engine>,
    id: String,
    tap: Arc<PcmTap>,
    my_gen: u64,
    stage: u8,
    preroll: u32,
    pad: usize,
    scratch: Vec<f32>,
    out: BytesMut,
    started: Instant,
    base: u64,
    sent: u64,
    ended: Option<&'static str>,
}

impl WavStream {
    fn add_sent(&mut self, n: u64) {
        self.sent += n;
        self.tap.position.fetch_max(self.base + self.sent, Ordering::Relaxed);
    }
}

impl Drop for WavStream {
    fn drop(&mut self) {
        if self.tap.gen.load(Ordering::Acquire) == self.my_gen {
            self.tap.connected.store(false, Ordering::Release);
        }
        log::info!(
            "flux {} : fin après {:.1} s ({} Ko) — {}",
            self.id,
            self.started.elapsed().as_secs_f32(),
            self.sent / 1024,
            self.ended.unwrap_or("connexion fermée par l'enceinte")
        );
        self.engine.notify_changed();
    }
}

// A Sonos does not answer Play until its probe of the stream has seen a non-zero sample, for up to 10 s: with the
// PC silent, activation used to time out. ±1 LSB (-90 dBFS) in the pre-roll lets it start at once.
fn preroll_chunk() -> Bytes {
    static CHUNK: std::sync::OnceLock<Bytes> = std::sync::OnceLock::new();
    CHUNK
        .get_or_init(|| {
            let mut b = BytesMut::with_capacity(240 * 4);
            for i in 0..240 * 2 {
                b.extend_from_slice(&(if i & 2 == 0 { -1i16 } else { 1 }).to_le_bytes());
            }
            b.freeze()
        })
        .clone()
}

const WAV_TOTAL: u64 = 0xFFFF_FFFF + 8;
const WAV_HEADER_LEN: u64 = 44;

#[derive(Debug, PartialEq)]
enum StreamRequest {
    Probe,
    Fresh,
    Resume(u64),
    Refuse,
}

// A renderer resuming after a network hiccup asks for bytes it has already been sent; Sonos also reads "tags" at
// offsets far ahead (16 MiB, 3.7 GB), and holds Play until that request ends.
const RESUME_SLACK: u64 = 2 << 20;

fn classify(is_head: bool, range: Option<&str>, served: u64) -> StreamRequest {
    if is_head {
        return StreamRequest::Probe;
    }
    let start = range
        .and_then(|r| r.trim().strip_prefix("bytes="))
        .and_then(|r| r.split(['-', ',']).next())
        .and_then(|s| s.trim().parse::<u64>().ok());
    match start {
        None | Some(0) => StreamRequest::Fresh,
        Some(s) if s <= WAV_HEADER_LEN => StreamRequest::Fresh,
        Some(s) if s <= served + RESUME_SLACK => StreamRequest::Resume(s),
        Some(_) => StreamRequest::Refuse,
    }
}

fn wav_headers(b: axum::http::response::Builder) -> axum::http::response::Builder {
    b.header(header::CONTENT_TYPE, "audio/wav")
        .header(header::CACHE_CONTROL, "no-cache, no-store")
        .header("transferMode.dlna.org", "Streaming")
}

// HEAD requests and tag probes must never replace the active stream, or the renderer drops playback.
pub async fn wav_handler(
    State(engine): State<Arc<Engine>>,
    Path((key, file)): Path<(String, String)>,
    method: axum::http::Method,
    headers: axum::http::HeaderMap,
) -> Response {
    if !engine.key_ok(&key) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let id = file.strip_suffix(".wav").unwrap_or(&file).to_string();
    let range = headers.get(header::RANGE).and_then(|v| v.to_str().ok()).map(str::to_string);
    let agent = headers.get(header::USER_AGENT).and_then(|v| v.to_str().ok()).unwrap_or("?").chars().take(60).collect::<String>();
    let served = engine.tap(&id).map_or(0, |t| t.position.load(Ordering::Relaxed));
    let req = classify(method == axum::http::Method::HEAD, range.as_deref(), served);
    log::info!("flux {id} : {method} {} ({agent}) → {req:?}", range.as_deref().unwrap_or("sans Range"));

    let Some(tap) = engine.tap(&id) else {
        return (StatusCode::NOT_FOUND, "enceinte inactive").into_response();
    };
    let base = match req {
        StreamRequest::Resume(s) => s,
        _ => 0,
    };
    let (status, stage, pad, content_range) = match req {
        StreamRequest::Probe => {
            return wav_headers(Response::builder().status(StatusCode::OK))
                .header(header::ACCEPT_RANGES, "bytes")
                .body(Body::empty())
                .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
        }
        StreamRequest::Refuse => {
            return Response::builder()
                .status(StatusCode::RANGE_NOT_SATISFIABLE)
                .header(header::CONTENT_RANGE, format!("bytes */{WAV_TOTAL}"))
                .body(Body::empty())
                .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
        }
        StreamRequest::Fresh => (StatusCode::OK, 0u8, 0usize, None),
        StreamRequest::Resume(s) => {
            let pad = ((4 - (s - WAV_HEADER_LEN) % 4) % 4) as usize;
            (StatusCode::PARTIAL_CONTENT, 2u8, pad, Some(format!("bytes {s}-{}/{WAV_TOTAL}", WAV_TOTAL - 1)))
        }
    };

    let my_gen = tap.gen.fetch_add(1, Ordering::AcqRel) + 1;
    if stage == 2 {
        tap.drain();
        tap.connected.store(true, Ordering::Release);
        engine.notify_changed();
    }
    let preroll = engine.preroll_chunks();
    let st = WavStream {
        engine,
        id,
        tap,
        my_gen,
        stage,
        preroll,
        pad,
        scratch: Vec::with_capacity(4800),
        out: BytesMut::with_capacity(16 * 1024),
        started: Instant::now(),
        base,
        sent: 0,
        ended: None,
    };

    let stream = futures_util::stream::unfold(st, |mut st| async move {
        loop {
            match st.stage {
                0 => {
                    st.stage = 1;
                    st.tap.connected.store(true, Ordering::Release);
                    st.engine.notify_changed();
                    let h = wav_streaming_header(48_000, 2);
                    st.add_sent(h.len() as u64);
                    return Some((Ok::<Bytes, std::convert::Infallible>(Bytes::copy_from_slice(&h)), st));
                }
                1 => {
                    if st.preroll > 0 {
                        st.preroll -= 1;
                        st.add_sent(240 * 4);
                        return Some((Ok(preroll_chunk()), st));
                    }
                    st.tap.drain();
                    st.stage = 2;
                }
                _ => {
                    if st.pad > 0 {
                        let n = std::mem::take(&mut st.pad);
                        st.add_sent(n as u64);
                        return Some((Ok(Bytes::from(vec![0u8; n])), st));
                    }
                    if !st.engine.tap_is_current(&st.id, &st.tap) {
                        st.ended = Some("enceinte désactivée");
                        drop(st);
                        return None;
                    }
                    if st.tap.gen.load(Ordering::Acquire) != st.my_gen {
                        st.ended = Some("remplacé par une connexion plus récente de l'enceinte");
                        drop(st);
                        return None;
                    }
                    if st.tap.available() < WAV_CHUNK_SAMPLES {
                        wait_for_samples(&st.tap, WAV_CHUNK_SAMPLES, Duration::from_millis(40)).await;
                        continue;
                    }
                    st.scratch.clear();
                    let mut scratch = std::mem::take(&mut st.scratch);
                    st.tap.read_into(&mut scratch, 4800);
                    st.out.reserve(scratch.len() * 2);
                    f32_to_s16le(&scratch, &mut st.out);
                    st.scratch = scratch;
                    let chunk = st.out.split().freeze();
                    st.add_sent(chunk.len() as u64);
                    return Some((Ok(chunk), st));
                }
            }
        }
    });

    let mut b = wav_headers(Response::builder().status(status)).header(header::ACCEPT_RANGES, "bytes");
    if let Some(cr) = content_range {
        b = b.header(header::CONTENT_RANGE, cr);
    }
    b.body(Body::from_stream(stream)).unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn side_requests_never_replace_the_stream() {
        let served = 3_000_000;
        assert_eq!(classify(true, None, served), StreamRequest::Probe);
        assert_eq!(classify(true, Some("bytes=0-"), served), StreamRequest::Probe);
        assert_eq!(classify(false, None, served), StreamRequest::Fresh);
        assert_eq!(classify(false, Some("bytes=0-"), served), StreamRequest::Fresh);
        assert_eq!(classify(false, Some("bytes=2500000-"), served), StreamRequest::Resume(2_500_000));
        assert_eq!(classify(false, Some("bytes=4294967175-4294967302"), served), StreamRequest::Refuse);
        assert_eq!(classify(false, Some("n'importe quoi"), served), StreamRequest::Fresh);
        // Seen at start-up, after 16 KiB had been sent: a tag probe, never a resume.
        assert_eq!(classify(false, Some("bytes=16777778-"), 16_384), StreamRequest::Refuse);
    }

    #[test]
    fn service_urls_resolve_against_the_description() {
        let loc = reqwest::Url::parse("http://192.168.1.20:42300/desc/device.xml").unwrap();
        let root = reqwest::Url::parse("http://192.168.1.20:42300/description.xml").unwrap();
        assert_eq!(resolve_path(&root, Some("AVTransport/control".into())).as_deref(), Some("/AVTransport/control"));
        assert_eq!(resolve_path(&loc, Some("AVTransport/control".into())).as_deref(), Some("/desc/AVTransport/control"));
        assert_eq!(resolve_path(&loc, Some("/MediaRenderer/AVTransport/Control".into())).as_deref(), Some("/MediaRenderer/AVTransport/Control"));
        assert_eq!(resolve_path(&loc, Some("http://192.168.1.20:42300/rc?x=1".into())).as_deref(), Some("/rc?x=1"));
        assert_eq!(resolve_path(&loc, None), None);
        assert_eq!(rooted("AVTransport/control"), "/AVTransport/control");
        assert_eq!(rooted("/a"), "/a");
    }

    #[test]
    fn preroll_is_inaudible_but_not_digital_silence() {
        let c = preroll_chunk();
        assert_eq!(c.len(), 240 * 4);
        let samples: Vec<i16> = c.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect();
        assert!(samples.iter().all(|s| s.abs() == 1));
        assert_eq!(samples.iter().map(|s| *s as i32).sum::<i32>(), 0, "pas de composante continue");
    }

    #[test]
    fn last_change_is_parsed() {
        let body = "<e:propertyset xmlns:e=\"urn:schemas-upnp-org:event-1-0\"><e:property><LastChange>&lt;Event xmlns=&quot;urn:schemas-upnp-org:metadata-1-0/AVT/&quot;&gt;&lt;InstanceID val=&quot;0&quot;&gt;&lt;TransportState val=&quot;STOPPED&quot;/&gt;&lt;CurrentTrackURI val=&quot;http://192.168.1.20:49494/stream/abc.wav&quot;/&gt;&lt;/InstanceID&gt;&lt;/Event&gt;</LastChange></e:property></e:propertyset>";
        let (state, uri) = parse_last_change(body).unwrap();
        assert_eq!(state.as_deref(), Some("STOPPED"));
        assert_eq!(uri.as_deref(), Some("http://192.168.1.20:49494/stream/abc.wav"));
        assert!(parse_last_change("<x/>").is_none());
    }
}
