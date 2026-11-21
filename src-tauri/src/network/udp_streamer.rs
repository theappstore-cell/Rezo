use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle, Thread};
use std::time::{Duration, Instant};

use anyhow::Result;
use rtrb::Consumer;

use crate::audio::encoder::OpusEnc;

pub const RTP_PT: u8 = 111;
const SILENCE_THRESHOLD: f32 = 1e-4;
const IDLE_AFTER_MS: f32 = 300.0;
const HEARTBEAT_MS: f32 = 20.0;
const VOLUME_REPEAT: Duration = Duration::from_secs(2);

#[derive(Clone)]
pub struct UdpConfig {
    pub addr: SocketAddr,
    pub frame_ms: f32,
    pub bitrate_kbps: u32,
    pub redundancy: bool,
    pub ssrc: u32,
    // Receiver output volume (0-100), sent out of band: receivers older than this packet drop it as non-RTP.
    pub volume: Arc<AtomicU32>,
}

#[derive(Default)]
pub struct UdpStats {
    pub packets: AtomicU64,
    pub heartbeats: AtomicU64,
    pub send_errors: AtomicU64,
}

pub struct UdpHandle {
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
    pub thread: Thread,
}

impl UdpHandle {
    pub fn stop(mut self) {
        self.shutdown();
    }
    fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.thread.unpark();
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

impl Drop for UdpHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

pub fn spawn(cons: Consumer<f32>, cfg: UdpConfig) -> Result<UdpHandle> {
    let enc = OpusEnc::new(cfg.frame_ms, cfg.bitrate_kbps)?;
    let sock = UdpSocket::bind("0.0.0.0:0")?;
    sock.connect(cfg.addr)?;
    let _ = socket2::SockRef::from(&sock).set_tos_v4(0xB8);

    let stop = Arc::new(AtomicBool::new(false));
    let stats = Arc::new(UdpStats::default());
    let (s2, st2) = (stop.clone(), stats.clone());
    let join = thread::Builder::new()
        .name(format!("rezo-udp-{}", cfg.addr))
        .spawn(move || run(cons, enc, sock, cfg, s2, st2))?;
    let thread = join.thread().clone();
    Ok(UdpHandle { stop, join: Some(join), thread })
}

fn put_header(buf: &mut [u8], marker: bool, seq: u16, ts: u32, ssrc: u32) {
    buf[0] = 0x80;
    buf[1] = ((marker as u8) << 7) | RTP_PT;
    buf[2..4].copy_from_slice(&seq.to_be_bytes());
    buf[4..8].copy_from_slice(&ts.to_be_bytes());
    buf[8..12].copy_from_slice(&ssrc.to_be_bytes());
}

fn run(mut cons: Consumer<f32>, mut enc: OpusEnc, sock: UdpSocket, cfg: UdpConfig, stop: Arc<AtomicBool>, stats: Arc<UdpStats>) {
    let _mmcss = crate::audio::promote_current_thread();

    let frame_frames = enc.frame_frames;
    let frame_samples = frame_frames * 2;
    let frame_ms = frame_frames as f32 * 1000.0 / 48_000.0;

    let mut pcm = vec![0.0f32; frame_samples];
    let mut cur = [0u8; 1500];
    let mut prev = [0u8; 1500];
    let mut prev_len = 0usize;
    let mut pkt = [0u8; 12 + 3 + 1500 + 1500];

    let (mut seq, mut ts) = (0u16, 0u32);
    let mut silent_ms = 0.0f32;
    let mut hb_acc = 0.0f32;
    let mut idle = false;
    let mut resume = true;

    let mut redundancy_on = cfg.redundancy;
    let mut clean_since: Option<Instant> = None;
    let mut last_feedback: Option<Instant> = None;
    let mut last_poll = Instant::now();
    let mut fbuf = [0u8; 64];
    let _ = sock.set_nonblocking(true);
    let mut sent_volume: Option<(u32, Instant)> = None;

    while !stop.load(Ordering::Acquire) {
        let volume = cfg.volume.load(Ordering::Relaxed).min(100);
        if sent_volume.map_or(true, |(v, at)| v != volume || at.elapsed() >= VOLUME_REPEAT) {
            let _ = sock.send(&volume_packet(volume));
            sent_volume = Some((volume, Instant::now()));
        }
        if cfg.redundancy && last_poll.elapsed() >= Duration::from_millis(250) {
            last_poll = Instant::now();
            while let Ok(n) = sock.recv(&mut fbuf) {
                if let Some((received, lost)) = parse_feedback(&fbuf[..n]) {
                    last_feedback = Some(Instant::now());
                    if lost > 0 {
                        clean_since = None;
                        if !redundancy_on {
                            log::debug!("{} : pertes signalées, redondance réactivée", cfg.addr);
                        }
                        redundancy_on = true;
                    } else if received > 0 {
                        let since = *clean_since.get_or_insert_with(Instant::now);
                        if redundancy_on && since.elapsed() >= Duration::from_secs(60) {
                            log::debug!("{} : 60 s sans perte, redondance coupée", cfg.addr);
                            redundancy_on = false;
                        }
                    }
                }
            }
            if !redundancy_on && last_feedback.map_or(true, |t| t.elapsed() > Duration::from_secs(10)) {
                redundancy_on = true;
                clean_since = None;
            }
        }
        while cons.slots() >= frame_samples {
            if let Ok(chunk) = cons.read_chunk(frame_samples) {
                let (a, b) = chunk.as_slices();
                pcm[..a.len()].copy_from_slice(a);
                pcm[a.len()..].copy_from_slice(b);
                chunk.commit_all();
            } else {
                break;
            }

            let silent = pcm.iter().all(|s| s.abs() < SILENCE_THRESHOLD);
            if silent {
                silent_ms += frame_ms;
                if !idle && silent_ms >= IDLE_AFTER_MS {
                    idle = true;
                    prev_len = 0;
                }
            } else {
                if idle {
                    resume = true;
                }
                silent_ms = 0.0;
                idle = false;
            }

            if idle {
                hb_acc += frame_ms;
                if hb_acc >= HEARTBEAT_MS {
                    hb_acc = 0.0;
                    put_header(&mut pkt, false, seq, ts.wrapping_add(frame_frames as u32), cfg.ssrc);
                    seq = seq.wrapping_add(1);
                    if sock.send(&pkt[..12]).is_err() {
                        stats.send_errors.fetch_add(1, Ordering::Relaxed);
                    }
                    stats.heartbeats.fetch_add(1, Ordering::Relaxed);
                }
            } else {
                match enc.encode(&pcm, &mut cur) {
                    Ok(len) => {
                        put_header(&mut pkt, resume, seq, ts, cfg.ssrc);
                        let red = redundancy_on && prev_len > 0 && !resume;
                        pkt[12] = red as u8;
                        let mut n = 15;
                        if red {
                            pkt[13..15].copy_from_slice(&(prev_len as u16).to_be_bytes());
                            pkt[n..n + prev_len].copy_from_slice(&prev[..prev_len]);
                            n += prev_len;
                        } else {
                            pkt[13..15].copy_from_slice(&0u16.to_be_bytes());
                        }
                        pkt[n..n + len].copy_from_slice(&cur[..len]);
                        n += len;
                        if sock.send(&pkt[..n]).is_err() {
                            stats.send_errors.fetch_add(1, Ordering::Relaxed);
                        }
                        stats.packets.fetch_add(1, Ordering::Relaxed);
                        prev[..len].copy_from_slice(&cur[..len]);
                        prev_len = len;
                        seq = seq.wrapping_add(1);
                        resume = false;
                    }
                    Err(e) => log::warn!("encodage Opus : {e}"),
                }
            }
            ts = ts.wrapping_add(frame_frames as u32);
        }
        thread::park_timeout(Duration::from_millis(50));
    }
}

fn parse_feedback(p: &[u8]) -> Option<(u16, u16)> {
    (p.len() >= 8 && &p[..4] == b"RZFB").then(|| (u16::from_be_bytes([p[4], p[5]]), u16::from_be_bytes([p[6], p[7]])))
}

fn volume_packet(volume: u32) -> [u8; 6] {
    let mut p = *b"RZVL\x00\x00";
    p[4..6].copy_from_slice(&(volume.min(100) as u16).to_be_bytes());
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_packet_is_not_rtp() {
        let p = volume_packet(73);
        assert_eq!(&p[..4], b"RZVL");
        assert_eq!(u16::from_be_bytes([p[4], p[5]]), 73);
        assert_ne!(p[0] >> 6, 2, "les récepteurs plus anciens doivent l'ignorer");
        assert_eq!(u16::from_be_bytes([volume_packet(250)[4], volume_packet(250)[5]]), 100);
    }

    #[test]
    fn feedback_parsing() {
        assert_eq!(parse_feedback(b"RZFB\x00d\x00\x02"), Some((100, 2)));
        assert_eq!(parse_feedback(b"RZFB\x00"), None);
        assert_eq!(parse_feedback(b"XXXX\x00\x00\x00\x00"), None);
    }

    #[test]
    fn header_layout() {
        let mut b = [0u8; 12];
        put_header(&mut b, true, 0x0102, 0x03040506, 0x0708090A);
        assert_eq!(b[0], 0x80);
        assert_eq!(b[1], 0x80 | RTP_PT);
        assert_eq!(&b[2..4], &[1, 2]);
        assert_eq!(&b[4..8], &[3, 4, 5, 6]);
        assert_eq!(&b[8..12], &[7, 8, 9, 10]);
    }
}
