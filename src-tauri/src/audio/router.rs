use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle, Thread};
use std::time::{Duration, Instant};

use crossbeam_channel::{bounded, unbounded, Receiver, Sender};
use rtrb::{Consumer, Producer};

use super::dsp::{block_peaks, db_to_lin, ChannelDsp, DelayLine, DspParams, Limiter};
use super::{AtomicF32, BLOCK_FRAMES, BLOCK_SAMPLES, CHANNELS, SAMPLE_RATE};
use crate::state::rooms::ChannelParams;

pub const DEFAULT_DELAY_FRAMES: usize = SAMPLE_RATE as usize / 10;
// Some endpoints (idle or muted digital outputs) deliver 30-100 ms per packet: only a backlog that large is
// a real lag worth dropping, otherwise every burst would lose its tail.
const MAX_BACKLOG_SAMPLES: usize = SAMPLE_RATE as usize / 5 * CHANNELS;
const TARGET_BACKLOG_SAMPLES: usize = BLOCK_SAMPLES * 2;
const FILTER_TAIL_FRAMES: usize = SAMPLE_RATE as usize / 10;
const MAX_BURST_BLOCKS: u32 = 8;
const ZEROS: [f32; BLOCK_SAMPLES] = [0.0; BLOCK_SAMPLES];

pub struct ChannelShared {
    pub gain_db: AtomicF32,
    pub volume_db: AtomicF32,
    pub low_db: AtomicF32,
    pub mid_db: AtomicF32,
    pub high_db: AtomicF32,
    pub pan: AtomicF32,
    pub delay_ms: AtomicF32,
    pub bass_db: AtomicF32,
    pub presence_db: AtomicF32,
    pub loudness: AtomicF32,
    pub width: AtomicF32,
    pub comp_thresh_db: AtomicF32,
    pub comp_ratio: AtomicF32,
    pub comp_makeup_db: AtomicF32,
    pub gate_thresh_db: AtomicF32,
    pub comp_on: AtomicBool,
    pub gate_on: AtomicBool,
    pub eq6_on: AtomicBool,
    pub eq6: [AtomicF32; 6],
    pub room_db: AtomicF32,
    pub mode: AtomicU8,
    pub mute: AtomicBool,
    pub solo: AtomicBool,
    pub phase: AtomicBool,
    pub room_mute: AtomicBool,
    pub room_solo: AtomicBool,
    pub peak_l: AtomicF32,
    pub peak_r: AtomicF32,
    // Set by "Identify": the router mixes a short two-note chime into this speaker only.
    pub chime: AtomicBool,
}

impl ChannelShared {
    pub fn new(p: &ChannelParams) -> Arc<Self> {
        let s = Arc::new(Self {
            gain_db: AtomicF32::default(),
            volume_db: AtomicF32::default(),
            low_db: AtomicF32::default(),
            mid_db: AtomicF32::default(),
            high_db: AtomicF32::default(),
            pan: AtomicF32::default(),
            delay_ms: AtomicF32::default(),
            bass_db: AtomicF32::default(),
            presence_db: AtomicF32::default(),
            loudness: AtomicF32::default(),
            width: AtomicF32::default(),
            comp_thresh_db: AtomicF32::default(),
            comp_ratio: AtomicF32::default(),
            comp_makeup_db: AtomicF32::default(),
            gate_thresh_db: AtomicF32::default(),
            comp_on: AtomicBool::new(false),
            gate_on: AtomicBool::new(false),
            eq6_on: AtomicBool::new(true),
            eq6: Default::default(),
            room_db: AtomicF32::default(),
            mode: AtomicU8::new(0),
            mute: AtomicBool::new(false),
            solo: AtomicBool::new(false),
            phase: AtomicBool::new(false),
            room_mute: AtomicBool::new(false),
            room_solo: AtomicBool::new(false),
            peak_l: AtomicF32::default(),
            peak_r: AtomicF32::default(),
            chime: AtomicBool::new(false),
        });
        s.load(p);
        s
    }

    pub fn load(&self, p: &ChannelParams) {
        self.gain_db.store(p.gain_db);
        self.volume_db.store(p.volume_db);
        self.low_db.store(p.low_db);
        self.mid_db.store(p.mid_db);
        self.high_db.store(p.high_db);
        self.pan.store(p.pan);
        self.delay_ms.store(p.delay_ms);
        self.bass_db.store(p.bass_db);
        self.presence_db.store(p.presence_db);
        self.loudness.store(p.loudness);
        self.width.store(p.width);
        self.comp_thresh_db.store(p.comp_thresh_db);
        self.comp_ratio.store(p.comp_ratio);
        self.comp_makeup_db.store(p.comp_makeup_db);
        self.gate_thresh_db.store(p.gate_thresh_db);
        self.comp_on.store(p.comp_on, Ordering::Relaxed);
        self.gate_on.store(p.gate_on, Ordering::Relaxed);
        self.eq6_on.store(p.eq_enabled, Ordering::Relaxed);
        for (a, g) in self.eq6.iter().zip(p.eq_gains) {
            a.store(g);
        }
        self.mode.store(p.mode.as_u8(), Ordering::Relaxed);
        self.mute.store(p.mute, Ordering::Relaxed);
        self.solo.store(p.solo, Ordering::Relaxed);
        self.phase.store(p.phase_invert, Ordering::Relaxed);
    }

    pub fn set_room(&self, volume_db: f32, mute: bool, solo: bool) {
        self.room_db.store(volume_db);
        self.room_mute.store(mute, Ordering::Relaxed);
        self.room_solo.store(solo, Ordering::Relaxed);
    }

    pub fn take_peaks(&self) -> (f32, f32) {
        (self.peak_l.take(), self.peak_r.take())
    }
}

pub struct ChannelRuntime {
    id: String,
    shared: Arc<ChannelShared>,
    out: Producer<f32>,
    wake: Option<Thread>,
    dsp: ChannelDsp,
    delay: Box<DelayLine>,
    limiter: Limiter,
    quiet_run: u32,
    dirty: bool,
    // Frame position inside the identification chime while it plays.
    chime_at: Option<u32>,
}

impl ChannelRuntime {
    pub fn new(id: String, shared: Arc<ChannelShared>, out: Producer<f32>, wake: Option<Thread>, delay_frames: usize) -> Box<Self> {
        Box::new(Self {
            id,
            shared,
            out,
            wake,
            dsp: ChannelDsp::new(),
            delay: Box::new(DelayLine::new(delay_frames)),
            limiter: Limiter::new(),
            quiet_run: 0,
            dirty: false,
            chime_at: None,
        })
    }

    fn tail_blocks(&self) -> u32 {
        ((self.delay.max_delay_frames() + FILTER_TAIL_FRAMES) / BLOCK_FRAMES + 2) as u32
    }
}

#[allow(dead_code)]
pub enum Garbage {
    Channel(Box<ChannelRuntime>),
    Delay(Box<DelayLine>),
}

enum Cmd {
    SetInput(Option<Consumer<f32>>),
    Add(Box<ChannelRuntime>),
    Remove(String),
    SetDelay(String, Box<DelayLine>),
}

pub struct RouterShared {
    pub master_db: AtomicF32,
    pub master_mute: AtomicBool,
    pub peak_in_l: AtomicF32,
    pub peak_in_r: AtomicF32,
    pub dropped_blocks: AtomicU64,
    pub blocks: AtomicU64,
    pub busy_ns: AtomicU64,
    pub max_block_ns: AtomicU64,
    pub quiet_channel_blocks: AtomicU64,
    pub total_channel_blocks: AtomicU64,
    stop: AtomicBool,
}

pub struct Router {
    tx: Sender<Cmd>,
    garbage: Receiver<Garbage>,
    pub shared: Arc<RouterShared>,
    join: Option<JoinHandle<()>>,
    thread: Thread,
}

impl Router {
    pub fn start() -> Router {
        let (tx, rx) = unbounded::<Cmd>();
        let (gtx, grx) = bounded::<Garbage>(64);
        let shared = Arc::new(RouterShared {
            master_db: AtomicF32::default(),
            master_mute: AtomicBool::new(false),
            peak_in_l: AtomicF32::default(),
            peak_in_r: AtomicF32::default(),
            dropped_blocks: AtomicU64::new(0),
            blocks: AtomicU64::new(0),
            busy_ns: AtomicU64::new(0),
            max_block_ns: AtomicU64::new(0),
            quiet_channel_blocks: AtomicU64::new(0),
            total_channel_blocks: AtomicU64::new(0),
            stop: AtomicBool::new(false),
        });
        let s2 = shared.clone();
        let join = thread::Builder::new()
            .name("rezo-router".into())
            .spawn(move || run(rx, gtx, s2))
            .expect("démarrage du thread routeur");
        let thread = join.thread().clone();
        Router { tx, garbage: grx, shared, join: Some(join), thread }
    }

    pub fn waker(&self) -> Thread {
        self.thread.clone()
    }

    pub fn set_input(&self, c: Option<Consumer<f32>>) {
        let _ = self.tx.send(Cmd::SetInput(c));
        self.thread.unpark();
    }

    pub fn add_channel(&self, ch: Box<ChannelRuntime>) {
        let _ = self.tx.send(Cmd::Add(ch));
        self.thread.unpark();
    }

    pub fn remove_channel(&self, id: &str) {
        let _ = self.tx.send(Cmd::Remove(id.to_string()));
        self.thread.unpark();
    }

    pub fn set_delay(&self, id: &str, line: DelayLine) {
        let _ = self.tx.send(Cmd::SetDelay(id.to_string(), Box::new(line)));
        self.thread.unpark();
    }

    pub fn reap(&self) {
        while let Ok(g) = self.garbage.try_recv() {
            drop(g);
        }
    }

    pub fn stop(&mut self) {
        self.shared.stop.store(true, Ordering::Release);
        self.thread.unpark();
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

impl Drop for Router {
    fn drop(&mut self) {
        self.stop();
    }
}

struct MasterCache {
    db: f32,
    muted: bool,
    lin: f32,
}

// "Identify": a soft two-note chime (C6 then G5), about -14 dBFS at its peak, one second long.
const CHIME_FRAMES: u32 = SAMPLE_RATE;

fn chime_sample(frame: u32) -> f32 {
    let t = frame as f32 / SAMPLE_RATE as f32;
    let note = |start: f32, freq: f32| {
        let dt = t - start;
        if dt < 0.0 {
            return 0.0;
        }
        let env = (dt / 0.004).min(1.0) * (-dt / 0.13).exp();
        env * (std::f32::consts::TAU * freq * dt).sin()
    };
    0.2 * (note(0.0, 1046.5) + note(0.22, 784.0))
}

// Adds the chime from `start` into an interleaved stereo block; returns where to continue, or None at the end.
fn add_chime(buf: &mut [f32; BLOCK_SAMPLES], start: u32) -> Option<u32> {
    for (i, frame) in buf.chunks_exact_mut(CHANNELS).enumerate() {
        let n = start + i as u32;
        if n >= CHIME_FRAMES {
            return None;
        }
        let v = chime_sample(n);
        for s in frame.iter_mut() {
            *s += v;
        }
    }
    let next = start + BLOCK_FRAMES as u32;
    (next < CHIME_FRAMES).then_some(next)
}

fn process_block(
    channels: &mut [Box<ChannelRuntime>],
    block: &mut [f32; BLOCK_SAMPLES],
    work: &mut [f32; BLOCK_SAMPLES],
    sh: &RouterShared,
    master: &mut MasterCache,
) {
    let silent_in = block.iter().all(|s| *s == 0.0);
    if !silent_in {
        let mdb = sh.master_db.load();
        let muted = sh.master_mute.load(Ordering::Relaxed);
        if mdb != master.db || muted != master.muted {
            master.db = mdb;
            master.muted = muted;
            master.lin = if muted { 0.0 } else { db_to_lin(mdb) };
        }
        if master.lin != 1.0 {
            for s in block.iter_mut() {
                *s *= master.lin;
            }
        }
        let (il, ir) = block_peaks(block);
        sh.peak_in_l.fetch_max_pos(il);
        sh.peak_in_r.fetch_max_pos(ir);
    }

    let any_solo = channels
        .iter()
        .any(|c| c.shared.solo.load(Ordering::Relaxed) || c.shared.room_solo.load(Ordering::Relaxed));
    // A negative delay cannot advance a live stream: it delays every other speaker instead.
    let base = channels.iter().map(|c| c.shared.delay_ms.load()).fold(0.0f32, f32::min);

    for ch in channels.iter_mut() {
        let s = &ch.shared;
        let soloed = s.solo.load(Ordering::Relaxed) || s.room_solo.load(Ordering::Relaxed);
        let silenced = s.mute.load(Ordering::Relaxed) || s.room_mute.load(Ordering::Relaxed) || (any_solo && !soloed);

        if s.chime.swap(false, Ordering::Relaxed) {
            ch.chime_at = Some(0);
        }
        let quiet = silent_in || silenced;
        ch.quiet_run = if quiet { ch.quiet_run.saturating_add(1) } else { 0 };
        sh.total_channel_blocks.fetch_add(1, Ordering::Relaxed);
        if quiet && ch.quiet_run > ch.tail_blocks() && ch.chime_at.is_some() {
            work.copy_from_slice(&ZEROS);
            ch.chime_at = ch.chime_at.and_then(|t| add_chime(work, t));
            write_block(ch, work, sh);
            continue;
        }
        // Silent for longer than the chain's memory (delay line + filter tails): the output is exactly zero.
        if quiet && ch.quiet_run > ch.tail_blocks() {
            if ch.quiet_run == ch.tail_blocks() + 1 {
                ch.dsp.reset();
                ch.limiter.reset();
            }
            sh.quiet_channel_blocks.fetch_add(1, Ordering::Relaxed);
            write_block(ch, &ZEROS, sh);
            continue;
        }

        let p = DspParams {
            gain_db: s.gain_db.load(),
            volume_db: s.volume_db.load(),
            bus_db: s.room_db.load(),
            low_db: s.low_db.load(),
            mid_db: s.mid_db.load(),
            high_db: s.high_db.load(),
            pan: s.pan.load(),
            bass_db: s.bass_db.load(),
            presence_db: s.presence_db.load(),
            loudness: s.loudness.load(),
            width: s.width.load(),
            comp_on: s.comp_on.load(Ordering::Relaxed),
            comp_thresh_db: s.comp_thresh_db.load(),
            comp_ratio: s.comp_ratio.load(),
            comp_makeup_db: s.comp_makeup_db.load(),
            gate_on: s.gate_on.load(Ordering::Relaxed),
            gate_thresh_db: s.gate_thresh_db.load(),
            mode: s.mode.load(Ordering::Relaxed),
            phase: s.phase.load(Ordering::Relaxed),
            silenced,
            eq6_on: s.eq6_on.load(Ordering::Relaxed),
            eq6: std::array::from_fn(|i| s.eq6[i].load()),
        };
        let delay_frames = (((s.delay_ms.load() - base) * (SAMPLE_RATE as f32 / 1000.0)).round().max(0.0)) as usize;

        work.copy_from_slice(block);
        ch.dsp.process(work, &p);
        ch.delay.process(work, delay_frames);
        if let Some(t) = ch.chime_at {
            ch.chime_at = add_chime(work, t);
        }
        let (pl, pr) = ch.limiter.process(work);
        if pl > 0.0 || pr > 0.0 {
            ch.shared.peak_l.fetch_max_pos(pl);
            ch.shared.peak_r.fetch_max_pos(pr);
        }
        write_block(ch, work, sh);
    }
}

#[inline]
fn write_block(ch: &mut ChannelRuntime, data: &[f32; BLOCK_SAMPLES], sh: &RouterShared) {
    if ch.out.slots() >= BLOCK_SAMPLES {
        if let Ok(chunk) = ch.out.write_chunk_uninit(BLOCK_SAMPLES) {
            chunk.fill_from_iter(data.iter().copied());
        }
        ch.dirty = true;
    } else {
        sh.dropped_blocks.fetch_add(1, Ordering::Relaxed);
    }
}

fn flush_wakes(channels: &mut [Box<ChannelRuntime>]) {
    for ch in channels.iter_mut() {
        if ch.dirty {
            ch.dirty = false;
            if let Some(w) = &ch.wake {
                w.unpark();
            }
        }
    }
}

fn run(rx: Receiver<Cmd>, garbage: Sender<Garbage>, sh: Arc<RouterShared>) {
    let mut mmcss: Option<super::MmcssGuard> = None;

    let mut input: Option<Consumer<f32>> = None;
    let mut channels: Vec<Box<ChannelRuntime>> = Vec::with_capacity(32);
    let mut block = [0.0f32; BLOCK_SAMPLES];
    let mut work = [0.0f32; BLOCK_SAMPLES];
    let mut master = MasterCache { db: f32::NAN, muted: false, lin: 1.0 };

    let block_dur = Duration::from_secs_f64(BLOCK_FRAMES as f64 / SAMPLE_RATE as f64);
    let mut next_silence = Instant::now();
    let mut burst = 0u32;

    while !sh.stop.load(Ordering::Acquire) {
        while let Ok(cmd) = rx.try_recv() {
            match cmd {
                Cmd::SetInput(c) => {
                    if c.is_some() && mmcss.is_none() {
                        mmcss = super::promote_current_thread();
                    } else if c.is_none() {
                        mmcss = None;
                    }
                    input = c;
                    next_silence = Instant::now();
                }
                Cmd::Add(ch) => {
                    channels.retain(|c| c.id != ch.id);
                    channels.push(ch);
                }
                Cmd::Remove(id) => {
                    if let Some(i) = channels.iter().position(|c| c.id == id) {
                        let ch = channels.swap_remove(i);
                        let _ = garbage.try_send(Garbage::Channel(ch));
                    }
                }
                Cmd::SetDelay(id, mut line) => {
                    if let Some(ch) = channels.iter_mut().find(|c| c.id == id) {
                        std::mem::swap(&mut ch.delay, &mut line);
                        ch.quiet_run = 0;
                    }
                    let _ = garbage.try_send(Garbage::Delay(line));
                }
            }
        }

        if input.is_none() && channels.is_empty() {
            burst = 0;
            thread::park_timeout(Duration::from_secs(1));
            next_silence = Instant::now();
            continue;
        }
        let have_block = match input.as_mut() {
            Some(c) => {
                let avail = c.slots();
                if avail > MAX_BACKLOG_SAMPLES {
                    let skip = (avail - TARGET_BACKLOG_SAMPLES) / CHANNELS * CHANNELS;
                    if let Ok(chunk) = c.read_chunk(skip) {
                        chunk.commit_all();
                    }
                }
                if c.slots() >= BLOCK_SAMPLES {
                    if let Ok(chunk) = c.read_chunk(BLOCK_SAMPLES) {
                        let (a, b) = chunk.as_slices();
                        block[..a.len()].copy_from_slice(a);
                        block[a.len()..].copy_from_slice(b);
                        chunk.commit_all();
                    }
                    true
                } else {
                    false
                }
            }
            None => {
                let now = Instant::now();
                if now >= next_silence {
                    next_silence += block_dur;
                    if now > next_silence + Duration::from_millis(50) {
                        next_silence = now;
                    }
                    block.fill(0.0);
                    true
                } else {
                    false
                }
            }
        };
        if !have_block {
            flush_wakes(&mut channels);
            burst = 0;
            let wait = if input.is_some() {
                Duration::from_millis(20)
            } else {
                next_silence.saturating_duration_since(Instant::now()).max(Duration::from_millis(1))
            };
            thread::park_timeout(wait);
            continue;
        }

        let t0 = Instant::now();
        process_block(&mut channels, &mut block, &mut work, &sh, &mut master);
        let dt = t0.elapsed().as_nanos() as u64;
        sh.blocks.fetch_add(1, Ordering::Relaxed);
        sh.busy_ns.fetch_add(dt, Ordering::Relaxed);
        sh.max_block_ns.fetch_max(dt, Ordering::Relaxed);

        burst += 1;
        if burst >= MAX_BURST_BLOCKS {
            flush_wakes(&mut channels);
            burst = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::rooms::ChannelParams;

    #[test]
    fn chime_plays_once_on_one_speaker_and_master_mute_silences_the_rest() {
        let (mut chans, mut cons) = make_channels(2, ChannelParams::default());
        let sh = RouterShared::test_new();
        let mut master = MasterCache { db: f32::NAN, muted: false, lin: 1.0 };
        let mut block = [0.0f32; BLOCK_SAMPLES];
        let mut work = [0.0f32; BLOCK_SAMPLES];
        // Long silence first, so both channels are on the zero-output shortcut.
        for _ in 0..200 {
            process_block(&mut chans, &mut block, &mut work, &sh, &mut master);
            for c in cons.iter_mut() {
                let n = c.slots();
                c.read_chunk(n).unwrap().commit_all();
            }
        }
        chans[1].shared.chime.store(true, Ordering::Relaxed);
        let blocks = CHIME_FRAMES as usize / BLOCK_FRAMES + 3;
        let (mut peak0, mut peak1, mut tail) = (0.0f32, 0.0f32, 0.0f32);
        for b in 0..blocks {
            process_block(&mut chans, &mut block, &mut work, &sh, &mut master);
            let read = |c: &mut Consumer<f32>| {
                let n = c.slots();
                let ch = c.read_chunk(n).unwrap();
                let (a, b2) = ch.as_slices();
                let m = a.iter().chain(b2).fold(0.0f32, |m, v| m.max(v.abs()));
                ch.commit_all();
                m
            };
            peak0 = peak0.max(read(&mut cons[0]));
            let p1 = read(&mut cons[1]);
            peak1 = peak1.max(p1);
            if b == blocks - 1 {
                tail = p1;
            }
        }
        assert_eq!(peak0, 0.0, "the other speaker stays silent");
        assert!(peak1 > 0.1 && peak1 < 0.35, "chime level {peak1}");
        assert_eq!(tail, 0.0, "the chime ends by itself");
        assert!(chans[1].chime_at.is_none());

        // Master mute: music in, nothing out.
        sh.master_mute.store(true, Ordering::Relaxed);
        block.iter_mut().enumerate().for_each(|(i, s)| *s = ((i as f32) * 0.05).sin() * 0.5);
        let mut out = 0.0f32;
        for _ in 0..60 {
            let mut b = block;
            process_block(&mut chans, &mut b, &mut work, &sh, &mut master);
            for c in cons.iter_mut() {
                let n = c.slots();
                let ch = c.read_chunk(n).unwrap();
                let (a, b2) = ch.as_slices();
                out = out.max(a.iter().chain(b2).fold(0.0f32, |m, v| m.max(v.abs())));
                ch.commit_all();
            }
        }
        assert_eq!(out, 0.0, "master mute silences every speaker");
    }

    fn make_channels(n: usize, params: ChannelParams) -> (Vec<Box<ChannelRuntime>>, Vec<Consumer<f32>>) {
        let mut chans = vec![];
        let mut cons = vec![];
        for i in 0..n {
            let (p, c) = rtrb::RingBuffer::new(BLOCK_SAMPLES * 4);
            cons.push(c);
            chans.push(ChannelRuntime::new(format!("c{i}"), ChannelShared::new(&params), p, None, DEFAULT_DELAY_FRAMES));
        }
        (chans, cons)
    }

    fn tone(block: &mut [f32; BLOCK_SAMPLES], phase: &mut f32, amp: f32) {
        for f in block.chunks_exact_mut(2) {
            let v = (*phase).sin() * amp;
            f[0] = v;
            f[1] = v;
            *phase += 2.0 * std::f32::consts::PI * 440.0 / SAMPLE_RATE as f32;
        }
    }

    fn drain(cons: &mut [Consumer<f32>]) {
        for c in cons.iter_mut() {
            let n = c.slots();
            if n > 0 {
                if let Ok(ch) = c.read_chunk(n) {
                    ch.commit_all();
                }
            }
        }
    }

    #[test]
    fn quiet_path_is_bit_exact() {
        let mut p = ChannelParams::default();
        p.low_db = 4.0;
        p.high_db = -3.0;
        p.delay_ms = 30.0;
        let (mut chans, mut cons) = make_channels(1, p);
        let sh = RouterShared::test_new();
        let mut master = MasterCache { db: f32::NAN, muted: false, lin: 1.0 };
        let (mut block, mut work, mut phase) = ([0.0f32; BLOCK_SAMPLES], [0.0f32; BLOCK_SAMPLES], 0.0f32);

        let (mut refs, mut rcons) = make_channels(1, ChannelParams { low_db: 4.0, high_db: -3.0, delay_ms: 30.0, ..Default::default() });
        let mut master_ref = MasterCache { db: f32::NAN, muted: false, lin: 1.0 };
        let mut max_diff = 0.0f32;
        let mut saw_quiet = false;
        for i in 0..(400 * 4) {
            if i < 400 {
                tone(&mut block, &mut phase, 0.5);
            } else {
                block.fill(0.0);
            }
            let mut b2 = block;
            process_block(&mut chans, &mut block, &mut work, &sh, &mut master);
            refs[0].quiet_run = 0;
            process_block(&mut refs, &mut b2, &mut work, &sh, &mut master_ref);
            let (mut a, mut b) = ([0.0f32; BLOCK_SAMPLES], [0.0f32; BLOCK_SAMPLES]);
            let ca = cons[0].read_chunk(BLOCK_SAMPLES).unwrap();
            let (x, y) = ca.as_slices();
            a[..x.len()].copy_from_slice(x);
            a[x.len()..].copy_from_slice(y);
            ca.commit_all();
            let cb = rcons[0].read_chunk(BLOCK_SAMPLES).unwrap();
            let (x, y) = cb.as_slices();
            b[..x.len()].copy_from_slice(x);
            b[x.len()..].copy_from_slice(y);
            cb.commit_all();
            for k in 0..BLOCK_SAMPLES {
                max_diff = max_diff.max((a[k] - b[k]).abs());
            }
            saw_quiet |= chans[0].quiet_run > chans[0].tail_blocks();
        }
        assert!(saw_quiet, "le chemin silence n'a jamais été pris");
        assert!(max_diff < 1e-6, "écart max {max_diff}");
    }

    #[test]
    #[ignore]
    fn bench_process_block() {
        for (name, amp, eq, delay, force_full) in [
            ("silence, SANS raccourci (référence)", 0.0f32, false, 0.0f32, true),
            ("silence, chemin rapide", 0.0, false, 0.0, false),
            ("signal, réglages neutres", 0.3, false, 0.0, false),
            ("signal, EQ 3 bandes + retard 40 ms", 0.3, true, 40.0, false),
        ] {
            let mut p = ChannelParams::default();
            if eq {
                p.low_db = 3.0;
                p.mid_db = -2.0;
                p.high_db = 4.0;
            }
            p.delay_ms = delay;
            let (mut chans, mut cons) = make_channels(4, p);
            let sh = RouterShared::test_new();
            let mut master = MasterCache { db: f32::NAN, muted: false, lin: 1.0 };
            let (mut block, mut work, mut phase) = ([0.0f32; BLOCK_SAMPLES], [0.0f32; BLOCK_SAMPLES], 0.0f32);
            let n = 100_000u32;
            let mut total = Duration::ZERO;
            for i in 0..n {
                if amp > 0.0 {
                    tone(&mut block, &mut phase, amp);
                } else {
                    block.fill(0.0);
                }
                if force_full {
                    for c in chans.iter_mut() {
                        c.quiet_run = 0;
                    }
                }
                let t = Instant::now();
                process_block(&mut chans, &mut block, &mut work, &sh, &mut master);
                total += t.elapsed();
                if i % 3 == 0 {
                    drain(&mut cons);
                }
            }
            let per_block_us = total.as_secs_f64() * 1e6 / n as f64;
            println!("{name:<40} {per_block_us:7.2} µs/bloc (4 enceintes)  =>  {:.3} % d'un cœur en temps réel", per_block_us * 400.0 / 1e6 * 100.0);
        }
    }
}

impl RouterShared {
    #[cfg(test)]
    pub fn test_new() -> RouterShared {
        RouterShared {
            master_db: AtomicF32::default(),
            master_mute: AtomicBool::new(false),
            peak_in_l: AtomicF32::default(),
            peak_in_r: AtomicF32::default(),
            dropped_blocks: AtomicU64::new(0),
            blocks: AtomicU64::new(0),
            busy_ns: AtomicU64::new(0),
            max_block_ns: AtomicU64::new(0),
            quiet_channel_blocks: AtomicU64::new(0),
            total_channel_blocks: AtomicU64::new(0),
            stop: AtomicBool::new(false),
        }
    }
}
