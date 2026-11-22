//! Plugins propios de NoName Studio: NN EQ, NN Dynamics, NN Verb, NN Space, NN Wave y NN Tune.
use crate::dsp::{mtof, Biquad, FType};
use crate::project::*;
use std::f32::consts::{PI, TAU};
use std::sync::OnceLock;

// ============================ NN EQ ============================

pub fn band_biquad(b: &Band, sr: f32) -> Biquad {
    match b.kind {
        BandKind::Bell => Biquad::new(FType::Peak, b.freq, b.q, b.gain, sr),
        BandKind::LowShelf => Biquad::new(FType::LowShelf, b.freq, b.q.max(0.3), b.gain, sr),
        BandKind::HighShelf => Biquad::new(FType::HighShelf, b.freq, b.q.max(0.3), b.gain, sr),
        BandKind::LowCut => Biquad::new(FType::High, b.freq, b.q, 0.0, sr),
        BandKind::HighCut => Biquad::new(FType::Low, b.freq, b.q, 0.0, sr),
    }
}

/// Respuesta total del ecualizador en dB a la frecuencia `f`.
pub fn eq_response_db(eq: &[Band; 6], f: f32, sr: f32) -> f32 {
    eq.iter().filter(|b| b.on).map(|b| 20.0 * band_biquad(b, sr).mag(f, sr).max(1e-6).log10()).sum()
}

pub struct Eq6 {
    bq: [[Biquad; 2]; 6],
    params: Option<[Band; 6]>,
    on: [bool; 6],
    pub flat: bool,
}
impl Default for Eq6 {
    fn default() -> Self {
        Eq6 { bq: [[Biquad::default(); 2]; 6], params: None, on: [false; 6], flat: true }
    }
}
impl Eq6 {
    pub fn set(&mut self, eq: &[Band; 6], sr: f32) {
        if self.params.as_ref() == Some(eq) {
            return;
        }
        for (i, b) in eq.iter().enumerate() {
            let n = band_biquad(b, sr);
            for side in 0..2 {
                let z = self.bq[i][side];
                self.bq[i][side] = n;
                self.bq[i][side].copy_state(&z);
            }
            self.on[i] = b.on;
        }
        self.flat = eq_is_flat(eq);
        self.params = Some(*eq);
    }
    #[inline]
    pub fn mono(&mut self, mut x: f32) -> f32 {
        if self.flat {
            return x;
        }
        for i in 0..6 {
            if self.on[i] {
                x = self.bq[i][0].run(x);
            }
        }
        x
    }
    #[inline]
    pub fn stereo(&mut self, mut l: f32, mut r: f32) -> (f32, f32) {
        if self.flat {
            return (l, r);
        }
        for i in 0..6 {
            if self.on[i] {
                l = self.bq[i][0].run(l);
                r = self.bq[i][1].run(r);
            }
        }
        (l, r)
    }
}

// ============================ NN Dynamics ============================

pub struct Comp {
    env: f32,
    pub gr_db: f32,
    coefs: (f32, f32),
    key: (f32, f32),
}
impl Default for Comp {
    fn default() -> Self {
        Comp { env: 0.0, gr_db: 0.0, coefs: (0.0, 0.0), key: (-1.0, -1.0) }
    }
}
impl Comp {
    #[inline]
    pub fn gain(&mut self, level: f32, p: &CompP, sr: f32) -> f32 {
        if !p.on {
            self.gr_db = 0.0;
            return 1.0;
        }
        if self.key != (p.attack_ms, p.release_ms) {
            self.key = (p.attack_ms, p.release_ms);
            self.coefs = ((-1.0 / (p.attack_ms.max(0.1) * 0.001 * sr)).exp(), (-1.0 / (p.release_ms.max(1.0) * 0.001 * sr)).exp());
        }
        let c = if level > self.env { self.coefs.0 } else { self.coefs.1 };
        self.env = level + c * (self.env - level);
        let db = 20.0 * self.env.max(1e-6).log10();
        // rodilla suave de 6 dB
        let over = db - p.threshold;
        let knee = 6.0;
        let red = if over <= -knee / 2.0 {
            0.0
        } else if over >= knee / 2.0 {
            over * (1.0 - 1.0 / p.ratio.max(1.0))
        } else {
            let x = over + knee / 2.0;
            (1.0 - 1.0 / p.ratio.max(1.0)) * x * x / (2.0 * knee)
        };
        self.gr_db = red;
        10f32.powf((p.makeup - red) / 20.0)
    }
}

pub struct Limiter {
    g: f32,
    pub gr_db: f32,
}
impl Default for Limiter {
    fn default() -> Self {
        Limiter { g: 1.0, gr_db: 0.0 }
    }
}
impl Limiter {
    #[inline]
    pub fn run(&mut self, l: f32, r: f32, p: &LimP, sr: f32) -> (f32, f32) {
        if !p.on {
            self.gr_db = 0.0;
            return (l.clamp(-1.0, 1.0), r.clamp(-1.0, 1.0));
        }
        let pre = 10f32.powf(p.gain_db / 20.0);
        let (l, r) = (l * pre, r * pre);
        let ceil = 10f32.powf(p.ceiling_db / 20.0);
        let pk = l.abs().max(r.abs());
        let target = if pk > ceil { ceil / pk } else { 1.0 };
        if target < self.g {
            self.g = target;
        } else {
            let rel = (-1.0 / (p.release_ms.max(1.0) * 0.001 * sr)).exp();
            self.g = target + rel * (self.g - target);
        }
        self.gr_db = -20.0 * self.g.max(1e-6).log10();
        ((l * self.g).clamp(-ceil, ceil), (r * self.g).clamp(-ceil, ceil))
    }
}

// ============================ NN Verb ============================

struct Line {
    buf: Vec<f32>,
    len: usize,
    idx: usize,
    store: f32,
}
impl Line {
    fn new(max: usize) -> Line {
        Line { buf: vec![0.0; max.max(2)], len: max.max(2), idx: 0, store: 0.0 }
    }
}

pub struct Verb {
    combs: Vec<Line>,
    alls: Vec<Line>,
    pre: Vec<f32>,
    pre_idx: usize,
    hp: Biquad,
    key: Option<VerbP>,
    sr: f32,
}
const COMB: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLP: [usize; 4] = [556, 441, 341, 225];
impl Verb {
    pub fn new(sr: f32) -> Verb {
        let sc = sr / 44100.0;
        let mut combs = vec![];
        let mut alls = vec![];
        for side in 0..2 {
            for &l in &COMB {
                combs.push(Line::new(((l + side * 23) as f32 * sc * 1.6) as usize));
            }
            for &l in &ALLP {
                alls.push(Line::new(((l + side * 23) as f32 * sc) as usize));
            }
        }
        Verb { combs, alls, pre: vec![0.0; (sr * 0.26) as usize], pre_idx: 0, hp: Biquad::default(), key: None, sr }
    }
    fn configure(&mut self, p: &VerbP) {
        if self.key.as_ref() == Some(p) {
            return;
        }
        let sc = self.sr / 44100.0;
        let f = 0.35 + p.size.clamp(0.0, 1.0) * 1.2;
        for side in 0..2 {
            for (i, &l) in COMB.iter().enumerate() {
                let c = &mut self.combs[side * 8 + i];
                c.len = (((l + side * 23) as f32 * sc * f) as usize).clamp(2, c.buf.len());
                c.idx %= c.len;
            }
        }
        self.hp.set(FType::High, p.lowcut.max(20.0), 0.707, 0.0, self.sr);
        self.key = Some(*p);
    }
    #[inline]
    pub fn run(&mut self, x: f32, p: &VerbP) -> (f32, f32) {
        self.configure(p);
        let pd = ((p.predelay_ms.clamp(0.0, 250.0) * 0.001 * self.sr) as usize).min(self.pre.len() - 1);
        let n = self.pre.len();
        self.pre[self.pre_idx] = x;
        let xin = self.hp.run(self.pre[(self.pre_idx + n - pd) % n]) * 0.015;
        self.pre_idx = (self.pre_idx + 1) % n;
        let fb = 0.7 + p.decay.clamp(0.0, 1.0) * 0.285;
        let damp = p.damp.clamp(0.0, 0.95);
        let mut out = [0.0f32; 2];
        for side in 0..2 {
            let mut acc = 0.0;
            for c in &mut self.combs[side * 8..side * 8 + 8] {
                let y = c.buf[c.idx];
                c.store = y * (1.0 - damp) + c.store * damp;
                c.buf[c.idx] = xin + c.store * fb;
                c.idx = (c.idx + 1) % c.len;
                acc += y;
            }
            for a in &mut self.alls[side * 4..side * 4 + 4] {
                let b = a.buf[a.idx];
                a.buf[a.idx] = acc + b * 0.5;
                a.idx = (a.idx + 1) % a.len;
                acc = b - acc;
            }
            out[side] = acc;
        }
        let w = p.width.clamp(0.0, 1.0);
        let (w1, w2) = (w / 2.0 + 0.5, (1.0 - w) / 2.0);
        ((out[0] * w1 + out[1] * w2) * p.ret, (out[1] * w1 + out[0] * w2) * p.ret)
    }
}

// ============================ NN Space ============================

/// Red de retardos con realimentación (8 líneas, matriz de Householder) y modulación.
pub struct Space {
    lines: Vec<Vec<f32>>,
    w: usize,
    lp: [f32; 8],
    lfo: f32,
    sr: f32,
    diff: [Line; 4],
}
const RATIOS: [f32; 8] = [1.0, 1.131, 1.273, 1.414, 1.531, 1.672, 1.797, 1.971];
impl Space {
    pub fn new(sr: f32) -> Space {
        let max = (sr * 4.2) as usize;
        let d = [142usize, 107, 379, 277];
        Space {
            lines: (0..8).map(|_| vec![0.0; max]).collect(),
            w: 0,
            lp: [0.0; 8],
            lfo: 0.0,
            sr,
            diff: [Line::new(d[0]), Line::new(d[1]), Line::new(d[2]), Line::new(d[3])],
        }
    }
    #[inline]
    pub fn run(&mut self, x: f32, p: &SpaceP) -> (f32, f32) {
        // difusión de entrada (cadena de pasa-todo)
        let mut s = x * 0.25;
        for a in &mut self.diff {
            let b = a.buf[a.idx];
            a.buf[a.idx] = s + b * (0.35 + p.density * 0.35);
            a.idx = (a.idx + 1) % a.len;
            s = b - s * (0.35 + p.density * 0.35);
        }
        let n = self.lines[0].len();
        self.lfo = (self.lfo + p.mod_rate.max(0.01) / self.sr).fract();
        let base = p.delay_ms.clamp(10.0, 2000.0) * 0.001 * self.sr;
        let depth = p.mod_depth.clamp(0.0, 1.0) * 0.008 * self.sr;
        let mut o = [0f32; 8];
        for i in 0..8 {
            let m = (TAU * (self.lfo + i as f32 * 0.125)).sin() * depth;
            let d = (base * RATIOS[i] + m).clamp(2.0, (n - 2) as f32);
            let rp = self.w as f32 + n as f32 - d;
            let i0 = rp.floor() as usize % n;
            let fr = rp.fract();
            let l = &self.lines[i];
            let y = l[i0] + (l[(i0 + 1) % n] - l[i0]) * fr;
            let damp = p.damp.clamp(0.0, 0.95);
            self.lp[i] = y * (1.0 - damp) + self.lp[i] * damp;
            o[i] = self.lp[i];
        }
        let sum: f32 = o.iter().sum::<f32>() * (2.0 / 8.0);
        let fb = p.feedback.clamp(0.0, 0.985);
        for i in 0..8 {
            let v = (o[i] - sum) * fb + s;
            self.lines[i][self.w] = v.clamp(-4.0, 4.0);
        }
        self.w = (self.w + 1) % n;
        let l = (o[0] + o[2] + o[4] + o[6]) * 0.5 * p.ret;
        let r = (o[1] + o[3] + o[5] + o[7]) * 0.5 * p.ret;
        (l, r)
    }
}

// ============================ NN Wave ============================

pub const WT_SIZE: usize = 2048;
pub const WT_NAMES: [&str; 5] = ["Seno", "Triángulo", "Sierra", "Cuadrada", "Formante"];

pub fn tables() -> &'static Vec<Vec<f32>> {
    static T: OnceLock<Vec<Vec<f32>>> = OnceLock::new();
    T.get_or_init(|| {
        let mk = |amp: &dyn Fn(usize) -> f32| {
            let mut t = vec![0f32; WT_SIZE];
            for h in 1..=48usize {
                let a = amp(h);
                if a == 0.0 {
                    continue;
                }
                for (i, v) in t.iter_mut().enumerate() {
                    *v += a * (TAU * h as f32 * i as f32 / WT_SIZE as f32).sin();
                }
            }
            let pk = t.iter().fold(0f32, |m, x| m.max(x.abs())).max(1e-6);
            t.iter_mut().for_each(|x| *x /= pk);
            t
        };
        vec![
            mk(&|h| if h == 1 { 1.0 } else { 0.0 }),
            mk(&|h| if h % 2 == 1 { (if (h / 2) % 2 == 0 { 1.0 } else { -1.0 }) / (h * h) as f32 } else { 0.0 }),
            mk(&|h| 1.0 / h as f32),
            mk(&|h| if h % 2 == 1 { 1.0 / h as f32 } else { 0.0 }),
            mk(&|h| {
                let f1 = (-((h as f32 - 5.0) / 1.6).powi(2)).exp();
                let f2 = 0.6 * (-((h as f32 - 12.0) / 2.5).powi(2)).exp();
                (f1 + f2 + 0.15 / h as f32) * if h % 3 == 0 { 0.7 } else { 1.0 }
            }),
        ]
    })
}

#[inline]
pub fn wt_sample(pos: f32, ph: f32) -> f32 {
    let t = tables();
    let pos = pos.clamp(0.0, 4.0);
    let i = (pos.floor() as usize).min(3);
    let fr = pos - i as f32;
    let x = ph.fract() * WT_SIZE as f32;
    let j = x as usize % WT_SIZE;
    let k = (j + 1) % WT_SIZE;
    let xf = x.fract();
    let a = t[i][j] + (t[i][k] - t[i][j]) * xf;
    let b = t[i + 1][j] + (t[i + 1][k] - t[i + 1][j]) * xf;
    a + (b - a) * fr
}

#[inline]
pub fn adsr(t: f32, len: f32, e: &[f32; 4]) -> f32 {
    let (a, d, s, r) = (e[0].max(0.001), e[1].max(0.001), e[2].clamp(0.0, 1.0), e[3].max(0.005));
    let lvl = |t: f32| {
        if t < a {
            t / a
        } else if t < a + d {
            1.0 - (1.0 - s) * (t - a) / d
        } else {
            s
        }
    };
    if t < len {
        lvl(t)
    } else {
        (lvl(len) * (1.0 - (t - len) / r)).max(0.0)
    }
}

/// Estado de una voz de NN Wave.
#[derive(Clone, Copy)]
pub struct WaveState {
    pub p: SynthP,
    ph: [f32; 16],
    ic1: f32,
    ic2: f32,
    lfo: f32,
}
impl WaveState {
    pub fn new(p: SynthP, seed: u32) -> WaveState {
        let mut ph = [0f32; 16];
        for (i, v) in ph.iter_mut().enumerate() {
            *v = ((seed.wrapping_mul(2654435761).wrapping_add(i as u32 * 40503)) >> 8) as f32 / (1u32 << 24) as f32;
        }
        WaveState { p, ph, ic1: 0.0, ic2: 0.0, lfo: 0.0 }
    }
    pub fn end_time(&self, len: f32) -> f32 {
        len + self.p.amp[3].max(0.005)
    }
    #[inline]
    pub fn next(&mut self, t: f32, len: f32, m: f32, vel: f32, sr: f32) -> f32 {
        let p = &self.p;
        let dt = 1.0 / sr;
        self.lfo = (self.lfo + p.lfo_rate * dt).fract();
        let lfo = (self.lfo * TAU).sin() * p.lfo_depth;
        let mut sum = 0.0;
        let mut slot = 0;
        for o in p.osc.iter() {
            let n = o.unison.clamp(1, 8) as usize;
            if !o.on {
                slot += n;
                continue;
            }
            let pitch = m + (o.oct * 12 + o.semi) as f32 + o.fine / 100.0 + if p.lfo_target == 2 { lfo * 2.0 } else { 0.0 };
            let f0 = mtof(pitch);
            let pos = o.wt + if p.lfo_target == 1 { lfo * 2.0 } else { 0.0 };
            let norm = o.level / (n as f32).sqrt();
            for u in 0..n {
                if slot >= 16 {
                    break;
                }
                let spread = if n > 1 { u as f32 / (n - 1) as f32 * 2.0 - 1.0 } else { 0.0 };
                let f = f0 * 2f32.powf(spread * o.detune * 0.5 / 12.0);
                self.ph[slot] = (self.ph[slot] + f * dt).fract();
                sum += wt_sample(pos, self.ph[slot]) * norm;
                slot += 1;
            }
        }
        // filtro de estado variable (TPT) con envolvente y LFO
        let fe = adsr(t, len, &p.fadsr);
        let mut fc = p.cutoff * 2f32.powf(p.fenv * fe * 6.0);
        if p.lfo_target == 0 {
            fc *= 2f32.powf(lfo * 2.0);
        }
        let fc = fc.clamp(20.0, sr * 0.45);
        let g = (PI * fc / sr).tan();
        let k = 2.0 - 1.9 * p.reso.clamp(0.0, 1.0);
        let a1 = 1.0 / (1.0 + g * (g + k));
        let v3 = sum - self.ic2;
        let v1 = a1 * self.ic1 + g * a1 * v3;
        let v2 = self.ic2 + g * v1;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        let y = match p.ftype {
            1 => sum - k * v1 - v2,
            2 => v1,
            _ => v2,
        };
        y * adsr(t, len, &p.amp) * p.level * vel * 0.3
    }
}

// ============================ NN Tune ============================

pub struct TuneResult {
    pub out: Vec<f32>,
    /// tono detectado y corregido por cuadro (nota MIDI fraccional; 0 = sin voz)
    pub detected: Vec<f32>,
    pub corrected: Vec<f32>,
    pub hop: usize,
}

pub fn scale_has(key: u8, scale: u8, note: i32) -> bool {
    let d = (note - key as i32).rem_euclid(12);
    match scale {
        0 => [0, 2, 3, 5, 7, 8, 10].contains(&d),
        1 => [0, 2, 4, 5, 7, 9, 11].contains(&d),
        _ => true,
    }
}

pub fn yin(x: &[f32], sr: f32) -> Option<f32> {
    let w = x.len() / 2;
    let tmin = (sr / 1000.0) as usize;
    let tmax = ((sr / 70.0) as usize).min(w - 1);
    let mut d = vec![0f32; tmax + 1];
    for tau in 1..=tmax {
        let mut s = 0.0;
        for j in 0..w {
            let v = x[j] - x[j + tau];
            s += v * v;
        }
        d[tau] = s;
    }
    let mut cum = 0.0;
    let mut dn = vec![1f32; tmax + 1];
    for tau in 1..=tmax {
        cum += d[tau];
        dn[tau] = d[tau] * tau as f32 / cum.max(1e-9);
    }
    let mut tau = tmin.max(2);
    while tau < tmax {
        if dn[tau] < 0.15 {
            while tau + 1 < tmax && dn[tau + 1] < dn[tau] {
                tau += 1;
            }
            let (a, b, c) = (dn[tau - 1], dn[tau], dn[tau + 1]);
            let den = a - 2.0 * b + c;
            let off = if den.abs() > 1e-9 { 0.5 * (a - c) / den } else { 0.0 };
            return Some(sr / (tau as f32 + off.clamp(-1.0, 1.0)));
        }
        tau += 1;
    }
    None
}

/// Corrección de afinación: detecta el tono (YIN), lo lleva a la nota más cercana de la escala
/// y desplaza el tono con un desplazador de dos cabezales (sin cambiar la duración).
pub fn autotune(x: &[f32], sr: f32, p: &TuneP) -> TuneResult {
    let hop = 256usize;
    let win = 2048usize.min(x.len().max(4) / 2 * 2).max(256);
    let frames = x.len() / hop + 1;
    let mut detected = vec![0f32; frames];
    let mut corrected = vec![0f32; frames];
    let mut ratio = vec![1f32; frames];
    let alpha = 1.0 - p.speed.clamp(0.0, 1.0) * 0.95;
    let mut rs = 1.0f32;
    let mut last_note: Option<i32> = None;
    // la detección se hace a media resolución (4 veces más rápida, sobra para la voz)
    let xd: Vec<f32> = x.chunks(2).map(|c| c.iter().sum::<f32>() / c.len() as f32).collect();
    let wd2 = win / 2;
    for f in 0..frames {
        let s = f * hop / 2;
        let target = if s + wd2 <= xd.len() {
            let fr = &xd[s..s + wd2];
            let rms = (fr.iter().map(|v| v * v).sum::<f32>() / wd2 as f32).sqrt();
            if rms > 0.01 { yin(fr, sr / 2.0) } else { None }
        } else {
            None
        };
        if target.is_none() {
            last_note = None;
        }
        let r = match target {
            Some(hz) if p.on => {
                let midi = 69.0 + 12.0 * (hz / 440.0).log2();
                let mut best = midi.round() as i32;
                let mut bd = f32::MAX;
                for n in (midi.round() as i32 - 6)..=(midi.round() as i32 + 6) {
                    if scale_has(p.key, p.scale, n) && (n as f32 - midi).abs() < bd {
                        bd = (n as f32 - midi).abs();
                        best = n;
                    }
                }
                // histéresis: no salta de nota mientras la voz siga cerca de la anterior
                if let Some(prev) = last_note {
                    if prev != best && scale_has(p.key, p.scale, prev) && (prev as f32 - midi).abs() < bd + 0.35 {
                        best = prev;
                    }
                }
                last_note = Some(best);
                detected[f] = midi;
                corrected[f] = best as f32;
                2f32.powf((best as f32 - midi) / 12.0)
            }
            Some(hz) => {
                detected[f] = 69.0 + 12.0 * (hz / 440.0).log2();
                1.0
            }
            None => 1.0,
        };
        rs += (r - rs) * alpha;
        ratio[f] = rs;

    }
    // desplazador de tono de dos cabezales con ventanas complementarias
    let wd = 0.035 * sr;
    let mut out = vec![0f32; x.len()];
    let mut phase = 0.0f32;
    let read = |pos: f32| -> f32 {
        if pos < 0.0 {
            return 0.0;
        }
        let i = pos as usize;
        if i + 1 >= x.len() {
            return 0.0;
        }
        x[i] + (x[i + 1] - x[i]) * pos.fract()
    };
    for (n, o) in out.iter_mut().enumerate() {
        let fpos = n as f32 / hop as f32;
        let f0 = (fpos as usize).min(frames - 1);
        let f1 = (f0 + 1).min(frames - 1);
        let r = ratio[f0] + (ratio[f1] - ratio[f0]) * fpos.fract();
        phase = (phase + (1.0 - r) / wd).rem_euclid(1.0);
        let p2 = (phase + 0.5).fract();
        let d1 = phase * wd;
        let d2 = p2 * wd;
        let g1 = (PI * phase).sin().powi(2);
        let g2 = (PI * p2).sin().powi(2);
        let base = n as f32 + wd * 0.5;
        let wet = read(base - d1) * g1 + read(base - d2) * g2;
        *o = (wet * p.mix + x[n] * (1.0 - p.mix)) * p.gain;
    }
    TuneResult { out, detected, corrected, hop }
}
