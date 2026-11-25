use std::f64::consts::PI;

use super::{CHANNELS, SAMPLE_RATE};

#[inline]
pub fn db_to_lin(db: f32) -> f32 {
    if db <= -89.5 {
        0.0
    } else {
        10f32.powf(db * 0.05)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Coef {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
}

impl Coef {
    pub const UNITY: Coef = Coef { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0 };

    fn norm(b0: f64, b1: f64, b2: f64, a0: f64, a1: f64, a2: f64) -> Coef {
        let i = 1.0 / a0;
        Coef { b0: (b0 * i) as f32, b1: (b1 * i) as f32, b2: (b2 * i) as f32, a1: (a1 * i) as f32, a2: (a2 * i) as f32 }
    }

    pub fn low_shelf(f0: f64, gain_db: f64) -> Coef {
        let a = 10f64.powf(gain_db / 40.0);
        let w0 = 2.0 * PI * f0 / SAMPLE_RATE as f64;
        let (s, c) = w0.sin_cos();
        let alpha = s / 2.0 * 2f64.sqrt();
        let k = 2.0 * a.sqrt() * alpha;
        Self::norm(
            a * ((a + 1.0) - (a - 1.0) * c + k),
            2.0 * a * ((a - 1.0) - (a + 1.0) * c),
            a * ((a + 1.0) - (a - 1.0) * c - k),
            (a + 1.0) + (a - 1.0) * c + k,
            -2.0 * ((a - 1.0) + (a + 1.0) * c),
            (a + 1.0) + (a - 1.0) * c - k,
        )
    }

    pub fn high_shelf(f0: f64, gain_db: f64) -> Coef {
        let a = 10f64.powf(gain_db / 40.0);
        let w0 = 2.0 * PI * f0 / SAMPLE_RATE as f64;
        let (s, c) = w0.sin_cos();
        let alpha = s / 2.0 * 2f64.sqrt();
        let k = 2.0 * a.sqrt() * alpha;
        Self::norm(
            a * ((a + 1.0) + (a - 1.0) * c + k),
            -2.0 * a * ((a - 1.0) + (a + 1.0) * c),
            a * ((a + 1.0) + (a - 1.0) * c - k),
            (a + 1.0) - (a - 1.0) * c + k,
            2.0 * ((a - 1.0) - (a + 1.0) * c),
            (a + 1.0) - (a - 1.0) * c - k,
        )
    }

    pub fn peaking(f0: f64, gain_db: f64, q: f64) -> Coef {
        let a = 10f64.powf(gain_db / 40.0);
        let w0 = 2.0 * PI * f0 / SAMPLE_RATE as f64;
        let (s, c) = w0.sin_cos();
        let alpha = s / (2.0 * q);
        Self::norm(1.0 + alpha * a, -2.0 * c, 1.0 - alpha * a, 1.0 + alpha / a, -2.0 * c, 1.0 - alpha / a)
    }
}

#[derive(Clone, Copy)]
struct Band {
    c: Coef,
    z: [[f32; 2]; CHANNELS],
    on: bool,
}

impl Band {
    const fn new() -> Self {
        Self { c: Coef::UNITY, z: [[0.0; 2]; CHANNELS], on: false }
    }
    fn set(&mut self, c: Coef, gain_db: f32) {
        self.c = c;
        self.on = gain_db.abs() > 0.05;
        if !self.on {
            self.z = [[0.0; 2]; CHANNELS];
        }
    }
    #[inline(always)]
    fn tick(&mut self, ch: usize, x: f32) -> f32 {
        let c = self.c;
        let z = &mut self.z[ch];
        let y = c.b0 * x + z[0];
        z[0] = c.b1 * x - c.a1 * y + z[1];
        z[1] = c.b2 * x - c.a2 * y;
        y
    }
    fn flush_denormals(&mut self) {
        for z in self.z.iter_mut().flatten() {
            if z.abs() < 1e-18 {
                *z = 0.0;
            }
        }
    }
}

pub struct Eq3 {
    low: Band,
    mid: Band,
    high: Band,
    last: [f32; 3],
}

impl Eq3 {
    pub fn new() -> Self {
        Self { low: Band::new(), mid: Band::new(), high: Band::new(), last: [f32::NAN; 3] }
    }

    pub fn set_gains(&mut self, low_db: f32, mid_db: f32, high_db: f32) {
        if low_db != self.last[0] {
            self.low.set(Coef::low_shelf(100.0, low_db as f64), low_db);
            self.last[0] = low_db;
        }
        if mid_db != self.last[1] {
            self.mid.set(Coef::peaking(1000.0, mid_db as f64, 0.8), mid_db);
            self.last[1] = mid_db;
        }
        if high_db != self.last[2] {
            self.high.set(Coef::high_shelf(8000.0, high_db as f64), high_db);
            self.last[2] = high_db;
        }
    }

    pub fn reset(&mut self) {
        for b in [&mut self.low, &mut self.mid, &mut self.high] {
            b.z = [[0.0; 2]; CHANNELS];
        }
    }

    pub fn process(&mut self, buf: &mut [f32]) {
        if !(self.low.on || self.mid.on || self.high.on) {
            return;
        }
        for frame in buf.chunks_exact_mut(CHANNELS) {
            for (ch, s) in frame.iter_mut().enumerate() {
                let mut x = *s;
                if self.low.on {
                    x = self.low.tick(ch, x);
                }
                if self.mid.on {
                    x = self.mid.tick(ch, x);
                }
                if self.high.on {
                    x = self.high.tick(ch, x);
                }
                *s = x;
            }
        }
        self.low.flush_denormals();
        self.mid.flush_denormals();
        self.high.flush_denormals();
    }
}

impl Default for Eq3 {
    fn default() -> Self {
        Self::new()
    }
}

// Six-band equaliser on Spotify's bands: low shelf at 60 Hz, peaks from 150 Hz to 2.4 kHz, high shelf at 15 kHz.
pub const EQ6_FREQS: [f64; 6] = [60.0, 150.0, 400.0, 1000.0, 2400.0, 15000.0];
pub const EQ6_MAX_DB: f32 = 12.0;
const EQ6_Q: f64 = 1.0;
// Gains glide by at most 0.2 dB per 2.5 ms block (12 dB in 150 ms): each coefficient step is too small to click.
const EQ6_GLIDE_DB: f32 = 0.2;

pub struct Eq6 {
    bands: [Band; 6],
    cur: [f32; 6],
}

impl Eq6 {
    pub fn new() -> Self {
        Self { bands: [Band::new(); 6], cur: [0.0; 6] }
    }

    fn coef(band: usize, db: f32) -> Coef {
        match band {
            0 => Coef::low_shelf(EQ6_FREQS[0], db as f64),
            5 => Coef::high_shelf(EQ6_FREQS[5], db as f64),
            _ => Coef::peaking(EQ6_FREQS[band], db as f64, EQ6_Q),
        }
    }

    pub fn reset(&mut self) {
        for b in self.bands.iter_mut() {
            b.z = [[0.0; 2]; CHANNELS];
        }
    }

    #[cfg(test)]
    pub fn current_gains(&self) -> [f32; 6] {
        self.cur
    }

    // Bypass glides every band to 0 dB; the target gains themselves are left untouched by the caller.
    pub fn process(&mut self, buf: &mut [f32], gains: &[f32; 6], enabled: bool) {
        for (i, &g) in gains.iter().enumerate() {
            let target = if enabled && g.is_finite() { g.clamp(-EQ6_MAX_DB, EQ6_MAX_DB) } else { 0.0 };
            let cur = self.cur[i];
            if cur != target {
                let next = if (target - cur).abs() <= EQ6_GLIDE_DB { target } else { cur + EQ6_GLIDE_DB.copysign(target - cur) };
                self.cur[i] = next;
                self.bands[i].set(Self::coef(i, next), next);
            }
        }
        if !self.bands.iter().any(|b| b.on) {
            return;
        }
        for frame in buf.chunks_exact_mut(CHANNELS) {
            for (ch, s) in frame.iter_mut().enumerate() {
                let mut x = *s;
                for b in self.bands.iter_mut() {
                    if b.on {
                        x = b.tick(ch, x);
                    }
                }
                *s = x;
            }
        }
        for b in self.bands.iter_mut() {
            b.flush_denormals();
        }
    }
}

impl Default for Eq6 {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Tone {
    bands: [Band; 4],
    last: [f32; 4],
}

impl Tone {
    pub fn new() -> Self {
        Self { bands: [Band::new(); 4], last: [f32::NAN; 4] }
    }

    pub fn set(&mut self, bass_db: f32, presence_db: f32, loudness: f32) {
        let keys = [bass_db, presence_db, loudness, loudness];
        let loud_lo = loudness * 9.0;
        let loud_hi = loudness * 6.0;
        for i in 0..4 {
            if keys[i] != self.last[i] {
                self.last[i] = keys[i];
                match i {
                    0 => self.bands[0].set(Coef::peaking(70.0, bass_db as f64, 0.9), bass_db),
                    1 => self.bands[1].set(Coef::peaking(3500.0, presence_db as f64, 0.9), presence_db),
                    2 => self.bands[2].set(Coef::low_shelf(90.0, loud_lo as f64), loud_lo),
                    _ => self.bands[3].set(Coef::high_shelf(10000.0, loud_hi as f64), loud_hi),
                }
            }
        }
    }

    pub fn reset(&mut self) {
        for b in self.bands.iter_mut() {
            b.z = [[0.0; 2]; CHANNELS];
        }
    }

    pub fn process(&mut self, buf: &mut [f32]) {
        if !self.bands.iter().any(|b| b.on) {
            return;
        }
        for frame in buf.chunks_exact_mut(CHANNELS) {
            for (ch, s) in frame.iter_mut().enumerate() {
                let mut x = *s;
                for b in self.bands.iter_mut() {
                    if b.on {
                        x = b.tick(ch, x);
                    }
                }
                *s = x;
            }
        }
        for b in self.bands.iter_mut() {
            b.flush_denormals();
        }
    }
}

impl Default for Tone {
    fn default() -> Self {
        Self::new()
    }
}

struct Gate {
    env: f32,
    gain: f32,
    open: bool,
    hold: u32,
}

impl Gate {
    const fn new() -> Self {
        Self { env: 0.0, gain: 1.0, open: true, hold: 0 }
    }

    fn reset(&mut self) {
        *self = Self::new();
    }

    fn process(&mut self, buf: &mut [f32], thresh_db: f32) {
        let open_at = db_to_lin(thresh_db);
        let close_at = open_at * 0.7;
        let det_rel = (-1.0 / (0.03 * SAMPLE_RATE as f32)).exp();
        let atk = 1.0 - (-1.0 / (0.002 * SAMPLE_RATE as f32)).exp();
        let rel = 1.0 - (-1.0 / (0.06 * SAMPLE_RATE as f32)).exp();
        let hold_frames = (SAMPLE_RATE as f32 * 0.05) as u32;
        for f in buf.chunks_exact_mut(CHANNELS) {
            let peak = f[0].abs().max(f[1].abs());
            self.env = if peak > self.env { peak } else { peak + (self.env - peak) * det_rel };
            if self.env > open_at {
                self.open = true;
                self.hold = hold_frames;
            } else if self.env < close_at {
                if self.hold > 0 {
                    self.hold -= 1;
                } else {
                    self.open = false;
                }
            }
            let target = if self.open { 1.0 } else { 0.0 };
            self.gain += (target - self.gain) * if self.open { atk } else { rel };
            f[0] *= self.gain;
            f[1] *= self.gain;
        }
        if self.env < 1e-18 {
            self.env = 0.0;
        }
        if self.gain < 1e-12 {
            self.gain = 0.0;
        }
    }
}

struct Comp {
    env: f32,
}

impl Comp {
    const fn new() -> Self {
        Self { env: 0.0 }
    }

    fn reset(&mut self) {
        self.env = 0.0;
    }

    fn process(&mut self, buf: &mut [f32], thresh_db: f32, ratio: f32, makeup_db: f32) {
        let thr = db_to_lin(thresh_db).max(1e-6);
        let slope = 1.0 / ratio.max(1.0) - 1.0;
        let atk = (-1.0 / (0.005 * SAMPLE_RATE as f32)).exp();
        let rel = (-1.0 / (0.12 * SAMPLE_RATE as f32)).exp();
        let makeup = db_to_lin(makeup_db);
        for f in buf.chunks_exact_mut(CHANNELS) {
            let peak = f[0].abs().max(f[1].abs());
            self.env = if peak > self.env { peak + (self.env - peak) * atk } else { peak + (self.env - peak) * rel };
            let g = if self.env > thr { (self.env / thr).powf(slope) } else { 1.0 } * makeup;
            f[0] *= g;
            f[1] *= g;
        }
        if self.env < 1e-18 {
            self.env = 0.0;
        }
    }
}

#[derive(Clone, Copy)]
pub struct DspParams {
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
    pub gain_db: f32,
    pub volume_db: f32,
    pub bus_db: f32,
    pub low_db: f32,
    pub mid_db: f32,
    pub high_db: f32,
    pub pan: f32,
    pub mode: u8,
    pub phase: bool,
    pub silenced: bool,
    pub eq6_on: bool,
    pub eq6: [f32; 6],
}

impl Default for DspParams {
    fn default() -> Self {
        Self {
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
            gain_db: 0.0,
            volume_db: 0.0,
            bus_db: 0.0,
            low_db: 0.0,
            mid_db: 0.0,
            high_db: 0.0,
            pan: 0.0,
            mode: 0,
            phase: false,
            silenced: false,
            eq6_on: true,
            eq6: [0.0; 6],
        }
    }
}

pub struct ChannelDsp {
    eq: Eq3,
    eq6: Eq6,
    tone: Tone,
    gate: Gate,
    comp: Comp,
    cur_l: f32,
    cur_r: f32,
    gain_key: [f32; 3],
    gain_lin: f32,
    pan_key: f32,
    pan_gl: f32,
    pan_gr: f32,
}

impl ChannelDsp {
    pub fn new() -> Self {
        Self {
            eq: Eq3::new(),
            eq6: Eq6::new(),
            tone: Tone::new(),
            gate: Gate::new(),
            comp: Comp::new(),
            cur_l: 0.0,
            cur_r: 0.0,
            gain_key: [f32::NAN; 3],
            gain_lin: 0.0,
            pan_key: f32::NAN,
            pan_gl: 1.0,
            pan_gr: 1.0,
        }
    }

    pub fn reset(&mut self) {
        self.eq.reset();
        self.eq6.reset();
        self.tone.reset();
        self.gate.reset();
        self.comp.reset();
    }

    pub fn process(&mut self, buf: &mut [f32], p: &DspParams) {
        if p.gate_on {
            self.gate.process(buf, p.gate_thresh_db);
        } else if !self.gate.open || self.gate.gain != 1.0 {
            self.gate.reset();
        }
        self.eq.set_gains(p.low_db, p.mid_db, p.high_db);
        self.eq.process(buf);
        self.tone.set(p.bass_db, p.presence_db, p.loudness);
        self.tone.process(buf);
        self.eq6.process(buf, &p.eq6, p.eq6_on);
        if p.comp_on {
            self.comp.process(buf, p.comp_thresh_db, p.comp_ratio, p.comp_makeup_db);
        } else if self.comp.env != 0.0 {
            self.comp.reset();
        }
        if p.mode == 0 && p.width != 1.0 {
            let w = p.width;
            for f in buf.chunks_exact_mut(CHANNELS) {
                let m = 0.5 * (f[0] + f[1]);
                let s = 0.5 * (f[0] - f[1]) * w;
                f[0] = m + s;
                f[1] = m - s;
            }
        }

        if p.pan != self.pan_key {
            self.pan_key = p.pan;
            self.pan_gl = (p.pan.max(0.0) * std::f32::consts::FRAC_PI_2).cos();
            self.pan_gr = ((-p.pan).max(0.0) * std::f32::consts::FRAC_PI_2).cos();
        }
        let g = if p.silenced {
            0.0
        } else {
            let key = [p.gain_db, p.volume_db, p.bus_db];
            if key != self.gain_key {
                self.gain_key = key;
                self.gain_lin = db_to_lin(p.gain_db) * db_to_lin(p.volume_db) * db_to_lin(p.bus_db);
            }
            self.gain_lin
        };
        let sign = if p.phase { -1.0 } else { 1.0 };
        let (tl, tr) = (g * self.pan_gl * sign, g * self.pan_gr * sign);

        if tl == self.cur_l && tr == self.cur_r && p.mode == 0 {
            if tl == 1.0 && tr == 1.0 {
                return;
            }
            for f in buf.chunks_exact_mut(CHANNELS) {
                f[0] *= tl;
                f[1] *= tr;
            }
            return;
        }

        let n = (buf.len() / CHANNELS) as f32;
        let (dl, dr) = ((tl - self.cur_l) / n, (tr - self.cur_r) / n);
        let (mut cl, mut cr) = (self.cur_l, self.cur_r);
        for f in buf.chunks_exact_mut(CHANNELS) {
            let (l, r) = (f[0], f[1]);
            let (l, r) = match p.mode {
                1 => {
                    let m = 0.5 * (l + r);
                    (m, m)
                }
                2 => (l, l),
                3 => (r, r),
                _ => (l, r),
            };
            cl += dl;
            cr += dr;
            f[0] = l * cl;
            f[1] = r * cr;
        }
        self.cur_l = tl;
        self.cur_r = tr;
    }
}

impl Default for ChannelDsp {
    fn default() -> Self {
        Self::new()
    }
}

pub struct DelayLine {
    buf: Vec<f32>,
    cap: usize,
    w: usize,
    cur: usize,
}

impl DelayLine {
    pub fn new(max_delay_frames: usize) -> Self {
        let cap = max_delay_frames + super::BLOCK_FRAMES + 2;
        Self { buf: vec![0.0; cap * CHANNELS], cap, w: 0, cur: 0 }
    }

    pub fn max_delay_frames(&self) -> usize {
        self.cap - super::BLOCK_FRAMES - 2
    }

    pub fn bytes_for(max_delay_frames: usize) -> usize {
        (max_delay_frames + super::BLOCK_FRAMES + 2) * CHANNELS * std::mem::size_of::<f32>()
    }

    #[inline(always)]
    fn tap(&self, delay: usize) -> usize {
        let r = self.w + self.cap - delay;
        if r >= self.cap {
            r - self.cap
        } else {
            r
        }
    }

    pub fn process(&mut self, io: &mut [f32], target: usize) {
        let target = target.min(self.max_delay_frames());
        let n = io.len() / CHANNELS;
        if target == 0 && self.cur == 0 {
            let first = n.min(self.cap - self.w);
            self.buf[self.w * 2..(self.w + first) * 2].copy_from_slice(&io[..first * 2]);
            self.buf[..(n - first) * 2].copy_from_slice(&io[first * 2..n * 2]);
            self.w = (self.w + n) % self.cap;
            return;
        }
        let fading = target != self.cur;
        for i in 0..n {
            let w = self.w;
            let (il, ir) = (io[i * 2], io[i * 2 + 1]);
            self.buf[w * 2] = il;
            self.buf[w * 2 + 1] = ir;
            let a = self.tap(self.cur);
            let (mut ol, mut or) = (self.buf[a * 2], self.buf[a * 2 + 1]);
            if fading {
                let b = self.tap(target);
                let t = (i + 1) as f32 / n as f32;
                ol += (self.buf[b * 2] - ol) * t;
                or += (self.buf[b * 2 + 1] - or) * t;
            }
            io[i * 2] = ol;
            io[i * 2 + 1] = or;
            self.w = if w + 1 == self.cap { 0 } else { w + 1 };
        }
        self.cur = target;
    }
}

pub struct Limiter {
    env: f32,
    release: f32,
}

const LIMIT_THRESHOLD: f32 = 0.92;
const SOFT_KNEE: f32 = 0.85;

#[inline(always)]
pub fn soft_clip(x: f32) -> f32 {
    let a = x.abs();
    if a <= SOFT_KNEE {
        x
    } else {
        let k = 1.0 - SOFT_KNEE;
        (SOFT_KNEE + k * ((a - SOFT_KNEE) / k).tanh()).copysign(x)
    }
}

impl Limiter {
    pub fn new() -> Self {
        Self { env: 0.0, release: (-1.0 / (0.08 * SAMPLE_RATE as f32)).exp() }
    }

    pub fn reset(&mut self) {
        self.env = 0.0;
    }

    pub fn process(&mut self, buf: &mut [f32]) -> (f32, f32) {
        let (mut pl, mut pr) = (0.0f32, 0.0f32);
        for f in buf.chunks_exact_mut(CHANNELS) {
            let peak = f[0].abs().max(f[1].abs());
            self.env = if peak > self.env { peak } else { peak + (self.env - peak) * self.release };
            let gr = if self.env > LIMIT_THRESHOLD { LIMIT_THRESHOLD / self.env } else { 1.0 };
            f[0] = soft_clip(f[0] * gr);
            f[1] = soft_clip(f[1] * gr);
            pl = pl.max(f[0].abs());
            pr = pr.max(f[1].abs());
        }
        if self.env < 1e-18 {
            self.env = 0.0;
        }
        (pl, pr)
    }
}

impl Default for Limiter {
    fn default() -> Self {
        Self::new()
    }
}

pub fn block_peaks(buf: &[f32]) -> (f32, f32) {
    let (mut l, mut r) = (0.0f32, 0.0f32);
    for f in buf.chunks_exact(CHANNELS) {
        l = l.max(f[0].abs());
        r = r.max(f[1].abs());
    }
    (l, r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eq_flat_is_transparent() {
        let mut eq = Eq3::new();
        eq.set_gains(0.0, 0.0, 0.0);
        let mut b = [0.3f32, -0.2, 0.5, 0.1];
        let c = b;
        eq.process(&mut b);
        assert_eq!(b, c);
    }

    #[test]
    fn low_shelf_boosts_dc() {
        let mut eq = Eq3::new();
        eq.set_gains(6.0, 0.0, 0.0);
        let mut last = 0.0;
        for _ in 0..200 {
            let mut b = [1.0f32; 240];
            eq.process(&mut b);
            last = b[239];
        }
        assert!((last - 1.995).abs() < 0.05, "gain DC = {last}");
    }

    fn sine_block(freq: f32, start: usize, amp: f32) -> [f32; 240] {
        let mut b = [0.0f32; 240];
        for (i, f) in b.chunks_exact_mut(2).enumerate() {
            let v = amp * (2.0 * std::f32::consts::PI * freq * (start + i) as f32 / SAMPLE_RATE as f32).sin();
            f[0] = v;
            f[1] = v;
        }
        b
    }

    fn peak(b: &[f32]) -> f32 {
        b.iter().fold(0.0f32, |m, x| m.max(x.abs()))
    }

    // Steady-state gain at `freq` once the glide has settled.
    fn eq6_gain_db(gains: [f32; 6], enabled: bool, freq: f32) -> f32 {
        let mut eq = Eq6::new();
        let mut out = 0.0f32;
        for k in 0..800 {
            let mut b = sine_block(freq, k * 120, 0.1);
            eq.process(&mut b, &gains, enabled);
            if k >= 700 {
                out = out.max(peak(&b));
            }
        }
        20.0 * (out / 0.1).log10()
    }

    #[test]
    fn eq6_bands_hit_their_frequencies() {
        let mut g = [0.0; 6];
        g[0] = 6.0;
        assert!((eq6_gain_db(g, true, 30.0) - 6.0).abs() < 0.6, "low shelf");
        let mut g = [0.0; 6];
        g[3] = 6.0;
        assert!((eq6_gain_db(g, true, 1000.0) - 6.0).abs() < 0.3, "peak at 1 kHz");
        assert!(eq6_gain_db(g, true, 60.0).abs() < 0.5, "1 kHz band leaves 60 Hz alone");
        let mut g = [0.0; 6];
        g[5] = -6.0;
        assert!((eq6_gain_db(g, true, 20000.0) + 6.0).abs() < 1.0, "high shelf");
    }

    #[test]
    fn eq6_bypass_is_flat_but_keeps_the_curve() {
        let g = [6.5, 4.5, 1.0, 0.0, 0.0, 0.0];
        assert!(eq6_gain_db(g, false, 60.0).abs() < 0.05);
        let mut eq = Eq6::new();
        let mut b = sine_block(440.0, 0, 0.1);
        let c = b;
        eq.process(&mut b, &g, false);
        assert_eq!(b, c, "disabled and flat: samples untouched");
        assert_eq!(eq.current_gains(), [0.0; 6]);
    }

    #[test]
    fn eq6_gain_jumps_glide_without_clicks() {
        let mut eq = Eq6::new();
        let mut prev_peak = 0.0f32;
        let mut worst_step = 0.0f32;
        let mut prev_sample = 0.0f32;
        for k in 0..400 {
            let gains = if k < 50 { [0.0; 6] } else { [0.0, 0.0, 0.0, 12.0, 0.0, 0.0] };
            let mut b = sine_block(1000.0, k * 120, 0.05);
            eq.process(&mut b, &gains, true);
            let p = peak(&b);
            if k > 50 && prev_peak > 0.0 {
                worst_step = worst_step.max(20.0 * (p / prev_peak).log10());
            }
            for f in b.chunks_exact(2) {
                assert!((f[0] - prev_sample).abs() < 0.2, "discontinuité de {:.3}", (f[0] - prev_sample).abs());
                prev_sample = f[0];
            }
            prev_peak = p;
        }
        assert!(worst_step < 0.5, "pas de {worst_step:.2} dB entre deux blocs");
        assert_eq!(eq.current_gains()[3], 12.0);
    }

    #[test]
    fn eq6_instances_are_independent() {
        let (mut a, mut b) = (ChannelDsp::new(), ChannelDsp::new());
        let pa = DspParams { eq6: [6.0, 0.0, 0.0, 0.0, 0.0, 0.0], ..Default::default() };
        let pb = DspParams::default();
        let mut ob = [0.0f32; 240];
        for k in 0..600 {
            let mut xa = sine_block(60.0, k * 120, 0.1);
            let mut xb = xa;
            a.process(&mut xa, &pa);
            b.process(&mut xb, &pb);
            ob = xb;
        }
        let input = sine_block(60.0, 599 * 120, 0.1);
        assert_eq!(ob, input, "le réglage de A ne touche pas B");
    }

    #[test]
    fn delay_shifts_impulse() {
        let mut d = DelayLine::new(2000);
        let mut b = vec![0.0f32; 480];
        b[0] = 1.0;
        b[1] = 1.0;
        d.process(&mut b, 0);
        assert_eq!(b[0], 1.0);
        let mut d = DelayLine::new(2000);
        let mut first = vec![0.0f32; 240];
        d.process(&mut first, 100);
        let mut imp = vec![0.0f32; 480];
        imp[0] = 1.0;
        imp[1] = 1.0;
        d.process(&mut imp, 100);
        assert_eq!(imp[200], 1.0);
        assert_eq!(imp[0], 0.0);
    }

    fn run(p: DspParams, input: &[f32], blocks: usize) -> Vec<f32> {
        let mut d = ChannelDsp::new();
        let mut out = Vec::new();
        for _ in 0..blocks {
            let mut b = input.to_vec();
            d.process(&mut b, &p);
            out = b;
        }
        out
    }

    #[test]
    fn new_controls_default_to_transparent() {
        let input: Vec<f32> = (0..240).map(|i| ((i / 2) as f32 * 0.1).sin() * 0.4 * if i % 2 == 0 { 1.0 } else { 0.5 }).collect();
        let out = run(DspParams::default(), &input, 3);
        assert_eq!(out, input);
    }

    #[test]
    fn width_zero_collapses_to_mono() {
        let input = [0.8f32, -0.2, 0.6, 0.0, 0.1, 0.3, 0.5, 0.5];
        let out = run(DspParams { width: 0.0, ..Default::default() }, &input, 3);
        for f in out.chunks_exact(2) {
            assert!((f[0] - f[1]).abs() < 1e-6);
        }
        assert!((out[0] - 0.3).abs() < 1e-6);
    }

    #[test]
    fn bass_boost_raises_low_tone() {
        let n = 4800;
        let tone: Vec<f32> = (0..n).flat_map(|i| {
            let v = (2.0 * std::f32::consts::PI * 70.0 * i as f32 / 48000.0).sin() * 0.2;
            [v, v]
        }).collect();
        let mut d = ChannelDsp::new();
        let p = DspParams { bass_db: 12.0, ..Default::default() };
        let mut peak = 0.0f32;
        for _ in 0..4 {
            let mut b = tone.clone();
            d.process(&mut b, &p);
            peak = b.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        }
        assert!(peak > 0.6, "crête = {peak}");
    }

    #[test]
    fn compressor_reduces_loud_signal() {
        let loud = vec![0.8f32; 9600];
        let p = DspParams { comp_on: true, comp_thresh_db: -20.0, comp_ratio: 4.0, ..Default::default() };
        let out = run(p, &loud, 5);
        assert!(out[out.len() - 1] < 0.5, "sortie = {}", out[out.len() - 1]);
    }

    #[test]
    fn gate_mutes_below_threshold_and_passes_above() {
        let p = DspParams { gate_on: true, gate_thresh_db: -40.0, ..Default::default() };
        let quiet = vec![0.001f32; 9600];
        assert!(run(p, &quiet, 4).iter().all(|s| s.abs() < 1e-4));
        let loud = vec![0.3f32; 9600];
        let out = run(p, &loud, 4);
        assert!((out[out.len() - 1] - 0.3).abs() < 1e-3);
    }

    #[test]
    fn limiter_never_exceeds_unity() {
        let mut l = Limiter::new();
        let mut b: Vec<f32> = (0..2400).map(|i| ((i as f32) * 0.05).sin() * 4.0).collect();
        let (pl, pr) = l.process(&mut b);
        assert!(pl <= 1.0 && pr <= 1.0);
        assert!(b.iter().all(|s| s.abs() <= 1.0));
    }
}
