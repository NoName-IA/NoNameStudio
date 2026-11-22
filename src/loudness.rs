//! Medidor de sonoridad LUFS (ITU-R BS.1770 / EBU R128): momentáneo, corto plazo e integrado.
use crate::dsp::{Biquad, FType};

pub struct Lufs {
    pre: [Biquad; 2],
    hp: [Biquad; 2],
    block: usize,
    acc: f64,
    n: usize,
    /// energía de los últimos bloques de 100 ms
    hist: Vec<f64>,
    pos: usize,
    filled: usize,
    /// bloques de 400 ms (con solapamiento) para el integrado
    gated: Vec<f64>,
    pub momentary: f32,
    pub short: f32,
    pub true_peak: f32,
}

fn lufs(ms: f64) -> f32 {
    if ms <= 1e-12 { -70.0 } else { (-0.691 + 10.0 * ms.log10()) as f32 }
}

impl Lufs {
    pub fn new(sr: f32) -> Lufs {
        // filtro K: estante agudo +4 dB en 1681 Hz y pasa-altos 38 Hz
        let pre = Biquad::new(FType::HighShelf, 1681.0, 0.7071, 4.0, sr);
        let hp = Biquad::new(FType::High, 38.0, 0.5, 0.0, sr);
        Lufs { pre: [pre; 2], hp: [hp; 2], block: (sr * 0.1) as usize, acc: 0.0, n: 0, hist: vec![0.0; 30], pos: 0, filled: 0, gated: Vec::with_capacity(4096), momentary: -70.0, short: -70.0, true_peak: 0.0 }
    }
    pub fn reset(&mut self) {
        self.acc = 0.0;
        self.n = 0;
        self.hist.iter_mut().for_each(|x| *x = 0.0);
        self.filled = 0;
        self.gated.clear();
        self.momentary = -70.0;
        self.short = -70.0;
        self.true_peak = 0.0;
    }
    #[inline]
    pub fn push(&mut self, l: f32, r: f32) {
        let a = self.hp[0].run(self.pre[0].run(l)) as f64;
        let b = self.hp[1].run(self.pre[1].run(r)) as f64;
        self.acc += a * a + b * b;
        self.true_peak = self.true_peak.max(l.abs()).max(r.abs());
        self.n += 1;
        if self.n >= self.block {
            let ms = self.acc / self.n as f64;
            self.acc = 0.0;
            self.n = 0;
            self.hist[self.pos] = ms;
            self.pos = (self.pos + 1) % self.hist.len();
            self.filled = (self.filled + 1).min(self.hist.len());
            let last = |k: usize| -> f64 {
                let k = k.min(self.filled).max(1);
                (0..k).map(|i| self.hist[(self.pos + self.hist.len() - 1 - i) % self.hist.len()]).sum::<f64>() / k as f64
            };
            let m = last(4);
            self.momentary = lufs(m);
            self.short = lufs(last(30));
            if self.filled >= 4 && self.gated.len() < 200_000 {
                self.gated.push(m);
            }
        }
    }
    /// Sonoridad integrada con las dos compuertas (-70 LUFS absoluta, -10 LU relativa).
    pub fn integrated(&self) -> f32 {
        integrate(&self.gated)
    }
}

pub fn integrate(blocks: &[f64]) -> f32 {
    let abs: Vec<f64> = blocks.iter().copied().filter(|&m| lufs(m) > -70.0).collect();
    if abs.is_empty() {
        return -70.0;
    }
    let mean = abs.iter().sum::<f64>() / abs.len() as f64;
    let rel = lufs(mean) - 10.0;
    let g: Vec<f64> = abs.into_iter().filter(|&m| lufs(m) > rel).collect();
    if g.is_empty() {
        return -70.0;
    }
    lufs(g.iter().sum::<f64>() / g.len() as f64)
}

/// Mide un audio estéreo intercalado completo. Devuelve (integrado LUFS, pico dBFS).
pub fn measure(audio: &[f32], sr: f32) -> (f32, f32) {
    let mut m = Lufs::new(sr);
    for f in audio.chunks(2) {
        m.push(f[0], f.get(1).copied().unwrap_or(f[0]));
    }
    (m.integrated(), 20.0 * m.true_peak.max(1e-9).log10())
}
