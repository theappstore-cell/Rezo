use std::net::UdpSocket;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, StreamConfig};
use mdns_sd::{ServiceDaemon, ServiceInfo};
use opus::{Channels, Decoder};

const RATE: u32 = 48_000;

fn arg(name: &str) -> Option<String> {
    let a: Vec<String> = std::env::args().collect();
    a.iter().position(|x| x == name).and_then(|i| a.get(i + 1).cloned())
}

fn main() -> Result<()> {
    let port: u16 = arg("--port").and_then(|p| p.parse().ok()).unwrap_or(5004);
    let name = arg("--name").unwrap_or_else(|| format!("Récepteur Rezo {port}"));
    let buffer_ms: usize = arg("--buffer-ms").and_then(|p| p.parse().ok()).unwrap_or(20);
    let target = RATE as usize * buffer_ms / 1000 * 2;
    let max_fill = target + RATE as usize / 20 * 2;

    let host = cpal::default_host();
    let dev = host.default_output_device().ok_or_else(|| anyhow!("aucune sortie audio"))?;
    let cfg = StreamConfig { channels: 2, sample_rate: RATE, buffer_size: BufferSize::Default };
    let (mut prod, mut cons) = rtrb::RingBuffer::<f32>::new(RATE as usize);
    let started = Arc::new(AtomicBool::new(false));
    let st = started.clone();
    let stream = dev.build_output_stream(
        cfg,
        move |out: &mut [f32], _| {
            let mut avail = cons.slots();
            if avail > max_fill {
                let skip = (avail - target) & !1;
                if let Ok(c) = cons.read_chunk(skip) {
                    c.commit_all();
                }
                avail = cons.slots();
            }
            if !st.load(Ordering::Relaxed) {
                if avail >= target {
                    st.store(true, Ordering::Relaxed);
                } else {
                    out.fill(0.0);
                    return;
                }
            }
            if avail < out.len() {
                let n = avail & !1;
                if let Ok(c) = cons.read_chunk(n) {
                    let (a, b) = c.as_slices();
                    out[..a.len()].copy_from_slice(a);
                    out[a.len()..n].copy_from_slice(b);
                    c.commit_all();
                }
                out[n..].fill(0.0);
                st.store(false, Ordering::Relaxed);
            } else if let Ok(c) = cons.read_chunk(out.len()) {
                let (a, b) = c.as_slices();
                out[..a.len()].copy_from_slice(a);
                out[a.len()..].copy_from_slice(b);
                c.commit_all();
            }
        },
        |e| eprintln!("erreur audio : {e}"),
        None,
    )?;
    stream.play()?;

    let sock = UdpSocket::bind(("0.0.0.0", port))?;
    let _ = socket2::SockRef::from(&sock).set_recv_buffer_size(1 << 20);
    let _mdns = announce(&name, port)?;
    println!("« {name} » écoute sur UDP :{port} (tampon {buffer_ms} ms) — Ctrl+C pour quitter");

    let mut dec = Decoder::new(RATE, Channels::Stereo)?;
    let mut pcm = vec![0f32; 5760 * 2];
    let mut buf = [0u8; 4096];
    let mut expected_seq: Option<u16> = None;
    let mut last_ts: Option<u32> = None;
    let (mut fb_recv, mut fb_lost, mut fb_last) = (0u16, 0u16, Instant::now());

    let mut push = |samples: &[f32]| {
        if prod.slots() >= samples.len() {
            if let Ok(c) = prod.write_chunk_uninit(samples.len()) {
                c.fill_from_iter(samples.iter().copied());
            }
        }
    };
    let silence = |n_frames: usize, push: &mut dyn FnMut(&[f32])| {
        let z = [0f32; 960];
        let mut left = n_frames.min(RATE as usize) * 2;
        while left > 0 {
            let n = left.min(z.len());
            push(&z[..n]);
            left -= n;
        }
    };

    loop {
        let (n, peer) = sock.recv_from(&mut buf)?;
        if n < 12 || buf[0] >> 6 != 2 {
            continue;
        }
        let marker = buf[1] & 0x80 != 0;
        let seq = u16::from_be_bytes([buf[2], buf[3]]);
        let ts = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);
        let payload = &buf[12..n];

        if let Some(prev) = last_ts {
            let gap = ts.wrapping_sub(prev) as usize;
            if (marker || payload.is_empty()) && gap > 0 && gap < RATE as usize * 5 {
                silence(gap, &mut push);
            }
        }

        if payload.is_empty() {
            last_ts = Some(ts);
            continue;
        }
        if marker {
            expected_seq = None;
            if let Ok(d) = Decoder::new(RATE, Channels::Stereo) {
                dec = d;
            }
        }

        let red = payload[0] & 1 == 1;
        let prev_len = u16::from_be_bytes([payload[1], payload[2]]) as usize;
        let (prev, cur) = if red && payload.len() >= 3 + prev_len {
            (Some(&payload[3..3 + prev_len]), &payload[3 + prev_len..])
        } else {
            (None, &payload[3..])
        };

        if let Some(exp) = expected_seq {
            let lost = seq.wrapping_sub(exp) as usize;
            if lost > 0 && lost < 4000 {
                fb_lost = fb_lost.saturating_add(lost as u16);
            }
            if lost == 1 {
                if let Some(p) = prev {
                    if let Ok(k) = dec.decode_float(p, &mut pcm, false) {
                        push(&pcm[..k * 2]);
                    }
                } else if let Ok(k) = dec.decode_float(&[], &mut pcm[..480], false) {
                    push(&pcm[..k * 2]);
                }
            } else if lost > 1 && lost < 8 {
                for _ in 0..lost {
                    if let Ok(k) = dec.decode_float(&[], &mut pcm[..480], false) {
                        push(&pcm[..k * 2]);
                    }
                }
            }
        }
        match dec.decode_float(cur, &mut pcm, false) {
            Ok(k) => {
                push(&pcm[..k * 2]);
                last_ts = Some(ts.wrapping_add(k as u32));
            }
            Err(e) => eprintln!("décodage : {e}"),
        }
        expected_seq = Some(seq.wrapping_add(1));
        fb_recv = fb_recv.saturating_add(1);
        if fb_last.elapsed() >= Duration::from_secs(1) {
            let mut fb = *b"RZFB\x00\x00\x00\x00";
            fb[4..6].copy_from_slice(&fb_recv.to_be_bytes());
            fb[6..8].copy_from_slice(&fb_lost.to_be_bytes());
            let _ = sock.send_to(&fb, peer);
            (fb_recv, fb_lost, fb_last) = (0, 0, Instant::now());
        }
    }
}

fn announce(name: &str, port: u16) -> Result<ServiceDaemon> {
    let daemon = ServiceDaemon::new()?;
    let id: String = name.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_lowercase();
    let host = format!("{}.local.", if id.is_empty() { "rezo-recv".into() } else { id.clone() });
    let props = [("id", id.as_str()), ("name", name)];
    let info = ServiceInfo::new("_rezo._udp.local.", &id, &host, "", port, &props[..])?.enable_addr_auto();
    daemon.register(info)?;
    Ok(daemon)
}
