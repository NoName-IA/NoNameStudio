//! Síntesis y efectos, muestra a muestra (sin asignar memoria en el hilo de audio).
use crate::fx::{Comp, Eq6, WaveState};
use crate::media::Buf;
use crate::project::{CompP, Inst, SynthP, Band};
use std::sync::Arc;
use std::f32::consts::{PI, TAU};

#[inline]
pub fn mtof(m: f32) -> f32 {
    440.0 * 2f32.powf((m - 69.0) / 12.0)
}

/// Generador de ruido rápido (xorshift).
#[derive(Clone, Copy)]
pub struct Rng(pub u32);
impl Rng {
    #[inline]
    pub fn next(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
    #[inline]
    pub fn unit(&mut self) -> f32 {
        self.next() * 0.5 + 0.5
    }
}

pub fn hash(a: u32, b: u32) -> u32 {
    let mut x = a.wrapping_mul(374_761_393).wrapping_add(b.wrapping_mul(668_265_263));
    x = (x ^ (x >> 13)).wrapping_mul(1_274_126_177);
    x ^ (x >> 16) | 1
}

#[derive(Clone, Copy, PartialEq)]
pub enum FType {
    Low,
    High,
    Band,
    LowShelf,
    HighShelf,
    Peak,
}

/// Biquad (RBJ cookbook), forma directa transpuesta II.
#[derive(Clone, Copy)]
pub struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}
impl Default for Biquad {
    fn default() -> Self {
        Biquad { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0, z1: 0.0, z2: 0.0 }
    }
}
impl Biquad {
    pub fn new(t: FType, f: f32, q: f32, gain_db: f32, sr: f32) -> Self {
        let mut b = Biquad::default();
        b.set(t, f, q, gain_db, sr);
        b
    }
    pub fn set(&mut self, t: FType, f: f32, q: f32, gain_db: f32, sr: f32) {
        let f = f.clamp(10.0, sr * 0.49);
        let w = TAU * f / sr;
        let (s, c) = w.sin_cos();
        let alpha = s / (2.0 * q.max(0.05));
        let a = 10f32.powf(gain_db / 40.0);
        let (b0, b1, b2, a0, a1, a2) = match t {
            FType::Low => ((1.0 - c) / 2.0, 1.0 - c, (1.0 - c) / 2.0, 1.0 + alpha, -2.0 * c, 1.0 - alpha),
            FType::High => ((1.0 + c) / 2.0, -(1.0 + c), (1.0 + c) / 2.0, 1.0 + alpha, -2.0 * c, 1.0 - alpha),
            FType::Band => (alpha, 0.0, -alpha, 1.0 + alpha, -2.0 * c, 1.0 - alpha),
            FType::Peak => (1.0 + alpha * a, -2.0 * c, 1.0 - alpha * a, 1.0 + alpha / a, -2.0 * c, 1.0 - alpha / a),
            FType::LowShelf => {
                let sq = 2.0 * a.sqrt() * alpha;
                (
                    a * ((a + 1.0) - (a - 1.0) * c + sq),
                    2.0 * a * ((a - 1.0) - (a + 1.0) * c),
                    a * ((a + 1.0) - (a - 1.0) * c - sq),
                    (a + 1.0) + (a - 1.0) * c + sq,
                    -2.0 * ((a - 1.0) + (a + 1.0) * c),
                    (a + 1.0) + (a - 1.0) * c - sq,
                )
            }
            FType::HighShelf => {
                let sq = 2.0 * a.sqrt() * alpha;
                (
                    a * ((a + 1.0) + (a - 1.0) * c + sq),
                    -2.0 * a * ((a - 1.0) + (a + 1.0) * c),
                    a * ((a + 1.0) + (a - 1.0) * c - sq),
                    (a + 1.0) - (a - 1.0) * c + sq,
                    2.0 * ((a - 1.0) - (a + 1.0) * c),
                    (a + 1.0) - (a - 1.0) * c - sq,
                )
            }
        };
        self.b0 = b0 / a0;
        self.b1 = b1 / a0;
        self.b2 = b2 / a0;
        self.a1 = a1 / a0;
        self.a2 = a2 / a0;
    }
    /// Magnitud de la respuesta en frecuencia (lineal) en `f` Hz.
    pub fn mag(&self, f: f32, sr: f32) -> f32 {
        let w = TAU * f / sr;
        let (c1, s1) = (w.cos(), -w.sin());
        let (c2, s2) = ((2.0 * w).cos(), -(2.0 * w).sin());
        let nr = self.b0 + self.b1 * c1 + self.b2 * c2;
        let ni = self.b1 * s1 + self.b2 * s2;
        let dr = 1.0 + self.a1 * c1 + self.a2 * c2;
        let di = self.a1 * s1 + self.a2 * s2;
        ((nr * nr + ni * ni) / (dr * dr + di * di).max(1e-12)).sqrt()
    }
    pub fn copy_state(&mut self, o: &Biquad) {
        self.z1 = o.z1;
        self.z2 = o.z2;
    }
    pub fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }
    #[inline]
    pub fn run(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

/// Caída exponencial equivalente a exponentialRamp(pico → 0.0001) en `dec` segundos.
#[inline]
fn exp_dec(t: f32, dec: f32) -> f32 {
    (-9.21 * t / dec.max(1e-4)).exp()
}
/// Seno rápido de una fase en vueltas (1.0 = una vuelta). Error < 0,000004 (inaudible),
/// unas 3 veces más rápido que `sin()`.
#[inline]
pub fn sin_v(p: f32) -> f32 {
    let mut x = p - p.round(); // -0,5…0,5
    if x > 0.25 {
        x = 0.5 - x;
    } else if x < -0.25 {
        x = -0.5 - x;
    }
    let z = x * TAU; // -π/2…π/2
    let z2 = z * z;
    z * (1.0 + z2 * (-1.0 / 6.0 + z2 * (1.0 / 120.0 + z2 * (-1.0 / 5040.0 + z2 * (1.0 / 362_880.0)))))
}
#[inline]
fn tri(ph: f32) -> f32 {
    let p = ph.fract();
    4.0 * (p - 0.5).abs() - 1.0
}

/// Una voz que suena en un canal.
pub struct Voice {
    pub ch: usize,
    pub kind: Inst,
    pub delay: u32,
    t: f32,
    end: f32,
    vel: f32,
    freq: f32,
    k: f32,
    d: f32,
    len: f32,
    ph: [f32; 4],
    f1: Biquad,
    f2: Biquad,
    rng: Rng,
    m: f32,
    wave: Option<WaveState>,
    sample: Option<Arc<Vec<f32>>>,
    spos: f64,
    srate: f64,
    pub done: bool,
    /// nota MIDI (para soltarla con note-off)
    pub note: u8,
    /// nota sostenida por un teclado MIDI hasta el note-off
    pub held: bool,
    smp: Option<Box<SmpState>>,
    /// paneo propio de la voz (-1…1; lo usa el paneo de la pista de la playlist)
    pub pan: f32,
    /// caídas exponenciales que se actualizan multiplicando (valor y factor por muestra)
    ex: [f32; 2],
    exf: [f32; 2],
}

/// Reproducción de un archivo (NN Sampler y clips de audio).
pub struct SmpState {
    pub buf: Arc<Buf>,
    /// posición en cuadros del buffer; `rate` negativo = al revés
    pub pos: f64,
    pub rate: f64,
    pub lo: f64,
    pub hi: f64,
    pub gain: f32,
    pub att: f32,
    pub rel: f32,
    /// fundidos del clip: tiempo del clip al empezar la voz y duración total del clip
    pub fade_in: f32,
    pub fade_out: f32,
    pub t0: f32,
    pub dur: f32,
}

impl Voice {
    /// Caída exponencial `i` (igual a `exp_dec(t, dec)`), avanzada una muestra multiplicando.
    #[inline]
    fn caida(&mut self, i: usize, dec: f32, dt: f32) -> f32 {
        if self.exf[i] == 0.0 {
            self.exf[i] = (-9.21 * dt / dec.max(1e-4)).exp();
        }
        let v = self.ex[i];
        self.ex[i] *= self.exf[i];
        v
    }
    /// `m` es la nota MIDI (ignorada en percusión), `len` la duración de la nota en segundos.
    pub fn new(ch: usize, kind: Inst, delay: u32, vel: f32, m: f32, len: f32, tune: f32, decay: f32, seed: u32, sr: f32) -> Voice {
        let k = 2f32.powf(tune / 12.0);
        let d = decay.max(0.1);
        let rng = Rng(seed | 1);
        let (f1, f2, end) = match kind {
            Inst::Kick => (Biquad::new(FType::Band, 3500.0, 0.7, 0.0, sr), Biquad::default(), 0.48 * d + 0.06),
            Inst::Snare => (Biquad::new(FType::Band, 1900.0 * k, 0.7, 0.0, sr), Biquad::new(FType::High, 400.0, 0.7, 0.0, sr), 0.25 * d + 0.03),
            Inst::Clap => (Biquad::new(FType::Band, 1200.0 * k, 1.1, 0.0, sr), Biquad::default(), 0.06 + 0.22 * d),
            Inst::Rim => (Biquad::new(FType::Band, 2600.0 * k, 3.0, 0.0, sr), Biquad::default(), 0.05 * d + 0.02),
            Inst::Hat => (Biquad::new(FType::High, (7500.0 * k).min(16000.0), 0.7, 0.0, sr), Biquad::default(), 0.045 * d + 0.03),
            Inst::OHat => (Biquad::new(FType::High, (7000.0 * k).min(16000.0), 0.7, 0.0, sr), Biquad::default(), 0.3 * d + 0.04),
            Inst::Rhodes => (Biquad::default(), Biquad::default(), len.max(0.1) + 0.4),
            Inst::Pad => (Biquad::new(FType::Low, 500.0, 0.5, 0.0, sr), Biquad::default(), len.max(0.2) + 0.75),
            Inst::Bass => (Biquad::new(FType::Low, 320.0, 0.7, 0.0, sr), Biquad::default(), len.max(0.08)),
            Inst::Bell => (Biquad::default(), Biquad::default(), 1.2),
            Inst::Synth | Inst::Vocal | Inst::Sampler | Inst::Audio | Inst::Plugin => (Biquad::default(), Biquad::default(), len.max(0.05)),
        };
        let note = m.round().clamp(0.0, 127.0) as u8;
        Voice { ch, kind, delay, t: 0.0, end, vel, freq: mtof(m), k, d, len, ph: [0.0; 4], f1, f2, rng, m, wave: None, sample: None, spos: 0.0, srate: 1.0, done: false, note, held: false, smp: None, pan: 0.0, ex: [1.0; 2], exf: [0.0; 2] }
    }

    /// Voz que reproduce un archivo. `len` = duración de la nota (s); `max_t` = tope (s).
    pub fn file(ch: usize, kind: Inst, delay: u32, vel: f32, st: SmpState, len: f32, max_t: f32, sr: f32) -> Voice {
        let mut v = Voice::new(ch, kind, delay, vel, 60.0, len, 0.0, 1.0, 1, sr);
        let region = ((st.hi - st.lo) / st.rate.abs().max(1e-9)) as f32 / sr;
        v.len = len;
        v.end = region.min(len + st.rel.max(0.003)).min(max_t).max(0.002);
        v.smp = Some(Box::new(st));
        v
    }

    /// Suelta una nota sostenida (note-off): empieza la fase de relajación.
    pub fn release(&mut self) {
        let t = self.t;
        self.held = false;
        match self.kind {
            Inst::Rhodes => {
                self.len = t + 0.05;
                self.end = self.end.min(self.len + 0.4);
            }
            Inst::Pad => {
                self.len = t;
                self.end = self.end.min(t + 0.75);
            }
            Inst::Bass => {
                self.len = (t + 0.06).max(0.08);
                self.end = self.end.min(self.len);
            }
            Inst::Synth => {
                self.len = t;
                if let Some(w) = &self.wave {
                    self.end = self.end.min(w.end_time(t));
                }
            }
            _ => {
                let r = self.smp.as_ref().map(|s| s.rel.max(0.004)).unwrap_or(0.01);
                self.len = self.len.min(t);
                self.end = self.end.min(t + r);
            }
        }
    }

    /// Corta la voz con un fundido muy corto (fin de bucle, parada).
    pub fn kill(&mut self) {
        if let Some(s) = &mut self.smp {
            s.rel = 0.006;
            self.len = self.len.min(self.t);
            self.end = self.end.min(self.t + 0.006);
        } else {
            self.release();
        }
    }

    pub fn is_file(&self) -> bool {
        self.smp.is_some()
    }

    /// Siguiente muestra estéreo.
    #[inline]
    pub fn next2(&mut self, sr: f32) -> (f32, f32) {
        if self.smp.is_none() {
            let m = self.next(sr);
            return (m, m);
        }
        if self.delay > 0 {
            self.delay -= 1;
            return (0.0, 0.0);
        }
        let t = self.t;
        self.t += 1.0 / sr;
        if t >= self.end {
            self.done = true;
            return (0.0, 0.0);
        }
        let len = self.len;
        let end = self.end;
        let vel = self.vel;
        let s = self.smp.as_mut().unwrap();
        if s.pos < s.lo || s.pos >= s.hi {
            self.done = true;
            return (0.0, 0.0);
        }
        let (l, r) = s.buf.lerp(s.pos);
        s.pos += s.rate;
        // envolvente: ataque, relajación tras la nota y fundido anti-clic al final
        let mut a = if s.att > 0.0 && t < s.att { t / s.att } else { 1.0 };
        if t > len {
            a *= (1.0 - (t - len) / s.rel.max(0.003)).max(0.0);
        }
        a *= ((end - t) / 0.003).clamp(0.0, 1.0);
        if s.dur > 0.0 {
            let ct = s.t0 + t;
            if s.fade_in > 0.0 {
                a *= (ct / s.fade_in).min(1.0);
            } else if s.t0 > 0.0 {
                a *= (t / 0.003).min(1.0);
            }
            if s.fade_out > 0.0 {
                a *= ((s.dur - ct) / s.fade_out).clamp(0.0, 1.0);
            }
        }
        let g = a * s.gain * vel;
        (l * g, r * g)
    }

    /// Voz de NN Wave.
    /// `desde`: nota anterior del canal (para el glide), si la hay.
    pub fn synth(ch: usize, delay: u32, vel: f32, m: f32, len: f32, p: SynthP, seed: u32, sr: f32, desde: Option<f32>) -> Voice {
        let mut v = Voice::new(ch, Inst::Synth, delay, vel, m, len, 0.0, 1.0, seed, sr);
        let mut w = WaveState::new(p, seed);
        if p.glide > 0.0 {
            w.desde = desde.or(if p.glide > 0.0 && p.osc[1].on == false && p.amp[2] == 0.0 { Some(m + 12.0) } else { None });
        }
        v.end = w.end_time(len.max(0.02));
        v.wave = Some(w);
        v
    }

    /// Reproducción de una toma de audio (voz), con su frecuencia de muestreo original.
    pub fn sample(ch: usize, delay: u32, data: Arc<Vec<f32>>, data_sr: f32, max_len: f32, sr: f32) -> Voice {
        let mut v = Voice::new(ch, Inst::Vocal, delay, 1.0, 60.0, max_len, 0.0, 1.0, 1, sr);
        v.end = (data.len() as f32 / data_sr).min(max_len.max(0.05));
        v.srate = data_sr as f64 / sr as f64;
        v.sample = Some(data);
        v
    }

    #[inline]
    pub fn next(&mut self, sr: f32) -> f32 {
        if self.delay > 0 {
            self.delay -= 1;
            return 0.0;
        }
        let dt = 1.0 / sr;
        let t = self.t;
        self.t += dt;
        if t >= self.end {
            self.done = true;
            return 0.0;
        }
        let v = self.vel;
        let (k, d) = (self.k, self.d);
        match self.kind {
            Inst::Kick => {
                let f = 48.0 * k + (130.0 - 48.0) * k * self.caida(0, 0.03 * 9.21, dt);
                self.ph[0] = (self.ph[0] + f * dt).fract();
                let env = if t < 0.004 { t / 0.004 } else { self.caida(1, 0.48 * d, dt) };
                let body = sin_v(self.ph[0]) * env * v;
                let click = if t < 0.016 { self.f1.run(self.rng.next()) * v * 0.35 * exp_dec(t, 0.015) } else { 0.0 };
                body + click
            }
            Inst::Snare => {
                let e_ruido = self.caida(0, 0.22 * d, dt);
                let e_tono = self.caida(1, 0.12 * d, dt);
                let n = self.f2.run(self.f1.run(self.rng.next())) * v * 0.8 * e_ruido;
                let f = 160.0 * k + 40.0 * k * (-t / 0.04).exp();
                self.ph[0] = (self.ph[0] + f * dt).fract();
                n * 1.6 + tri(self.ph[0]) * v * 0.5 * e_tono
            }
            Inst::Clap => {
                let env = if t < 0.033 {
                    let local = t % 0.011;
                    0.7 * (0.12f32 / 0.7).powf(local / 0.009).max(0.12)
                } else {
                    0.6 * exp_dec(t - 0.033, 0.22 * d)
                };
                self.f1.run(self.rng.next()) * v * env * 2.2
            }
            Inst::Rim => {
                self.ph[0] = (self.ph[0] + 820.0 * k * dt).fract();
                tri(self.ph[0]) * v * 0.5 * exp_dec(t, 0.04 * d) + self.f1.run(self.rng.next()) * v * 0.9 * exp_dec(t, 0.03 * d)
            }
            Inst::Hat => {
                let e = self.caida(0, 0.045 * d, dt);
                self.f1.run(self.rng.next()) * v * 0.55 * e
            }
            Inst::OHat => {
                let e = self.caida(0, 0.3 * d, dt);
                self.f1.run(self.rng.next()) * v * 0.50 * e
            }
            Inst::Rhodes => {
                // FM: portadora y moduladora 1:1, índice que cae de 1.6 a 0.15
                if self.exf[0] == 0.0 {
                    self.exf = [(-dt / 0.25).exp(), (-dt / 0.35).exp()];
                }
                let a = v * 0.16;
                let idx = 0.15 + 1.45 * self.ex[0];
                self.ex[0] *= self.exf[0];
                self.ph[1] = (self.ph[1] + self.freq * dt).fract();
                self.ph[0] = (self.ph[0] + self.freq * dt).fract();
                let s = sin_v(self.ph[0] + idx * sin_v(self.ph[1]) * (1.0 / TAU));
                let hold_end = (self.len - 0.05).max(0.1);
                let env = if t < 0.008 {
                    t / 0.008
                } else if t < hold_end {
                    let e = 0.35 + 0.65 * self.ex[1];
                    self.ex[1] *= self.exf[1];
                    e
                } else {
                    let at = 0.35 + 0.65 * (-(hold_end - 0.008) / 0.35).exp();
                    at * exp_dec(t - hold_end, 0.35)
                };
                self.ph[2] = (self.ph[2] + 4.2 * dt).fract();
                let trem = 0.82 + 0.18 * sin_v(self.ph[2]);
                s * env * a * trem
            }
            Inst::Pad => {
                let a = v * 0.05;
                let mut s = 0.0;
                for (i, det) in [-8.0f32, 0.0, 7.0].iter().enumerate() {
                    let f = self.freq * 2f32.powf(det / 1200.0);
                    self.ph[i] = (self.ph[i] + f * dt).fract();
                    s += self.ph[i] * 2.0 - 1.0;
                }
                if (self.t * sr) as u32 % 64 == 0 {
                    let fc = 500.0 + 1000.0 * (t / 0.6).min(1.0);
                    self.f1.set(FType::Low, fc, 0.5, 0.0, sr);
                }
                let s = self.f1.run(s);
                let len = self.len.max(0.2);
                let env = if t < 0.35 { t / 0.35 } else if t < len { 1.0 } else { exp_dec(t - len, 0.7) };
                s * env * a
            }
            Inst::Bass => {
                let a = v * 0.40;
                self.ph[0] = (self.ph[0] + self.freq * dt).fract();
                self.ph[1] = (self.ph[1] + self.freq * 2.0 * dt).fract();
                let s = sin_v(self.ph[0]) + tri(self.ph[1]) * 0.12;
                let len = self.len.max(0.08);
                let env = if t < 0.015 { t / 0.015 } else if t < len - 0.06 { 1.0 - 0.2 * ((t - 0.015) / (len - 0.075).max(0.01)) } else { 0.8 * exp_dec(t - (len - 0.06), 0.06) };
                self.f1.run(s) * env * a
            }
            Inst::Synth => match &mut self.wave {
                Some(w) => w.next(t, self.len, self.m, v, sr),
                None => 0.0,
            },
            Inst::Vocal => {
                let Some(d) = &self.sample else { return 0.0 };
                let i = self.spos as usize;
                self.spos += self.srate;
                if i + 1 >= d.len() {
                    self.done = true;
                    return 0.0;
                }
                let fr = (self.spos - self.srate).fract() as f32;
                // fundido corto al final de la toma para evitar clics
                let fade = ((self.end - t) / 0.01).clamp(0.0, 1.0);
                (d[i] + (d[i + 1] - d[i]) * fr) * fade
            }
            Inst::Sampler | Inst::Audio | Inst::Plugin => 0.0,
            Inst::Bell => {
                self.ph[0] = (self.ph[0] + self.freq * dt).fract();
                self.ph[1] = (self.ph[1] + self.freq * 3.01 * dt).fract();
                let s = sin_v(self.ph[0]) + 0.15 * sin_v(self.ph[1]);
                let env = if t < 0.005 { t / 0.005 } else { self.caida(0, 1.1, dt) };
                s * env * v * 0.21
            }
        }
    }
}

/// Banda de canal: EQ rápida, filtro, NN EQ, compresor, paneo y fader.
pub struct Strip {
    pub id: u32,
    lo: [Biquad; 2],
    hi: [Biquad; 2],
    cut: [Biquad; 2],
    params: (f32, f32, f32),
    pub peak: f32,
    pub eq: Eq6,
    pub comp: Comp,
}
impl Strip {
    pub fn new(id: u32) -> Strip {
        Strip { id, lo: [Biquad::default(); 2], hi: [Biquad::default(); 2], cut: [Biquad::default(); 2], params: (f32::NAN, f32::NAN, f32::NAN), peak: 0.0, eq: Eq6::default(), comp: Comp::default() }
    }
    pub fn update_fx(&mut self, eq: &[Band; 6], sr: f32) {
        self.eq.set(eq, sr);
    }
    /// NN EQ + NN Dynamics. `key` = nivel de la fuente de sidechain (None = la propia señal).
    #[inline]
    pub fn run_fx(&mut self, l: f32, r: f32, comp: &CompP, key: Option<f32>, sr: f32) -> (f32, f32) {
        let (l, r) = self.eq.stereo(l, r);
        if comp.on {
            let g = self.comp.gain(key.unwrap_or(l.abs().max(r.abs())), comp, sr);
            (l * g, r * g)
        } else {
            (l, r)
        }
    }
    pub fn update(&mut self, lo: f32, hi: f32, cut: f32, sr: f32) {
        if self.params != (lo, hi, cut) {
            self.params = (lo, hi, cut);
            for k in 0..2 {
                self.lo[k].set(FType::LowShelf, 200.0, 0.707, lo, sr);
                self.hi[k].set(FType::HighShelf, 4000.0, 0.707, hi, sr);
                self.cut[k].set(FType::Low, cut.min(sr * 0.45), 0.8, 0.0, sr);
            }
        }
    }
    #[inline]
    pub fn run(&mut self, l: f32, r: f32, cut_open: bool) -> (f32, f32) {
        let (mut l, mut r) = (l, r);
        if self.params.0 != 0.0 {
            l = self.lo[0].run(l);
            r = self.lo[1].run(r);
        }
        if self.params.1 != 0.0 {
            l = self.hi[0].run(l);
            r = self.hi[1].run(r);
        }
        if !cut_open {
            l = self.cut[0].run(l);
            r = self.cut[1].run(r);
        }
        (l, r)
    }
}

/// Reverb tipo Freeverb (8 peines + 4 pasa-todo por lado).
pub struct Reverb {
    combs: Vec<(Vec<f32>, usize, f32)>,
    alls: Vec<(Vec<f32>, usize)>,
}
impl Reverb {
    pub fn new(sr: f32) -> Reverb {
        let sc = sr / 44100.0;
        let cl = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
        let al = [556, 441, 341, 225];
        let mut combs = vec![];
        let mut alls = vec![];
        for side in 0..2 {
            for &l in &cl {
                combs.push((vec![0.0; ((l + side * 23) as f32 * sc) as usize], 0, 0.0));
            }
            for &l in &al {
                alls.push((vec![0.0; ((l + side * 23) as f32 * sc) as usize], 0));
            }
        }
        Reverb { combs, alls }
    }
    #[inline]
    pub fn run(&mut self, l: f32, r: f32) -> (f32, f32) {
        let (fb, damp) = (0.84, 0.25);
        let mut out = [0.0f32; 2];
        for side in 0..2 {
            let inp = if side == 0 { l } else { r } * 0.015;
            let mut acc = 0.0;
            for c in &mut self.combs[side * 8..side * 8 + 8] {
                let y = c.0[c.1];
                c.2 = y * (1.0 - damp) + c.2 * damp;
                c.0[c.1] = inp + c.2 * fb;
                c.1 = (c.1 + 1) % c.0.len();
                acc += y;
            }
            for a in &mut self.alls[side * 4..side * 4 + 4] {
                let b = a.0[a.1];
                a.0[a.1] = acc + b * 0.5;
                a.1 = (a.1 + 1) % a.0.len();
                acc = b - acc;
            }
            out[side] = acc;
        }
        (out[0], out[1])
    }
}

/// Delay estéreo con realimentación filtrada (sonido de cinta).
pub struct Delay {
    buf: Vec<[f32; 2]>,
    pos: usize,
    lp: [Biquad; 2],
}
impl Delay {
    pub fn new(sr: f32) -> Delay {
        Delay { buf: vec![[0.0; 2]; (sr * 3.0) as usize], pos: 0, lp: [Biquad::new(FType::Low, 2800.0, 0.7, 0.0, sr); 2] }
    }
    #[inline]
    pub fn run(&mut self, l: f32, r: f32, samples: usize) -> (f32, f32) {
        let n = self.buf.len();
        let d = samples.clamp(1, n - 1);
        let rd = self.buf[(self.pos + n - d) % n];
        let yl = self.lp[0].run(rd[0]);
        let yr = self.lp[1].run(rd[1]);
        self.buf[self.pos] = [l + yr * 0.38, r + yl * 0.38];
        self.pos = (self.pos + 1) % n;
        (yl, yr)
    }
}

/// Compresor de bus + limitador de picos.
pub struct Dynamics {
    env: f32,
    lim: f32,
    atk: f32,
    rel: f32,
    latk: f32,
    lrel: f32,
}
impl Dynamics {
    pub fn new(sr: f32) -> Dynamics {
        let c = |s: f32| (-1.0 / (s * sr)).exp();
        Dynamics { env: 0.0, lim: 1.0, atk: c(0.01), rel: c(0.2), latk: c(0.0008), lrel: c(0.08) }
    }
    #[inline]
    pub fn run(&mut self, l: f32, r: f32, master: f32) -> (f32, f32) {
        // compresor: umbral -14 dB, ratio 4:1
        let x = l.abs().max(r.abs());
        let c = if x > self.env { self.atk } else { self.rel };
        self.env = x + c * (self.env - x);
        let db = 20.0 * self.env.max(1e-6).log10();
        let over = (db + 14.0).max(0.0);
        let g = 10f32.powf(-over * 0.75 / 20.0) * 1.35;
        let (l, r) = (l * g * master, r * g * master);
        // limitador: techo -1 dBFS
        let ceil = 0.891;
        let pk = l.abs().max(r.abs());
        let target = if pk > ceil { ceil / pk } else { 1.0 };
        let cc = if target < self.lim { self.latk } else { self.lrel };
        self.lim = target + cc * (self.lim - target);
        let g = self.lim.min(target);
        ((l * g).clamp(-1.0, 1.0), (r * g).clamp(-1.0, 1.0))
    }
}

/// Ruido de vinilo (siseo + crujidos).
pub struct Vinyl {
    rng: Rng,
    bp: Biquad,
}
impl Vinyl {
    pub fn new(sr: f32) -> Vinyl {
        Vinyl { rng: Rng(0x1234_5677), bp: Biquad::new(FType::Band, 2500.0, 0.5, 0.0, sr) }
    }
    #[inline]
    pub fn run(&mut self) -> f32 {
        let mut x = self.rng.next() * 0.012;
        if self.rng.unit() < 0.00035 {
            x += self.rng.next() * 0.5;
        }
        self.bp.run(x)
    }
}

/// Paneo de potencia constante.
#[inline]
pub fn pan_gains(p: f32) -> (f32, f32) {
    let a = (p.clamp(-1.0, 1.0) + 1.0) * PI / 4.0;
    (a.cos(), a.sin())
}
