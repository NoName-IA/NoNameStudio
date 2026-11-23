//! Efectos NN: Chorus, Flanger, Phaser, Tremolo, Gate, Bitcrusher, Waveshaper, Tape Stop,
//! Tape Delay, Trance Delay, compresor multibanda, Spreader y Pitcher.
//! Cada efecto tiene 4 perillas (0…1) y una mezcla (seco/procesado).
use serde::{Deserialize, Serialize};
use std::f32::consts::{PI, TAU};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TipoEfecto {
    Chorus,
    Flanger,
    Phaser,
    Tremolo,
    Gate,
    Bitcrusher,
    Waveshaper,
    TapeStop,
    TapeDelay,
    TranceDelay,
    Multibanda,
    Spreader,
    Pitcher,
}

/// Un efecto de la cadena de un canal (o del master).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct EfectoP {
    pub tipo: TipoEfecto,
    pub on: bool,
    pub p: [f32; 4],
    pub mezcla: f32,
}

impl TipoEfecto {
    pub const TODOS: [TipoEfecto; 13] = [
        TipoEfecto::Chorus,
        TipoEfecto::Flanger,
        TipoEfecto::Phaser,
        TipoEfecto::Tremolo,
        TipoEfecto::Gate,
        TipoEfecto::Bitcrusher,
        TipoEfecto::Waveshaper,
        TipoEfecto::TapeStop,
        TipoEfecto::TapeDelay,
        TipoEfecto::TranceDelay,
        TipoEfecto::Multibanda,
        TipoEfecto::Spreader,
        TipoEfecto::Pitcher,
    ];
    pub fn nombre(self) -> &'static str {
        match self {
            TipoEfecto::Chorus => "Chorus",
            TipoEfecto::Flanger => "Flanger",
            TipoEfecto::Phaser => "Phaser",
            TipoEfecto::Tremolo => "Tremolo",
            TipoEfecto::Gate => "Gate",
            TipoEfecto::Bitcrusher => "Bitcrusher",
            TipoEfecto::Waveshaper => "Waveshaper",
            TipoEfecto::TapeStop => "Tape Stop",
            TipoEfecto::TapeDelay => "Tape Delay",
            TipoEfecto::TranceDelay => "Trance Delay",
            TipoEfecto::Multibanda => "Compresor multibanda",
            TipoEfecto::Spreader => "Spreader (estéreo)",
            TipoEfecto::Pitcher => "Pitcher (tono)",
        }
    }
    pub fn categoria(self) -> &'static str {
        match self {
            TipoEfecto::Chorus | TipoEfecto::Flanger | TipoEfecto::Phaser | TipoEfecto::Tremolo => "Modulación",
            TipoEfecto::TapeDelay | TipoEfecto::TranceDelay | TipoEfecto::TapeStop => "Eco y cinta",
            TipoEfecto::Bitcrusher | TipoEfecto::Waveshaper => "Distorsión",
            TipoEfecto::Gate | TipoEfecto::Multibanda => "Dinámica",
            TipoEfecto::Spreader | TipoEfecto::Pitcher => "Estéreo y tono",
        }
    }
    pub fn ayuda(self) -> &'static str {
        match self {
            TipoEfecto::Chorus => "Engrosa el sonido como si sonaran varias copias a la vez",
            TipoEfecto::Flanger => "Barrido metálico tipo «avión»",
            TipoEfecto::Phaser => "Barrido suave que gira, clásico en guitarras y pads",
            TipoEfecto::Tremolo => "El volumen sube y baja al ritmo",
            TipoEfecto::Gate => "Corta el sonido cuando baja de un nivel (quita ruido o crea cortes)",
            TipoEfecto::Bitcrusher => "Sonido de videojuego viejo: menos bits y menos resolución",
            TipoEfecto::Waveshaper => "Distorsión creativa con varias formas",
            TipoEfecto::TapeStop => "La cinta se frena hasta detenerse (y vuelve a arrancar)",
            TipoEfecto::TapeDelay => "Eco de cinta: cálido, oscuro y con vaivén",
            TipoEfecto::TranceDelay => "Eco al ritmo que rebota de izquierda a derecha",
            TipoEfecto::Multibanda => "Comprime graves, medios y agudos por separado",
            TipoEfecto::Spreader => "Ensancha el estéreo (y deja los graves al centro)",
            TipoEfecto::Pitcher => "Cambia el tono sin cambiar la velocidad",
        }
    }
    /// Nombres de las 4 perillas.
    pub fn perillas(self) -> [&'static str; 4] {
        match self {
            TipoEfecto::Chorus => ["Velocidad", "Profundidad", "Realimentación", "Ancho"],
            TipoEfecto::Flanger => ["Velocidad", "Profundidad", "Realimentación", "Retardo"],
            TipoEfecto::Phaser => ["Velocidad", "Profundidad", "Realimentación", "Centro"],
            TipoEfecto::Tremolo => ["Velocidad", "Profundidad", "Forma", "Paneo"],
            TipoEfecto::Gate => ["Umbral", "Cierre", "Rango", "Apertura"],
            TipoEfecto::Bitcrusher => ["Bits", "Muestreo", "Ganancia", "Suavizado"],
            TipoEfecto::Waveshaper => ["Saturación", "Forma", "Tono", "Salida"],
            TipoEfecto::TapeStop => ["Frenar", "Tiempo", "Arranque", "Curva"],
            TipoEfecto::TapeDelay => ["Tiempo", "Realimentación", "Vaivén", "Oscuridad"],
            TipoEfecto::TranceDelay => ["Ritmo", "Realimentación", "Filtro", "Rebote"],
            TipoEfecto::Multibanda => ["Graves", "Medios", "Agudos", "Intensidad"],
            TipoEfecto::Spreader => ["Ancho", "Separación", "Graves al centro", "Salida"],
            TipoEfecto::Pitcher => ["Semitonos", "Ajuste fino", "Suavidad", "Salida"],
        }
    }
    /// Valores iniciales razonables.
    pub fn por_defecto(self) -> EfectoP {
        let (p, mezcla) = match self {
            TipoEfecto::Chorus => ([0.25, 0.5, 0.2, 0.8], 0.5),
            TipoEfecto::Flanger => ([0.15, 0.6, 0.6, 0.3], 0.5),
            TipoEfecto::Phaser => ([0.2, 0.7, 0.5, 0.5], 0.5),
            TipoEfecto::Tremolo => ([0.5, 0.6, 0.0, 0.0], 1.0),
            TipoEfecto::Gate => ([0.4, 0.3, 1.0, 0.1], 1.0),
            TipoEfecto::Bitcrusher => ([0.4, 0.3, 0.5, 0.0], 0.7),
            TipoEfecto::Waveshaper => ([0.4, 0.0, 0.6, 0.6], 0.8),
            TipoEfecto::TapeStop => ([0.0, 0.4, 0.3, 0.5], 1.0),
            TipoEfecto::TapeDelay => ([0.4, 0.45, 0.3, 0.5], 0.35),
            TipoEfecto::TranceDelay => ([0.5, 0.4, 0.5, 1.0], 0.3),
            TipoEfecto::Multibanda => ([0.5, 0.5, 0.5, 0.4], 1.0),
            TipoEfecto::Spreader => ([0.7, 0.3, 1.0, 0.5], 1.0),
            TipoEfecto::Pitcher => ([0.5, 0.5, 0.5, 0.5], 1.0),
        };
        EfectoP { tipo: self, on: true, p, mezcla }
    }
    /// Texto del valor de una perilla (para mostrar al usuario).
    pub fn valor(self, k: usize, v: f32, bpm: f32) -> String {
        let _ = bpm;
        match (self, k) {
            (TipoEfecto::Chorus | TipoEfecto::Flanger | TipoEfecto::Phaser, 0) => format!("{:.2} Hz", vel_lfo(v)),
            (TipoEfecto::Tremolo, 0) => DIVISIONES[div_idx(v)].0.to_string(),
            (TipoEfecto::Tremolo, 2) => if v < 0.5 { "Suave".into() } else { "Cuadrada".into() },
            (TipoEfecto::Gate, 0) => format!("{:.0} dB", -60.0 + v * 60.0),
            (TipoEfecto::Bitcrusher, 0) => format!("{} bits", bits(v)),
            (TipoEfecto::Bitcrusher, 1) => format!("÷{}", reduccion(v)),
            (TipoEfecto::Waveshaper, 1) => FORMAS[forma(v)].to_string(),
            (TipoEfecto::TapeStop, 0) => if v >= 0.5 { "Frenado".into() } else { "Sonando".into() },
            (TipoEfecto::TapeStop, 1) | (TipoEfecto::TapeStop, 2) => format!("{:.2} s", 0.1 + v * 2.9),
            (TipoEfecto::TapeDelay, 0) => format!("{:.0} ms", 40.0 + v * 960.0),
            (TipoEfecto::TranceDelay, 0) => DIVISIONES[div_idx(v)].0.to_string(),
            (TipoEfecto::Multibanda, 0..=2) => format!("{:+.1} dB", (v - 0.5) * 24.0),
            (TipoEfecto::Spreader, 0) => format!("{:.0} %", v * 200.0),
            (TipoEfecto::Spreader, 1) => format!("{:.0} ms", v * 20.0),
            (TipoEfecto::Spreader, 2) => if v >= 0.5 { "Sí".into() } else { "No".into() },
            (TipoEfecto::Pitcher, 0) => format!("{:+} st", semitonos(v)),
            (TipoEfecto::Pitcher, 1) => format!("{:+.0} ¢", (v - 0.5) * 100.0),
            (_, 3) if matches!(self, TipoEfecto::Waveshaper | TipoEfecto::Spreader | TipoEfecto::Pitcher) => format!("{:+.1} dB", (v - 0.5) * 24.0),
            _ => format!("{:.0} %", v * 100.0),
        }
    }
}

const FORMAS: [&str; 4] = ["Suave", "Dura", "Plegada", "Rectificada"];
/// Divisiones de tiempo: (nombre, pasos de semicorchea)
const DIVISIONES: [(&str, f32); 7] = [("1/32", 0.5), ("1/16", 1.0), ("1/8 tresillo", 4.0 / 3.0), ("1/8", 2.0), ("3/16", 3.0), ("1/4", 4.0), ("1/2", 8.0)];

fn vel_lfo(v: f32) -> f32 {
    0.05 * 200f32.powf(v) // 0,05 … 10 Hz
}
fn div_idx(v: f32) -> usize {
    ((v * (DIVISIONES.len() as f32 - 0.001)) as usize).min(DIVISIONES.len() - 1)
}
fn bits(v: f32) -> u32 {
    (16.0 - v * 13.0).round() as u32 // 16 … 3 bits
}
fn reduccion(v: f32) -> u32 {
    (1.0 + v * v * 31.0).round() as u32
}
fn forma(v: f32) -> usize {
    ((v * 3.999) as usize).min(3)
}
fn semitonos(v: f32) -> i32 {
    ((v - 0.5) * 24.0).round() as i32
}
fn db(v: f32) -> f32 {
    10f32.powf(v / 20.0)
}

/// Línea de retardo estéreo con lectura fraccionaria.
struct Linea {
    l: Vec<f32>,
    r: Vec<f32>,
    pos: usize,
}
impl Linea {
    fn new(n: usize) -> Linea {
        Linea { l: vec![0.0; n.max(4)], r: vec![0.0; n.max(4)], pos: 0 }
    }
    #[inline]
    fn escribir(&mut self, l: f32, r: f32) {
        self.l[self.pos] = l;
        self.r[self.pos] = r;
        self.pos = (self.pos + 1) % self.l.len();
    }
    /// Lee `d` muestras atrás (d ≥ 1).
    #[inline]
    fn leer(buf: &[f32], pos: usize, d: f32) -> f32 {
        let n = buf.len();
        let d = d.clamp(1.0, (n - 2) as f32);
        let x = pos as f32 + n as f32 - d;
        let i = x as usize;
        let f = x - i as f32;
        let a = buf[i % n];
        let b = buf[(i + 1) % n];
        a + (b - a) * f
    }
}

/// Estado de un efecto mientras suena.
pub struct Proceso {
    pub tipo: TipoEfecto,
    sr: f32,
    fase: f32,
    linea: Linea,
    fb: [f32; 2],
    ap: [[f32; 6]; 2],
    env: f32,
    gate_g: f32,
    hold: [f32; 2],
    cont: u32,
    lp: [f32; 2],
    // tape stop
    vel: f32,
    lectura: f64,
    // multibanda: filtros de cruce y envolventes por banda
    xo: [[f32; 2]; 2],
    benv: [f32; 3],
    // spreader: graves al centro
    lpb: [f32; 2],
}

impl Proceso {
    pub fn new(tipo: TipoEfecto, sr: f32) -> Proceso {
        let largo = match tipo {
            TipoEfecto::TapeDelay | TipoEfecto::TranceDelay => (sr * 2.2) as usize,
            TipoEfecto::TapeStop => (sr * 4.0) as usize,
            TipoEfecto::Pitcher => (sr * 0.1) as usize,
            TipoEfecto::Chorus | TipoEfecto::Flanger | TipoEfecto::Spreader => (sr * 0.06) as usize,
            _ => 4,
        };
        Proceso {
            tipo,
            sr,
            fase: 0.0,
            linea: Linea::new(largo),
            fb: [0.0; 2],
            ap: [[0.0; 6]; 2],
            env: 0.0,
            gate_g: 0.0,
            hold: [0.0; 2],
            cont: 0,
            lp: [0.0; 2],
            vel: 1.0,
            lectura: 0.0,
            xo: [[0.0; 2]; 2],
            benv: [0.0; 3],
            lpb: [0.0; 2],
        }
    }

    /// Procesa un bloque estéreo en el lugar. `pasos_s` = semicorcheas por segundo (para los efectos al ritmo).
    pub fn procesar(&mut self, l: &mut [f32], r: &mut [f32], e: &EfectoP, pasos_s: f32) {
        let sr = self.sr;
        let p = e.p;
        let mezcla = e.mezcla.clamp(0.0, 1.0);
        let n = l.len().min(r.len());
        match self.tipo {
            TipoEfecto::Chorus | TipoEfecto::Flanger => {
                let flanger = self.tipo == TipoEfecto::Flanger;
                let inc = vel_lfo(p[0]) / sr;
                let (base, prof) = if flanger { (0.0003 + p[3] * 0.004, p[1] * 0.003) } else { (0.012, p[1] * 0.008) };
                let fbk = if flanger { (p[2] * 2.0 - 1.0) * 0.92 } else { p[2] * 0.6 };
                let ancho = if flanger { 0.25 } else { p[3] * 0.5 };
                for i in 0..n {
                    self.fase = (self.fase + inc).fract();
                    let mi = (self.fase * TAU).sin();
                    let md = ((self.fase + ancho) * TAU).sin();
                    let dl = (base + prof * (0.5 + 0.5 * mi)) * sr;
                    let dr = (base + prof * (0.5 + 0.5 * md)) * sr;
                    let wl = Linea::leer(&self.linea.l, self.linea.pos, dl);
                    let wr = Linea::leer(&self.linea.r, self.linea.pos, dr);
                    self.linea.escribir(l[i] + wl * fbk, r[i] + wr * fbk);
                    l[i] = l[i] * (1.0 - mezcla * 0.5) + wl * mezcla;
                    r[i] = r[i] * (1.0 - mezcla * 0.5) + wr * mezcla;
                }
            }
            TipoEfecto::Phaser => {
                let inc = vel_lfo(p[0]) / sr;
                let fbk = p[2] * 0.8;
                let centro = 300.0 * 8f32.powf(p[3]);
                for i in 0..n {
                    self.fase = (self.fase + inc).fract();
                    let m = (self.fase * TAU).sin();
                    let f = (centro * 3f32.powf(m * p[1] * 1.5)).clamp(40.0, sr * 0.45);
                    let t = (PI * f / sr).tan();
                    let a = (t - 1.0) / (t + 1.0);
                    for (k, x) in [l[i], r[i]].into_iter().enumerate() {
                        let mut y = x + self.fb[k] * fbk;
                        for s in 0..6 {
                            let z = self.ap[k][s];
                            let o = a * y + z;
                            self.ap[k][s] = y - a * o;
                            y = o;
                        }
                        self.fb[k] = y;
                        let out = x * (1.0 - mezcla * 0.5) + y * mezcla * 0.5 + x * mezcla * 0.5;
                        if k == 0 {
                            l[i] = out;
                        } else {
                            r[i] = out;
                        }
                    }
                }
            }
            TipoEfecto::Tremolo => {
                let pasos = DIVISIONES[div_idx(p[0])].1;
                let inc = pasos_s.max(0.1) / pasos / sr;
                for i in 0..n {
                    self.fase = (self.fase + inc).fract();
                    let s = if p[2] < 0.5 { 0.5 + 0.5 * (self.fase * TAU).cos() } else if self.fase < 0.5 { 1.0 } else { 0.0 };
                    let ga = 1.0 - p[1] * s;
                    // con «Paneo» el volumen va de un lado al otro
                    let gb = 1.0 - p[1] * (if p[2] < 0.5 { 0.5 - 0.5 * (self.fase * TAU).cos() } else { 1.0 - s });
                    let (gl, gr) = (ga, ga * (1.0 - p[3]) + gb * p[3]);
                    l[i] *= 1.0 - mezcla + mezcla * gl;
                    r[i] *= 1.0 - mezcla + mezcla * gr;
                }
            }
            TipoEfecto::Gate => {
                let umbral = db(-60.0 + p[0] * 60.0);
                let rel = 1.0 - (-1.0 / ((0.01 + p[1] * 0.5) * sr)).exp();
                let ata = 1.0 - (-1.0 / ((0.0005 + p[3] * 0.05) * sr)).exp();
                let piso = 1.0 - p[2];
                for i in 0..n {
                    let a = l[i].abs().max(r[i].abs());
                    self.env += (a - self.env) * if a > self.env { 0.3 } else { 0.0015 };
                    let objetivo = if self.env > umbral { 1.0 } else { piso };
                    self.gate_g += (objetivo - self.gate_g) * if objetivo > self.gate_g { ata } else { rel };
                    let g = 1.0 - mezcla + mezcla * self.gate_g;
                    l[i] *= g;
                    r[i] *= g;
                }
            }
            TipoEfecto::Bitcrusher => {
                let q = (2f32.powi(bits(p[0]) as i32 - 1)).max(1.0);
                let red = reduccion(p[1]);
                let pre = db((p[2] - 0.5) * 24.0);
                let suave = 1.0 - p[3] * 0.95;
                for i in 0..n {
                    if self.cont == 0 {
                        self.hold[0] = ((l[i] * pre).clamp(-1.0, 1.0) * q).round() / q;
                        self.hold[1] = ((r[i] * pre).clamp(-1.0, 1.0) * q).round() / q;
                    }
                    self.cont = (self.cont + 1) % red;
                    self.lp[0] += (self.hold[0] - self.lp[0]) * suave;
                    self.lp[1] += (self.hold[1] - self.lp[1]) * suave;
                    l[i] = l[i] * (1.0 - mezcla) + self.lp[0] * mezcla;
                    r[i] = r[i] * (1.0 - mezcla) + self.lp[1] * mezcla;
                }
            }
            TipoEfecto::Waveshaper => {
                let drive = 1.0 + p[0] * p[0] * 30.0;
                let fm = forma(p[1]);
                let tono = 1.0 - (-TAU * (800.0 * 20f32.powf(p[2])) / sr).exp();
                let salida = db((p[3] - 0.5) * 24.0) / drive.sqrt().max(1.0);
                let f = |x: f32| -> f32 {
                    let x = x * drive;
                    match fm {
                        0 => x.tanh(),
                        1 => x.clamp(-1.0, 1.0),
                        2 => (x * 0.5 * PI).sin(),
                        _ => x.abs().tanh() * 2.0 - 0.5,
                    }
                };
                for i in 0..n {
                    for k in 0..2 {
                        let x = if k == 0 { l[i] } else { r[i] };
                        self.lp[k] += (f(x) - self.lp[k]) * tono;
                        let y = x * (1.0 - mezcla) + self.lp[k] * salida * mezcla * drive.sqrt().min(4.0);
                        if k == 0 {
                            l[i] = y;
                        } else {
                            r[i] = y;
                        }
                    }
                }
            }
            TipoEfecto::TapeStop => {
                // se graba todo en el anillo y se lee a la velocidad de la cinta
                let frenar = p[0] >= 0.5;
                let t_fren = 0.1 + p[1] * 2.9;
                let t_arr = 0.1 + p[2] * 2.9;
                let len = self.linea.l.len();
                for i in 0..n {
                    self.linea.escribir(l[i], r[i]);
                    let objetivo = if frenar { 0.0 } else { 1.0 };
                    let paso = 1.0 / ((if frenar { t_fren } else { t_arr }) * sr);
                    if self.vel < objetivo {
                        self.vel = (self.vel + paso).min(1.0);
                    } else if self.vel > objetivo {
                        self.vel = (self.vel - paso).max(0.0);
                    }
                    // curva: la velocidad baja más rápido o más lento al final
                    let v = self.vel.powf(0.5 + p[3] * 2.0);
                    if self.vel >= 0.999 && !frenar {
                        // sonando normal: la lectura alcanza a la escritura
                        self.lectura = self.linea.pos as f64;
                        continue;
                    }
                    self.lectura += v as f64;
                    let atras = (self.linea.pos as f64 - self.lectura).rem_euclid(len as f64).max(1.0) as f32;
                    let wl = Linea::leer(&self.linea.l, self.linea.pos, atras);
                    let wr = Linea::leer(&self.linea.r, self.linea.pos, atras);
                    l[i] = l[i] * (1.0 - mezcla) + wl * mezcla * v.min(1.0).sqrt();
                    r[i] = r[i] * (1.0 - mezcla) + wr * mezcla * v.min(1.0).sqrt();
                }
            }
            TipoEfecto::TapeDelay => {
                let base = (0.04 + p[0] * 0.96) * sr;
                let fbk = p[1] * 0.92;
                let wow = p[2] * 0.004 * sr;
                let osc = 1.0 - (-TAU * (6000.0 * 0.08f32.powf(p[3])) / sr).exp();
                for i in 0..n {
                    self.fase = (self.fase + 0.7 / sr).fract();
                    let d = base + wow * (self.fase * TAU).sin();
                    let wl = Linea::leer(&self.linea.l, self.linea.pos, d);
                    let wr = Linea::leer(&self.linea.r, self.linea.pos, d);
                    // la cinta oscurece y satura cada repetición
                    self.lp[0] += (wl - self.lp[0]) * osc;
                    self.lp[1] += (wr - self.lp[1]) * osc;
                    let (sl, sr2) = ((self.lp[0] * 1.2).tanh() / 1.2, (self.lp[1] * 1.2).tanh() / 1.2);
                    self.linea.escribir(l[i] + sl * fbk, r[i] + sr2 * fbk);
                    l[i] += sl * mezcla;
                    r[i] += sr2 * mezcla;
                }
            }
            TipoEfecto::TranceDelay => {
                let d = (DIVISIONES[div_idx(p[0])].1 / pasos_s.max(0.1) * sr).min(self.linea.l.len() as f32 - 4.0);
                let fbk = p[1] * 0.9;
                // filtro: abajo de 0,5 pasa bajos, arriba pasa altos
                let lpc = 1.0 - (-TAU * (500.0 * 30f32.powf((p[2] * 2.0).min(1.0))) / sr).exp();
                let hpc = 1.0 - (-TAU * (20.0 * 60f32.powf(((p[2] - 0.5) * 2.0).max(0.0))) / sr).exp();
                let reb = p[3];
                for i in 0..n {
                    let wl = Linea::leer(&self.linea.l, self.linea.pos, d);
                    let wr = Linea::leer(&self.linea.r, self.linea.pos, d);
                    self.lp[0] += (wl - self.lp[0]) * lpc;
                    self.lp[1] += (wr - self.lp[1]) * lpc;
                    self.lpb[0] += (self.lp[0] - self.lpb[0]) * hpc;
                    self.lpb[1] += (self.lp[1] - self.lpb[1]) * hpc;
                    let (fl, fr) = (self.lp[0] - self.lpb[0], self.lp[1] - self.lpb[1]);
                    // rebote: la realimentación cruza de lado
                    let entrada = (l[i] + r[i]) * 0.5;
                    let al = entrada + (fr * reb + fl * (1.0 - reb)) * fbk;
                    let ar = entrada * (1.0 - reb) + (fl * reb + fr * (1.0 - reb)) * fbk;
                    self.linea.escribir(al, ar);
                    l[i] += fl * mezcla;
                    r[i] += fr * mezcla;
                }
            }
            TipoEfecto::Multibanda => {
                let c1 = 1.0 - (-TAU * 200.0 / sr).exp();
                let c2 = 1.0 - (-TAU * 3000.0 / sr).exp();
                let ganancias = [db((p[0] - 0.5) * 24.0), db((p[1] - 0.5) * 24.0), db((p[2] - 0.5) * 24.0)];
                let intens = p[3];
                let umbral = db(-12.0 - intens * 18.0);
                let razon = 1.0 + intens * 5.0;
                let (at, re) = (1.0 - (-1.0 / (0.005 * sr)).exp(), 1.0 - (-1.0 / (0.12 * sr)).exp());
                for i in 0..n {
                    let mut out = [0f32; 2];
                    let mut bandas = [[0f32; 3]; 2];
                    for k in 0..2 {
                        let x = if k == 0 { l[i] } else { r[i] };
                        self.xo[0][k] += (x - self.xo[0][k]) * c1;
                        self.xo[1][k] += (x - self.xo[1][k]) * c2;
                        let bajo = self.xo[0][k];
                        let medio = self.xo[1][k] - bajo;
                        let alto = x - self.xo[1][k];
                        bandas[k] = [bajo, medio, alto];
                    }
                    for b in 0..3 {
                        let a = bandas[0][b].abs().max(bandas[1][b].abs());
                        self.benv[b] += (a - self.benv[b]) * if a > self.benv[b] { at } else { re };
                        let g = if self.benv[b] > umbral { (umbral / self.benv[b]).powf(1.0 - 1.0 / razon) } else { 1.0 };
                        // ganancia de compensación según la intensidad
                        let comp = 1.0 + intens * 0.8;
                        for k in 0..2 {
                            out[k] += bandas[k][b] * g * ganancias[b] * comp;
                        }
                    }
                    l[i] = l[i] * (1.0 - mezcla) + out[0] * mezcla;
                    r[i] = r[i] * (1.0 - mezcla) + out[1] * mezcla;
                }
            }
            TipoEfecto::Spreader => {
                let ancho = p[0] * 2.0;
                let haas = (p[1] * 0.02 * sr).max(1.0);
                let graves = p[2] >= 0.5;
                let salida = db((p[3] - 0.5) * 24.0);
                let cb = 1.0 - (-TAU * 150.0 / sr).exp();
                for i in 0..n {
                    let (x, y) = (l[i], r[i]);
                    self.linea.escribir(x, y);
                    // retardo corto en un lado (efecto Haas) para ensanchar
                    let yd = Linea::leer(&self.linea.r, self.linea.pos, haas);
                    let yy = if p[1] > 0.01 { y * 0.5 + yd * 0.5 } else { y };
                    let m = (x + yy) * 0.5;
                    let mut s = (x - yy) * 0.5 * ancho;
                    if graves {
                        // los graves del lado se quitan (quedan al centro)
                        self.lpb[0] += (s - self.lpb[0]) * cb;
                        s -= self.lpb[0];
                    }
                    let (ol, or) = ((m + s) * salida, (m - s) * salida);
                    l[i] = x * (1.0 - mezcla) + ol * mezcla;
                    r[i] = y * (1.0 - mezcla) + or * mezcla;
                }
            }
            TipoEfecto::Pitcher => {
                // desplazador de dos cabezales: cambia el tono sin cambiar la velocidad
                let st = semitonos(p[0]) as f32 + (p[1] - 0.5);
                let razon = 2f32.powf(st / 12.0);
                let ventana = (0.02 + p[2] * 0.06) * sr;
                let salida = db((p[3] - 0.5) * 24.0);
                for i in 0..n {
                    self.linea.escribir(l[i], r[i]);
                    self.fase = (self.fase + (1.0 - razon) / ventana).rem_euclid(1.0);
                    let q = (self.fase + 0.5).fract();
                    let g1 = (PI * self.fase).sin().powi(2);
                    let g2 = (PI * q).sin().powi(2);
                    let (d1, d2) = (self.fase * ventana + 1.0, q * ventana + 1.0);
                    let wl = Linea::leer(&self.linea.l, self.linea.pos, d1) * g1 + Linea::leer(&self.linea.l, self.linea.pos, d2) * g2;
                    let wr = Linea::leer(&self.linea.r, self.linea.pos, d1) * g1 + Linea::leer(&self.linea.r, self.linea.pos, d2) * g2;
                    l[i] = l[i] * (1.0 - mezcla) + wl * salida * mezcla;
                    r[i] = r[i] * (1.0 - mezcla) + wr * salida * mezcla;
                }
            }
        }
        // protección: nada de valores inválidos que rompan el audio
        for i in 0..n {
            if !l[i].is_finite() || !r[i].is_finite() {
                l[i] = 0.0;
                r[i] = 0.0;
                self.fb = [0.0; 2];
                self.ap = [[0.0; 6]; 2];
            }
        }
    }

    /// ¿El efecto deja cola (sigue sonando sin entrada)?
    pub fn tiene_cola(tipo: TipoEfecto) -> bool {
        matches!(tipo, TipoEfecto::TapeDelay | TipoEfecto::TranceDelay | TipoEfecto::Chorus | TipoEfecto::Flanger | TipoEfecto::Phaser | TipoEfecto::TapeStop)
    }
}

/// Mantiene los procesos al día con la lista de efectos (crea o rehace los que cambiaron de tipo).
pub fn sincronizar(procesos: &mut Vec<Proceso>, lista: &[EfectoP], sr: f32) {
    if procesos.len() != lista.len() || procesos.iter().zip(lista).any(|(p, e)| p.tipo != e.tipo) {
        let mut viejos = std::mem::take(procesos);
        for e in lista {
            match viejos.iter().position(|p| p.tipo == e.tipo) {
                Some(i) => procesos.push(viejos.remove(i)),
                None => procesos.push(Proceso::new(e.tipo, sr)),
            }
        }
    }
}
