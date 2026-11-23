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

/// Una voz extra de un estilo: semitonos fijos o grados de la escala (3ra = 2 grados, 5ta = 4).
#[derive(Clone, Copy, PartialEq)]
pub enum Voz {
    Semis(i32),
    Grados(i32),
}

/// Un estilo (preset) de NN Tune.
pub struct Estilo {
    pub nombre: &'static str,
    pub categoria: u8,
    pub ayuda: &'static str,
    /// la voz principal se mueve estos semitonos después de afinarse
    pub mueve: i32,
    /// velocidad de corrección permitida (mín, máx): (0,0) = robot
    pub vel: (f32, f32),
    /// voces extra con su volumen
    pub voces: [Option<(Voz, f32)>; 2],
    /// cambia también el timbre (ardilla, monstruo): usa el desplazador de dos cabezales
    pub timbre: bool,
}

pub const CATEGORIAS: [&str; 4] = ["Esenciales", "Hip hop", "Hyperpop", "Ciencia ficción"];

pub const ESTILOS: [Estilo; 15] = [
    Estilo { nombre: "Clásico", categoria: 0, ayuda: "Corrección clásica, ajustable con la perilla", mueve: 0, vel: (0.0, 1.0), voces: [None, None], timbre: false },
    Estilo { nombre: "Natural", categoria: 0, ayuda: "Corrige sin que se note", mueve: 0, vel: (0.55, 1.0), voces: [None, None], timbre: false },
    Estilo { nombre: "Robot", categoria: 0, ayuda: "El efecto robot de siempre, al instante", mueve: 0, vel: (0.0, 0.0), voces: [None, None], timbre: false },
    Estilo { nombre: "Dúo", categoria: 0, ayuda: "Tu voz y otra una octava más grave", mueve: 0, vel: (0.0, 1.0), voces: [Some((Voz::Semis(-12), 0.6)), None], timbre: false },
    Estilo { nombre: "Tercera", categoria: 0, ayuda: "Tu voz y una armonía una tercera arriba", mueve: 0, vel: (0.0, 1.0), voces: [Some((Voz::Grados(2), 0.6)), None], timbre: false },
    Estilo { nombre: "Ardilla", categoria: 0, ayuda: "Tu voz una octava más aguda", mueve: 12, vel: (0.0, 1.0), voces: [None, None], timbre: true },
    Estilo { nombre: "Gran armonía", categoria: 0, ayuda: "Tu voz con una tercera y una quinta: un coro de tres", mueve: 0, vel: (0.0, 1.0), voces: [Some((Voz::Grados(2), 0.5)), Some((Voz::Grados(4), 0.42))], timbre: false },
    Estilo { nombre: "Trap", categoria: 1, ayuda: "Robot duro con una octava grave por debajo", mueve: 0, vel: (0.0, 0.0), voces: [Some((Voz::Semis(-12), 0.35)), None], timbre: false },
    Estilo { nombre: "Melódico", categoria: 1, ayuda: "Corrección suave con una armonía una tercera abajo", mueve: 0, vel: (0.2, 0.6), voces: [Some((Voz::Grados(-2), 0.4)), None], timbre: false },
    Estilo { nombre: "Voz grave", categoria: 1, ayuda: "Tu voz una octava más grave, sin sonar a monstruo", mueve: -12, vel: (0.0, 1.0), voces: [None, None], timbre: false },
    Estilo { nombre: "Hyper", categoria: 2, ayuda: "Aguda y robótica, al estilo hyperpop", mueve: 12, vel: (0.0, 0.0), voces: [None, None], timbre: true },
    Estilo { nombre: "Brillo", categoria: 2, ayuda: "Robot con una octava aguda encima que brilla", mueve: 0, vel: (0.0, 0.0), voces: [Some((Voz::Semis(12), 0.45)), None], timbre: false },
    Estilo { nombre: "Robot grave", categoria: 3, ayuda: "Una octava abajo y robótica: voz de máquina", mueve: -12, vel: (0.0, 0.0), voces: [None, None], timbre: true },
    Estilo { nombre: "Alien", categoria: 3, ayuda: "Robot con una quinta arriba y una octava abajo", mueve: 0, vel: (0.0, 0.0), voces: [Some((Voz::Semis(7), 0.55)), Some((Voz::Semis(-12), 0.4))], timbre: false },
    Estilo { nombre: "Coro", categoria: 3, ayuda: "Tu voz con una octava abajo y otra arriba", mueve: 0, vel: (0.4, 1.0), voces: [Some((Voz::Semis(-12), 0.45)), Some((Voz::Semis(12), 0.32))], timbre: false },
];

fn estilo(p: &TuneP) -> &'static Estilo {
    &ESTILOS[(p.estilo as usize).min(ESTILOS.len() - 1)]
}

/// ¿La nota está permitida por la escala de NN Tune?
pub fn en_escala(p: &TuneP, n: i32) -> bool {
    if p.mask != 0 {
        (p.mask >> n.rem_euclid(12)) & 1 == 1
    } else {
        scale_has(p.key, p.scale, n)
    }
}
/// Velocidad de corrección efectiva según el estilo.
pub fn velocidad(p: &TuneP) -> f32 {
    let (a, b) = estilo(p).vel;
    p.speed.clamp(a, b)
}
/// Mueve `k` grados de la escala desde la nota `n` (k negativo = hacia abajo).
pub fn grados(p: &TuneP, n: i32, k: i32) -> i32 {
    let todas = (0..12).all(|d| en_escala(p, n + d));
    if todas {
        // cromática: tercera mayor y quinta justa
        return n + match k {
            2 => 4,
            -2 => -3,
            4 => 7,
            -4 => -7,
            x => x,
        };
    }
    let paso = k.signum();
    let mut m = n;
    let mut c = 0;
    while c < k.abs() && (m - n).abs() < 12 {
        m += paso;
        if en_escala(p, m) {
            c += 1;
        }
    }
    m
}
/// Nota de la voz principal y de las voces extra (con su volumen) para la nota corregida.
pub fn voces(p: &TuneP, best: i32) -> (i32, [Option<(i32, f32)>; 2]) {
    let e = estilo(p);
    let mut extra = [None, None];
    for (k, v) in e.voces.iter().enumerate() {
        if let Some((voz, g)) = v {
            let nota = match voz {
                Voz::Semis(s) => best + s,
                Voz::Grados(d) => grados(p, best, *d),
            };
            extra[k] = Some((nota, *g * p.mezcla));
        }
    }
    (best + e.mueve, extra)
}
/// ¿Este estilo usa el desplazador de dos cabezales (cambia el timbre) o el algoritmo de baja demora?
pub fn usa_cabezales(p: &TuneP) -> bool {
    estilo(p).timbre || p.algoritmo == 1
}
/// Nota de la escala más cercana, con histéresis respecto a la anterior.
fn nota_objetivo(p: &TuneP, midi: f32, last: Option<i32>) -> i32 {
    let mut best = midi.round() as i32;
    let mut bd = f32::MAX;
    for n in (midi.round() as i32 - 6)..=(midi.round() as i32 + 6) {
        if en_escala(p, n) && (n as f32 - midi).abs() < bd {
            bd = (n as f32 - midi).abs();
            best = n;
        }
    }
    if let Some(prev) = last {
        if prev != best && en_escala(p, prev) && (prev as f32 - midi).abs() < bd + 0.35 {
            best = prev;
        }
    }
    best
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
    let (mut bt, mut bv) = (0usize, f32::MAX);
    for t in tmin.max(2)..tmax {
        if dn[t] < bv {
            bv = dn[t];
            bt = t;
        }
    }
    if bv < 0.3 && bt > 0 {
        let (a, b, c) = (dn[bt - 1], dn[bt], dn[bt + 1]);
        let den = a - 2.0 * b + c;
        let off = if den.abs() > 1e-9 { 0.5 * (a - c) / den } else { 0.0 };
        return Some(sr / (bt as f32 + off.clamp(-1.0, 1.0)));
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
        match target {
            Some(hz) => {
                let midi = 69.0 + 12.0 * (hz / 440.0).log2();
                detected[f] = midi;
                if p.on {
                    let best = nota_objetivo(p, midi, last_note);
                    last_note = Some(best);
                    corrected[f] = voces(p, best).0 as f32;
                }
            }
            None => last_note = None,
        }
    }
    // el sonido sale del mismo motor que en vivo (PSOLA), compensando su retardo
    let mut tl = TuneLive::new(sr);
    let lat = tl.latency(p);
    let mut out = Vec::with_capacity(x.len());
    for i in 0..x.len() + lat {
        let v = tl.process(x.get(i).copied().unwrap_or(0.0), p);
        if i >= lat {
            out.push(v * p.gain);
        }
    }
    TuneResult { out, detected, corrected, hop }
}

/// Afinado con una escala que cambia en el tiempo (por ejemplo, siguiendo los acordes).
/// `mascara(i)` da las notas permitidas en la muestra `i`.
pub fn autotune_con(x: &[f32], sr: f32, p: &TuneP, mascara: &dyn Fn(usize) -> u16) -> Vec<f32> {
    let mut tl = TuneLive::new(sr);
    let lat = tl.latency(p);
    let mut q = *p;
    let mut out = Vec::with_capacity(x.len());
    for i in 0..x.len() + lat {
        if i % 256 == 0 {
            q.mask = mascara(i);
        }
        let v = tl.process(x.get(i).copied().unwrap_or(0.0), &q);
        if i >= lat {
            out.push(v * p.gain);
        }
    }
    out
}

/// YIN sin pedir memoria (para el hilo de audio). `d` y `dn` deben medir al menos tmax+1.
pub fn yin_buf(x: &[f32], sr: f32, d: &mut [f32], dn: &mut [f32]) -> Option<f32> {
    let w = x.len() / 2;
    let tmin = (sr / 1000.0) as usize;
    let tmax = ((sr / 70.0) as usize).min(w - 1).min(d.len() - 1);
    for tau in 1..=tmax {
        let mut s = 0.0;
        for j in 0..w {
            let v = x[j] - x[j + tau];
            s += v * v;
        }
        d[tau] = s;
    }
    let mut cum = 0.0;
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
    let (mut bt, mut bv) = (0usize, f32::MAX);
    for t in tmin.max(2)..tmax {
        if dn[t] < bv {
            bv = dn[t];
            bt = t;
        }
    }
    if bv < 0.3 && bt > 0 {
        let (a, b, c) = (dn[bt - 1], dn[bt], dn[bt + 1]);
        let den = a - 2.0 * b + c;
        let off = if den.abs() > 1e-9 { 0.5 * (a - c) / den } else { 0.0 };
        return Some(sr / (bt as f32 + off.clamp(-1.0, 1.0)));
    }
    None
}

/// Tamaño de los anillos de audio de NN Tune (potencia de 2).
const ANILLO: usize = 1 << 14;
/// Voz principal + hasta dos voces extra.
const VOCES: usize = 3;

/// NN Tune en tiempo real.
/// - Algoritmo «Limpio» (PSOLA): corta la voz ciclo por ciclo en sus picos y la vuelve a armar al
///   ritmo de la nota correcta; la voz conserva su timbre. Tiene ~35 ms de demora.
/// - «Baja demora» y los estilos que cambian el timbre (Ardilla, Hyper, Robot grave): desplazador
///   de dos cabezales, sin demora extra.
pub struct TuneLive {
    sr: f32,
    hist: Vec<f32>,
    hpos: usize,
    acc: f32,
    nacc: u8,
    frame: Vec<f32>,
    d: Vec<f32>,
    dn: Vec<f32>,
    count: usize,
    last_note: Option<i32>,
    /// razón de tono y volumen de cada voz (antes y después de la última detección)
    prev: [f32; VOCES],
    rs: [f32; VOCES],
    pg: [f32; VOCES],
    g: [f32; VOCES],
    periodo: f32,
    pub cantada: f32,
    pub objetivo: i32,
    // PSOLA
    inp: Vec<f32>,
    ola: Vec<f32>,
    n: usize,
    lat: usize,
    tmax: f32,
    marks: std::collections::VecDeque<usize>,
    last_mark: usize,
    ts: [usize; VOCES],
    /// parte decimal acumulada de la posición de cada voz (sin ella las notas agudas salen altas)
    frac: [f32; VOCES],
    // dos cabezales
    delay: Vec<f32>,
    dpos: usize,
    wd: f32,
    wd_obj: f32,
    phase: [f32; VOCES],
    /// nota objetivo de cada voz (en semitonos, suavizada) y su frecuencia antes/después de la detección
    obj: [f32; VOCES],
    obj_ok: [bool; VOCES],
    pf: [f32; VOCES],
    fs: [f32; VOCES],
    /// último tono detectado y cuántas detecciones seguidas se ha sostenido sin poder medir
    ultimo_hz: f32,
    sostener: u8,
    /// tono de la detección anterior y largo de la última ventana (para predecir el tono en «Baja demora»)
    midi_ant: f32,
    largo: usize,
    /// la última marca de análisis cayó en voz (se puede comparar su forma de onda)
    marca_voz: bool,
    /// Retune: nota exacta (en semitonos, con decimales) a la que se lleva la voz ahora mismo
    pub ext: Option<f32>,
}

impl TuneLive {
    const HOP: usize = 256;
    pub fn new(sr: f32) -> TuneLive {
        let n = 1024;
        let wd = 0.035 * sr;
        let tmax = sr / 90.0;
        TuneLive {
            sr,
            hist: vec![0.0; n],
            hpos: 0,
            acc: 0.0,
            nacc: 0,
            frame: vec![0.0; n],
            d: vec![0.0; n / 2 + 2],
            dn: vec![1.0; n / 2 + 2],
            count: 0,
            last_note: None,
            prev: [1.0; VOCES],
            rs: [1.0; VOCES],
            pg: [1.0, 0.0, 0.0],
            g: [1.0, 0.0, 0.0],
            periodo: 0.0,
            cantada: 0.0,
            objetivo: -1,
            inp: vec![0.0; ANILLO],
            ola: vec![0.0; ANILLO],
            n: 0,
            lat: (tmax * 3.0) as usize + 64,
            tmax,
            marks: std::collections::VecDeque::with_capacity(512),
            last_mark: 0,
            ts: [0; VOCES],
            frac: [0.0; VOCES],
            delay: vec![0.0; (0.06 * sr) as usize + 8],
            dpos: 0,
            wd,
            wd_obj: wd,
            phase: [0.0, 0.33, 0.66],
            obj: [0.0; VOCES],
            obj_ok: [false; VOCES],
            pf: [0.0; VOCES],
            fs: [0.0; VOCES],
            ultimo_hz: 0.0,
            sostener: 0,
            midi_ant: 0.0,
            largo: n,
            marca_voz: false,
            ext: None,
        }
    }

    /// Retardo (en muestras) entre lo que entra y lo que sale con estos ajustes.
    pub fn latency(&self, p: &TuneP) -> usize {
        if usa_cabezales(p) { 0 } else { self.lat }
    }

    pub fn reset(&mut self) {
        self.hist.iter_mut().for_each(|v| *v = 0.0);
        self.delay.iter_mut().for_each(|v| *v = 0.0);
        self.inp.iter_mut().for_each(|v| *v = 0.0);
        self.ola.iter_mut().for_each(|v| *v = 0.0);
        self.marks.clear();
        self.last_mark = self.n;
        self.ts = [self.n; VOCES];
        self.prev = [1.0; VOCES];
        self.rs = [1.0; VOCES];
        self.pg = [1.0, 0.0, 0.0];
        self.g = [1.0, 0.0, 0.0];
        self.periodo = 0.0;
        self.last_note = None;
        self.obj_ok = [false; VOCES];
        self.pf = [0.0; VOCES];
        self.fs = [0.0; VOCES];
        self.ultimo_hz = 0.0;
        self.sostener = 0;
    }

    fn detect(&mut self, p: &TuneP) {
        let n = self.hist.len();
        // si ya se conoce el tono se mide con una ventana corta (3 ciclos): responde mucho más rápido
        // y sigue el vibrato; si no se encuentra, se mide con la ventana completa
        let corto = if self.periodo > 0.0 { (((self.periodo / 2.0) * 3.0) as usize * 2).clamp(192, n) } else { n };
        let mut hz = None;
        let mut rms = 0.0;
        for largo in [corto, n] {
            for i in 0..largo {
                self.frame[i] = self.hist[(self.hpos + n - largo + i) % n];
            }
            rms = (self.frame[..largo].iter().map(|v| v * v).sum::<f32>() / largo as f32).sqrt();
            hz = if rms > 0.004 { yin_buf(&self.frame[..largo], self.sr / 2.0, &mut self.d, &mut self.dn) } else { None };
            self.largo = largo;
            if hz.is_some() || largo == n {
                break;
            }
        }
        // si en medio de una nota no se pudo medir (respiración, consonante suave) se sostiene el último tono
        // unos instantes, así no se cuela la voz sin afinar
        match hz {
            Some(h) => {
                self.ultimo_hz = h;
                self.sostener = 0;
            }
            None if rms > 0.004 && self.ultimo_hz > 0.0 && self.sostener < 4 => {
                self.sostener += 1;
                hz = Some(self.ultimo_hz);
            }
            None => self.ultimo_hz = 0.0,
        }
        self.periodo = match hz {
            Some(h) if h > 60.0 && h < 1100.0 => self.sr / h,
            _ => 0.0,
        };
        if let Some(hz) = hz {
            let t = self.sr / hz;
            // ventana de los dos cabezales: corta (pocos ciclos) para que sigan bien el tono
            let base = if usa_cabezales(p) && estilo(p).timbre { 0.03 * self.sr } else { 0.014 * self.sr };
            self.wd_obj = (2.0 * t * (base / (2.0 * t)).round().max(1.0)).clamp(0.008 * self.sr, 0.055 * self.sr);
        }
        self.cantada = hz.map(|h| 69.0 + 12.0 * (h / 440.0).log2()).unwrap_or(0.0);
        self.objetivo = -1;
        let mut r = [1.0f32; VOCES];
        let mut g = [1.0f32, 0.0, 0.0];
        match hz {
            Some(hz) if p.on => {
                let midi = 69.0 + 12.0 * (hz / 440.0).log2();
                // en los cabezales el tono medido llega un poco tarde: se predice hacia dónde va la voz
                let midi_r = if usa_cabezales(p) && self.midi_ant > 0.0 {
                    let adelanto = self.largo as f32 - self.wd * 0.5 + Self::HOP as f32;
                    midi + ((midi - self.midi_ant) / Self::HOP as f32 * adelanto).clamp(-1.0, 1.0)
                } else {
                    midi
                };
                self.midi_ant = midi;
                let best = nota_objetivo(p, midi, self.last_note);
                self.last_note = Some(best);
                self.objetivo = best;
                let (n1, extra) = voces(p, best);
                let hay = extra.iter().any(|e| e.is_some());
                g[0] = if hay { 0.8 } else { 1.0 };
                let mut meta = [None; VOCES];
                meta[0] = Some(n1 as f32);
                for k in 0..2 {
                    if let Some((nota, vol)) = extra[k] {
                        meta[k + 1] = Some(nota as f32);
                        g[k + 1] = vol;
                    }
                }
                // Retune: la nota la decide el editor, sin voces extra ni suavizado
                if let Some(t) = self.ext {
                    meta = [Some(t), None, None];
                    g = [1.0, 0.0, 0.0];
                    self.objetivo = t.round() as i32;
                }
                let alpha = if self.ext.is_some() { 1.0 } else { 1.0 - velocidad(p).clamp(0.0, 1.0) * 0.95 };
                for k in 0..VOCES {
                    match meta[k] {
                        Some(m) => {
                            if self.obj_ok[k] {
                                self.obj[k] += (m - self.obj[k]) * alpha;
                            } else {
                                // al empezar una nota se cae directo en ella
                                self.obj[k] = m;
                                self.obj_ok[k] = true;
                            }
                            r[k] = 2f32.powf((self.obj[k] - midi_r) / 12.0);
                        }
                        None => {
                            self.obj_ok[k] = false;
                            r[k] = self.rs[k];
                        }
                    }
                }
            }
            _ => {
                self.last_note = None;
                self.midi_ant = 0.0;
                self.obj_ok = [false; VOCES];
                r[1] = self.rs[1];
                r[2] = self.rs[2];
            }
        }
        for k in 0..VOCES {
            self.pf[k] = self.fs[k];
            self.fs[k] = if self.obj_ok[k] { 440.0 * 2f32.powf((self.obj[k] - 69.0) / 12.0) } else { 0.0 };
            if self.pf[k] <= 0.0 {
                self.pf[k] = self.fs[k];
            }
            self.prev[k] = self.rs[k];
            self.rs[k] = r[k];
            self.pg[k] = self.g[k];
            // las voces extra entran y salen suave (sin cortes)
            self.g[k] += (g[k] - self.g[k]) * if k == 0 { 1.0 } else { 0.3 };
        }
    }

    /// Agrega un grano (un ciclo de la voz con ventana) centrado en `ta` de la entrada y en `ts` de la salida.
    #[inline]
    fn grano(&mut self, ta: usize, ts: usize, medio: usize, esc: f32) {
        if ta < medio || ts < medio {
            return;
        }
        let largo = 2 * medio;
        let k0 = std::f32::consts::TAU / largo as f32;
        for k in 0..largo {
            let w = 0.5 - 0.5 * (k0 * k as f32).cos();
            let s = self.inp[(ta - medio + k) & (ANILLO - 1)];
            self.ola[(ts - medio + k) & (ANILLO - 1)] += s * w * esc;
        }
    }

    /// Marca de análisis más cercana a `t`.
    fn cercana(&self, t: usize) -> usize {
        let mut best = t;
        let mut bd = usize::MAX;
        for &m in self.marks.iter().rev() {
            let d = m.abs_diff(t);
            if d < bd {
                bd = d;
                best = m;
            } else if m < t {
                break;
            }
        }
        best
    }

    fn alimentar(&mut self, x: f32, p: &TuneP) {
        self.acc += x;
        self.nacc += 1;
        if self.nacc == 2 {
            self.hist[self.hpos] = self.acc * 0.5;
            self.hpos = (self.hpos + 1) % self.hist.len();
            self.acc = 0.0;
            self.nacc = 0;
        }
        self.count += 1;
        if self.count >= Self::HOP {
            self.count = 0;
            self.detect(p);
        }
    }

    /// Solo escucha (detecta el tono) sin cambiar el sonido: para mostrar la nota con el autotune apagado.
    pub fn escuchar(&mut self, x: f32, p: &TuneP) {
        let q = TuneP { on: false, ..*p };
        self.alimentar(x, &q);
    }

    #[inline]
    pub fn process(&mut self, x: f32, p: &TuneP) -> f32 {
        self.alimentar(x, p);
        let t = self.count as f32 / Self::HOP as f32;
        let mut r = [1.0f32; VOCES];
        let mut g = [0.0f32; VOCES];
        let mut fo = [0.0f32; VOCES];
        for k in 0..VOCES {
            r[k] = self.prev[k] + (self.rs[k] - self.prev[k]) * t;
            g[k] = self.pg[k] + (self.g[k] - self.pg[k]) * t;
            fo[k] = self.pf[k] + (self.fs[k] - self.pf[k]) * t;
        }
        let dlen = self.delay.len();
        self.delay[self.dpos] = x;
        self.dpos = (self.dpos + 1) % dlen;
        self.wd += (self.wd_obj - self.wd) * 0.002;
        self.inp[self.n & (ANILLO - 1)] = x;
        self.n += 1;
        let n = self.n;

        if usa_cabezales(p) {
            // desplazador de dos cabezales por voz (sin demora extra; cambia también el timbre)
            let read = |d: f32, buf: &[f32], pos: usize| -> f32 {
                let at = pos as f32 - 1.0 - d + dlen as f32 * 2.0;
                let i = at as usize;
                let f = at.fract();
                let a = buf[i % dlen];
                let b = buf[(i + 1) % dlen];
                a + (b - a) * f
            };
            let mut wet = 0.0;
            for k in 0..VOCES {
                if k > 0 && g[k] < 0.001 {
                    continue;
                }
                self.phase[k] = (self.phase[k] + (1.0 - r[k]) / self.wd).rem_euclid(1.0);
                let q = (self.phase[k] + 0.5).fract();
                let v = read(self.phase[k] * self.wd, &self.delay, self.dpos) * (PI * self.phase[k]).sin().powi(2) + read(q * self.wd, &self.delay, self.dpos) * (PI * q).sin().powi(2);
                wet += v * g[k];
            }
            // las marcas de PSOLA siguen al día por si se cambia de algoritmo
            self.last_mark = n;
            self.ts = [n; VOCES];
            return wet * p.mix + x * (1.0 - p.mix);
        }

        // ---------- marcas de análisis: un pico por ciclo de la voz
        let con_voz = self.periodo > 0.0;
        let paso = if con_voz { self.periodo.min(self.tmax * 1.4) } else { 256.0 };
        if self.last_mark + ANILLO / 4 < n {
            self.last_mark = n.saturating_sub(ANILLO / 8);
        }
        loop {
            let pred = (self.last_mark as f32 + paso) as usize;
            let medio_b = (paso * 0.25) as usize;
            let mitad = (paso * 0.5) as usize;
            if pred + medio_b + mitad >= n {
                break;
            }
            let mut mark = pred;
            if con_voz && self.marca_voz {
                // la marca va donde la forma de onda más se parece a la del ciclo anterior:
                // así cada corte cae en el mismo punto del ciclo (sin temblor ni aspereza)
                let m = ANILLO - 1;
                let paso_j = if mitad > 96 { 2 } else { 1 };
                let mut mejor = f32::MIN;
                for k in pred.saturating_sub(medio_b)..=pred + medio_b {
                    let (mut c, mut e) = (0.0f32, 1e-9f32);
                    let mut j = 0;
                    while j < 2 * mitad {
                        let a = self.inp[(k + j - mitad) & m];
                        let b = self.inp[(self.last_mark + j - mitad) & m];
                        c += a * b;
                        e += a * a;
                        j += paso_j;
                    }
                    let v = c / e.sqrt();
                    if v > mejor {
                        mejor = v;
                        mark = k;
                    }
                }
            } else if con_voz {
                let mut mx = f32::MIN;
                for k in pred.saturating_sub(medio_b)..=pred + medio_b {
                    let v = self.inp[k & (ANILLO - 1)];
                    if v > mx {
                        mx = v;
                        mark = k;
                    }
                }
            }
            self.marca_voz = con_voz;
            let mark = mark.max(self.last_mark + 8);
            if self.marks.len() == self.marks.capacity() {
                self.marks.pop_front();
            }
            self.marks.push_back(mark);
            self.last_mark = mark;
        }

        // ---------- síntesis: granos repartidos al ritmo de cada voz
        let lectura = n.saturating_sub(self.lat);
        let medio = (paso.min(self.tmax)) as usize;
        for k in 0..VOCES {
            let mut ts = self.ts[k];
            let mut frac = self.frac[k];
            let rr = if con_voz { r[k] } else { 1.0 };
            let gain = if con_voz || k == 0 { g[k] } else { 0.0 };
            if ts < lectura + medio {
                ts = lectura + medio;
                frac = 0.0;
            }
            while ts as f32 + 1.5 * paso <= n as f32 {
                if gain > 0.001 {
                    let ta = self.cercana(ts);
                    // al bajar mucho el tono cada pedazo lleva un solo ciclo (si no, se sigue oyendo la nota original)
                    let (m_k, esc) = if con_voz && rr < 0.75 { (((paso * 0.6) as usize).max(16), gain / rr.clamp(0.25, 4.0) * 1.35) } else { (medio, gain / rr.clamp(0.25, 4.0)) };
                    self.grano(ta, ts, m_k, esc);
                }
                let salto = if con_voz && fo[k] > 20.0 { (self.sr / fo[k]).clamp(16.0, paso * 4.0) } else if con_voz { (paso / rr.clamp(0.25, 4.0)).max(16.0) } else { paso };
                let s = salto + frac;
                ts += s as usize;
                frac = s.fract();
            }
            self.ts[k] = ts;
            self.frac[k] = frac;
        }
        let i = lectura & (ANILLO - 1);
        let wet = self.ola[i];
        self.ola[i] = 0.0;
        let dry = self.inp[i];
        wet * p.mix + dry * (1.0 - p.mix)
    }
}

// ============================ Retune: editar la voz nota por nota ============================

/// Una nota detectada en la voz: desde/hasta (en tramos de `RT_HOP` muestras), su tono medio
/// y el tono al que el usuario la quiere llevar (en semitonos, con decimales).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NotaRt {
    pub a: usize,
    pub b: usize,
    pub centro: f32,
    pub objetivo: f32,
}

/// Tramo de análisis del Retune (muestras).
pub const RT_HOP: usize = 256;
/// El tramo `h` del contorno describe la voz alrededor de la muestra `h * RT_HOP + RT_CENTRO`.
pub const RT_CENTRO: usize = 1024;

/// Tono de la voz (en semitonos; 0 = sin tono) cada `RT_HOP` muestras.
pub fn contorno(x: &[f32], sr: f32) -> Vec<f32> {
    let win = 2048usize;
    let frames = x.len() / RT_HOP + 1;
    let xd: Vec<f32> = x.chunks(2).map(|c| c.iter().sum::<f32>() / c.len() as f32).collect();
    let wd2 = win / 2;
    let mut d = vec![0.0f32; wd2 / 2 + 2];
    let mut dn = vec![1.0f32; wd2 / 2 + 2];
    let mut out = vec![0f32; frames];
    for f in 0..frames {
        let s = f * RT_HOP / 2;
        if s + wd2 > xd.len() {
            break;
        }
        let fr = &xd[s..s + wd2];
        let rms = (fr.iter().map(|v| v * v).sum::<f32>() / wd2 as f32).sqrt();
        if rms > 0.006 {
            if let Some(h) = yin_buf(fr, sr / 2.0, &mut d, &mut dn) {
                if h > 60.0 && h < 1100.0 {
                    out[f] = 69.0 + 12.0 * (h / 440.0).log2();
                }
            }
        }
    }
    // se quitan saltos sueltos de octava (un tramo aislado lejos de sus vecinos)
    for f in 1..out.len().saturating_sub(1) {
        let (a, b, c) = (out[f - 1], out[f], out[f + 1]);
        if a > 0.0 && c > 0.0 && b > 0.0 && (a - c).abs() < 1.0 && (b - a).abs() > 6.0 {
            out[f] = (a + c) * 0.5;
        }
    }
    out
}

/// Separa el contorno en notas: tramos con voz donde el tono se mantiene cerca de una nota.
pub fn notas_de(cont: &[f32]) -> Vec<NotaRt> {
    let mut notas = vec![];
    let mut i = 0;
    let n = cont.len();
    while i < n {
        if cont[i] <= 0.0 {
            i += 1;
            continue;
        }
        let a = i;
        let mut suma = cont[i];
        let mut cuenta = 1.0;
        let mut fuera = 0;
        let mut huecos = 0;
        let mut j = i + 1;
        while j < n {
            let c = cont[j];
            if c <= 0.0 {
                huecos += 1;
                if huecos > 3 {
                    break;
                }
                j += 1;
                continue;
            }
            huecos = 0;
            let media = suma / cuenta;
            if (c - media).abs() > 0.75 {
                fuera += 1;
                if fuera >= 3 {
                    j -= 2;
                    break;
                }
            } else {
                fuera = 0;
                suma += c;
                cuenta += 1.0;
            }
            j += 1;
        }
        let b = j.min(n).max(a + 1);
        if b - a >= 6 {
            let mut v: Vec<f32> = cont[a..b].iter().copied().filter(|c| *c > 0.0).collect();
            v.sort_by(|p, q| p.total_cmp(q));
            let centro = v[v.len() / 2];
            notas.push(NotaRt { a, b, centro, objetivo: centro });
        }
        i = b.max(a + 1);
    }
    notas
}

/// Aplica el Retune: cada nota se lleva a su objetivo; `aplanar` (0…1) quita el vaivén del tono
/// dentro de cada nota. Las partes sin cambios quedan tal cual.
pub fn retune(x: &[f32], sr: f32, cont: &[f32], notas: &[NotaRt], aplanar: f32) -> Vec<f32> {
    let mut tl = TuneLive::new(sr);
    let tp_on = TuneP { on: true, key: 0, scale: 0, speed: 0.0, mix: 1.0, gain: 1.0, mask: 0xFFF, estilo: 0, algoritmo: 0, mezcla: 1.0 };
    let tp_off = TuneP { on: false, ..tp_on };
    let lat = tl.latency(&tp_on);
    // la nota que se pide ahora se aplica a la voz que se está armando un poco más atrás
    let retraso = 450usize;
    let objetivo_en = |t: usize, k: &mut usize| -> Option<f32> {
        let h = t.checked_sub(RT_CENTRO)? / RT_HOP;
        while *k < notas.len() && notas[*k].b <= h {
            *k += 1;
        }
        let nt = notas.get(*k)?;
        if h < nt.a {
            return None;
        }
        if (nt.objetivo - nt.centro).abs() < 0.005 && aplanar < 0.005 {
            return None;
        }
        let c = cont.get(h).copied().unwrap_or(0.0);
        Some(if c > 0.0 { nt.objetivo + (c - nt.centro) * (1.0 - aplanar) } else { nt.objetivo })
    };
    let mut k = 0;
    let mut out = Vec::with_capacity(x.len());
    let mut activo = false;
    for i in 0..x.len() + lat {
        if i % 64 == 0 {
            tl.ext = objetivo_en(i.saturating_sub(retraso), &mut k);
            activo = tl.ext.is_some();
        }
        let v = tl.process(x.get(i).copied().unwrap_or(0.0), if activo { &tp_on } else { &tp_off });
        if i >= lat {
            out.push(v);
        }
    }
    out
}

/// Nivelador de la voz: el mismo que se usa al escucharte (sube la voz a un nivel fuerte y parejo,
/// sin subir el ruido cuando no cantas, con un limitador suave). Así lo grabado suena como lo oíste.
pub fn nivelar(x: &[f32], sr: f32) -> Vec<f32> {
    let at_env = 1.0 - (-1.0 / (0.004 * sr)).exp();
    let rel_env = 1.0 - (-1.0 / (0.25 * sr)).exp();
    let at_agc = 1.0 - (-1.0 / (0.01 * sr)).exp();
    let rel_agc = 1.0 - (-1.0 / (0.4 * sr)).exp();
    let (mut env, mut agc) = (0.0f32, 1.0f32);
    x.iter()
        .map(|&v| {
            let a = v.abs();
            env += (a - env) * if a > env { at_env } else { rel_env };
            let deseo = if env > 0.004 { (0.42 / env).clamp(1.0, 8.0) } else { 1.0 };
            agc += (deseo - agc) * if deseo < agc { at_agc } else { rel_agc };
            let y = v * agc;
            let b = y.abs();
            if b < 0.8 { y } else { y.signum() * (0.8 + 0.2 * ((b - 0.8) / 0.2).tanh()) }
        })
        .collect()
}

/// Armonía: la misma voz llevada `k` grados de la escala (2 = tercera, 4 = quinta, -2 = tercera abajo)
/// o, si `semis` es Some, una cantidad fija de semitonos (±12 = octava). Cada nota cae afinada.
pub fn armonia(x: &[f32], sr: f32, mask: u16, k: i32, semis: Option<i32>) -> Vec<f32> {
    let cont = contorno(x, sr);
    let mut notas = notas_de(&cont);
    let tp = TuneP { on: true, key: 0, scale: 0, speed: 0.0, mix: 1.0, gain: 1.0, mask, estilo: 0, algoritmo: 0, mezcla: 1.0 };
    for n in notas.iter_mut() {
        // nota de la escala más cercana a la cantada, y desde ahí se cuentan los grados
        let base = nota_objetivo(&tp, n.centro, None);
        n.objetivo = match semis {
            Some(s) => (base + s) as f32,
            None => grados(&tp, base, k) as f32,
        };
    }
    retune(x, sr, &cont, &notas, 0.6)
}
