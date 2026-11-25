//! Quitar ruido del micrófono (solo en PC), como el filtro de ruido de Discord, con dos motores:
//!
//! - **Cantar**: reducción de ruido espectral (la de los estudios). Aprende solo cómo suena el
//!   fondo (ventilador, aire, zumbido, soplido del micrófono) y resta eso en cada frecuencia. No intenta
//!   adivinar qué es voz, así que no aplasta las notas largas, el vibrato ni los agudos. ~16 ms de retraso.
//! - **Hablar**: DeepFilterNet 3 (paquete `dfn3_rs`), una red neuronal entrenada con voz hablada: quita
//!   también ruidos que cambian (teclado, tráfico, gente), pero con el canto deforma las notas. ~30 ms.
//!
//! Se aplica a lo que entra por el micrófono, así que lo que te escuchas y lo que se graba suenan igual.
//! El retraso se descuenta de la toma al terminar de grabar.

use dfn3_rs::{Denoiser, ALGORITHM_DELAY, FRAME_SIZE, SAMPLE_RATE};
use realfft::num_complex::Complex32;
use realfft::{RealFftPlanner, RealToComplex, ComplexToReal};
use std::sync::Arc;

/// Filtro de ruido en vivo para la entrada del micrófono.
pub struct RuidoVivo {
    motor: Motor,
    /// retraso total en muestras de la entrada
    pub retraso: usize,
}

enum Motor {
    Canto(Box<Canto>),
    Habla(Box<Habla>),
}

impl RuidoVivo {
    /// `sr`: frecuencia del micrófono; `intensidad`: 0..1 (1 = «Solo mi voz»); `canto`: motor para cantar.
    pub fn new(sr: u32, intensidad: f32, canto: bool) -> Self {
        if canto {
            let c = Canto::new(intensidad);
            let retraso = c.retraso();
            RuidoVivo { motor: Motor::Canto(Box::new(c)), retraso }
        } else {
            let h = Habla::new(sr, intensidad);
            let retraso = h.retraso;
            RuidoVivo { motor: Motor::Habla(Box::new(h)), retraso }
        }
    }

    /// Cambia la intensidad sin saltos (se puede mover mientras suena).
    pub fn intensidad(&mut self, f: f32) {
        match &mut self.motor {
            Motor::Canto(c) => c.intensidad(f),
            Motor::Habla(h) => h.intensidad(f),
        }
    }

    /// Limpia las muestras que llegan y agrega las limpias a `salida` (con el retraso fijo de `retraso`).
    pub fn procesar(&mut self, entrada: &[f32], salida: &mut Vec<f32>) {
        match &mut self.motor {
            Motor::Canto(c) => c.procesar(entrada, salida),
            Motor::Habla(h) => h.procesar(entrada, salida),
        }
    }
}

/// Quita el ruido de un audio ya grabado (mismo filtro, sin retraso: la salida queda alineada).
pub fn quitar_ruido(x: &[f32], sr: u32, intensidad: f32, canto: bool) -> Vec<f32> {
    let mut f = RuidoVivo::new(sr, intensidad, canto);
    let mut out = Vec::with_capacity(x.len() + f.retraso + 4096);
    f.procesar(x, &mut out);
    // silencio al final para que salga la cola que quedó dentro del filtro
    let cola = vec![0.0f32; f.retraso + 2 * 1024 + (FRAME_SIZE as f64 * 2.0 * sr as f64 / SAMPLE_RATE as f64) as usize + 4];
    f.procesar(&cola, &mut out);
    out.into_iter().skip(f.retraso).chain(std::iter::repeat(0.0)).take(x.len()).collect()
}

// ------------------------------------------------------------------------------------------------
// Cantar: reducción espectral con estimación automática del ruido de fondo

/// Tamaño de la ventana (≈21 ms a 48 kHz) y salto (75 % de solapamiento).
const N: usize = 1024;
const SALTO: usize = N / 4;
const BINS: usize = N / 2 + 1;

pub struct Canto {
    fft: Arc<dyn RealToComplex<f32>>,
    ifft: Arc<dyn ComplexToReal<f32>>,
    ventana: Vec<f32>,
    /// últimas N muestras de entrada y cuántas nuevas llegaron desde el último análisis
    hist: Vec<f32>,
    nuevas: usize,
    /// suma de ventanas solapadas (salida)
    ola: Vec<f32>,
    marco: Vec<f32>,
    espectro: Vec<Complex32>,
    tmp: Vec<Complex32>,
    /// ruido estimado por frecuencia (potencia), potencia suavizada de la señal, ganancia anterior
    ruido: Vec<f32>,
    pot: Vec<f32>,
    gan: Vec<f32>,
    gan_s: Vec<f32>,
    /// mínimo reciente de la energía del cuadro (para saber cuándo solo hay fondo)
    marcos: u64,
    /// cuánto puede bajar el ruido (ganancia mínima, lineal) y su objetivo (para moverla sin saltos)
    piso: f32,
    piso_obj: f32,
}

impl Canto {
    fn new(intensidad: f32) -> Self {
        let mut pl = RealFftPlanner::<f32>::new();
        let fft = pl.plan_fft_forward(N);
        let ifft = pl.plan_fft_inverse(N);
        // raíz de Hann periódica en el análisis y en la síntesis: con 75 % de solapamiento suma 2
        let ventana: Vec<f32> = (0..N).map(|i| (0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / N as f32).cos()).sqrt()).collect();
        let espectro = fft.make_output_vec();
        let tmp = vec![Complex32::new(0.0, 0.0); fft.get_scratch_len().max(ifft.get_scratch_len())];
        let mut c = Canto {
            fft,
            ifft,
            ventana,
            hist: vec![0.0; N],
            nuevas: 0,
            ola: vec![0.0; N],
            marco: vec![0.0; N],
            espectro,
            tmp,
            ruido: vec![0.0; BINS],
            pot: vec![0.0; BINS],
            gan: vec![1.0; BINS],
            gan_s: vec![1.0; BINS],
            marcos: 0,
            piso: 1.0,
            piso_obj: 1.0,
        };
        c.intensidad(intensidad);
        c.piso = c.piso_obj;
        c
    }

    fn retraso(&self) -> usize {
        N - SALTO
    }

    /// 0 → baja el ruido 8 dB; 1 («Solo mi voz») → 30 dB. Más que eso empieza a sonar a «agua».
    fn intensidad(&mut self, f: f32) {
        let db = 8.0 + f.clamp(0.0, 1.0) * 22.0;
        self.piso_obj = 10f32.powf(-db / 20.0);
    }

    fn procesar(&mut self, entrada: &[f32], salida: &mut Vec<f32>) {
        for &x in entrada {
            let x = if x.is_finite() { x } else { 0.0 };
            // la historia corre hacia la izquierda de a un salto entero (se copia solo al analizar)
            self.hist[N - SALTO + self.nuevas] = x;
            self.nuevas += 1;
            if self.nuevas == SALTO {
                self.nuevas = 0;
                self.analizar();
                salida.extend_from_slice(&self.ola[..SALTO]);
                self.ola.copy_within(SALTO.., 0);
                self.ola[N - SALTO..].fill(0.0);
                self.hist.copy_within(SALTO.., 0);
            }
        }
    }

    fn analizar(&mut self) {
        for i in 0..N {
            self.marco[i] = self.hist[i] * self.ventana[i];
        }
        if self.fft.process_with_scratch(&mut self.marco, &mut self.espectro, &mut self.tmp).is_err() {
            return;
        }
        self.marcos += 1;
        self.piso += (self.piso_obj - self.piso) * 0.05;
        let escala = 1.0 / (N as f32 * N as f32);
        // energía del cuadro frente al ruido conocido: ¿solo hay fondo ahora?
        let (mut e_sen, mut e_rui) = (0.0f32, 0.0f32);
        for k in 1..BINS {
            let p = self.espectro[k].norm_sqr() * escala;
            // potencia suavizada (≈ 30 ms) para estimar el ruido sin que tiemble
            self.pot[k] += (p - self.pot[k]) * 0.35;
            e_sen += self.pot[k];
            e_rui += self.ruido[k];
        }
        let arrancando = self.marcos < 40; // primeros ~200 ms: se aprende el fondo lo más rápido posible
        let solo_fondo = arrancando || e_sen < e_rui * 2.0;
        for k in 1..BINS {
            let p = self.pot[k];
            let r = &mut self.ruido[k];
            if arrancando && self.marcos == 1 {
                *r = p;
            } else if p < *r {
                // bajó: el ruido nunca es más que lo que suena ahora
                *r += (p - *r) * 0.2;
            } else if solo_fondo {
                *r += (p - *r) * if arrancando { 0.2 } else { 0.03 };
            } else {
                // mientras cantas casi no se mueve (≈ +1 dB por segundo), así una nota larga no se vuelve «ruido»
                *r *= 1.0012;
            }
        }
        // ganancia de Wiener con SNR «dirigida por decisión» (Ephraim-Malah): poca agua y nada robótico
        for k in 0..BINS {
            let p = self.espectro[k].norm_sqr() * escala;
            let r = self.ruido[k].max(1e-12) * 1.4; // un poco de margen sobre el ruido medido
            let post = p / r;
            let prev = self.gan[k] * self.gan[k] * self.pot[k] / r;
            let prio = 0.96 * prev + 0.04 * (post - 1.0).max(0.0);
            self.gan[k] = (prio / (1.0 + prio)).clamp(0.0, 1.0);
        }
        // suavizado en frecuencia (3 bandas) y en el tiempo al cerrar (abre al instante, cierra en ~40 ms)
        for k in 0..BINS {
            let a = self.gan[k.saturating_sub(1)];
            let b = self.gan[k];
            let c = self.gan[(k + 1).min(BINS - 1)];
            let g = (0.25 * a + 0.5 * b + 0.25 * c).max(self.piso);
            let s = &mut self.gan_s[k];
            *s = if g > *s { g } else { *s + (g - *s) * 0.3 };
            self.espectro[k] *= *s;
        }
        // las frecuencias 0 y la más alta tienen que ser reales para la inversa
        self.espectro[0].im = 0.0;
        self.espectro[BINS - 1].im = 0.0;
        if self.ifft.process_with_scratch(&mut self.espectro, &mut self.marco, &mut self.tmp).is_err() {
            return;
        }
        // 1/N de la inversa y 1/2 por la suma de ventanas solapadas
        let k = 0.5 / N as f32;
        for i in 0..N {
            self.ola[i] += self.marco[i] * self.ventana[i] * k;
        }
    }
}

// ------------------------------------------------------------------------------------------------
// Hablar: DeepFilterNet 3

/// Intensidad (0..1) a decibeles que puede bajar el ruido. Arriba del 99 % no hay tope.
fn habla_db(f: f32) -> f32 {
    let f = f.clamp(0.0, 1.0);
    if f >= 0.99 {
        100.0
    } else {
        6.0 + f * 54.0
    }
}

/// Remuestreo lineal continuo (de a pedazos, sin cortes entre llamadas).
struct Remuestreo {
    /// muestras de entrada por cada muestra de salida
    paso: f64,
    /// posición de la próxima salida entre `prev` (0) y la muestra que llega (1)
    t: f64,
    prev: f32,
}

impl Remuestreo {
    fn new(sr_entrada: f64, sr_salida: f64) -> Self {
        Remuestreo { paso: sr_entrada / sr_salida, t: 0.0, prev: 0.0 }
    }

    #[inline]
    fn empujar(&mut self, x: f32, salida: &mut impl FnMut(f32)) {
        while self.t < 1.0 {
            salida(self.prev + (x - self.prev) * self.t as f32);
            self.t += self.paso;
        }
        self.t -= 1.0;
        self.prev = x;
    }
}

struct Habla {
    dn: Box<Denoiser>,
    a48: Remuestreo,
    de48: Remuestreo,
    bloque: Box<[f32; FRAME_SIZE]>,
    n: usize,
    db: f32,
    puerta: Puerta,
    retraso: usize,
}

/// «Solo mi voz» al hablar: después del filtro, apaga lo que suena bastante más bajo que una voz cerca
/// del micrófono (gente lejos, la tele de la otra pieza). Trabaja a 48 kHz, dentro del bloque de 10 ms.
struct Puerta {
    on: bool,
    env: f32,
    gan: f32,
    espera: u32,
}

impl Puerta {
    /// nivel desde el que se considera voz (-42 dBFS), cuánto baja lo demás (-30 dB) y tiempos
    const UMBRAL: f32 = 0.008;
    const CERRADA: f32 = 0.03;
    const SOSTENER: u32 = (SAMPLE_RATE as f32 * 0.15) as u32;

    #[inline]
    fn run(&mut self, y: f32) -> f32 {
        let sr = SAMPLE_RATE as f32;
        let a = y.abs();
        self.env = if a > self.env { a } else { self.env + (a - self.env) * (1.0 / (0.05 * sr)) };
        let objetivo = if !self.on {
            1.0
        } else if self.env > Self::UMBRAL {
            self.espera = Self::SOSTENER;
            1.0
        } else if self.espera > 0 {
            self.espera -= 1;
            1.0
        } else {
            Self::CERRADA
        };
        // abre en ~2 ms (no se come el principio de las palabras) y cierra en ~120 ms
        let k = if objetivo > self.gan { 1.0 / (0.002 * sr) } else { 1.0 / (0.12 * sr) };
        self.gan += (objetivo - self.gan) * k;
        y * self.gan
    }
}

impl Habla {
    fn new(sr: u32, intensidad: f32) -> Self {
        let db = habla_db(intensidad);
        let mut dn = Box::new(Denoiser::new());
        // calentamiento: que todas las etapas de la red ya hayan corrido una vez antes de grabar
        dn.set_thresholds(-100.0, 100.0, 100.0);
        let mut b = [0.0f32; FRAME_SIZE];
        let mut s = 0x9e37_79b9u32;
        for _ in 0..6 {
            for v in b.iter_mut() {
                s ^= s << 13;
                s ^= s >> 17;
                s ^= s << 5;
                *v = (s as f32 / u32::MAX as f32 - 0.5) * 1e-3;
            }
            dn.process_frame(&mut b);
        }
        dn.reset();
        dn.set_thresholds(-10.0, 30.0, 20.0);
        dn.set_attenuation_limit(db);
        let sr_f = sr.max(1) as f64;
        let retraso = (ALGORITHM_DELAY as f64 * sr_f / SAMPLE_RATE as f64).round() as usize + 1;
        Habla {
            dn,
            a48: Remuestreo::new(sr_f, SAMPLE_RATE as f64),
            de48: Remuestreo::new(SAMPLE_RATE as f64, sr_f),
            bloque: Box::new([0.0; FRAME_SIZE]),
            n: 0,
            db,
            puerta: Puerta { on: db >= 99.0, env: 0.0, gan: 1.0, espera: 0 },
            retraso,
        }
    }

    fn intensidad(&mut self, f: f32) {
        let db = habla_db(f);
        if (db - self.db).abs() > 0.01 && db.is_finite() {
            self.db = db;
            self.puerta.on = db >= 99.0;
            self.dn.set_attenuation_limit_smooth(db, std::time::Duration::from_millis(60));
        }
    }

    fn procesar(&mut self, entrada: &[f32], salida: &mut Vec<f32>) {
        let Habla { dn, a48, de48, bloque, n, puerta, .. } = self;
        for &x in entrada {
            let x = if x.is_finite() { x } else { 0.0 };
            a48.empujar(x, &mut |v| {
                bloque[*n] = v;
                *n += 1;
                if *n == FRAME_SIZE {
                    *n = 0;
                    dn.process_frame(bloque);
                    for &y in bloque.iter() {
                        let y = puerta.run(y);
                        de48.empujar(y, &mut |z| salida.push(z));
                    }
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rnd(s: &mut u32) -> f32 {
        *s ^= *s << 13;
        *s ^= *s >> 17;
        *s ^= *s << 5;
        *s as f32 / u32::MAX as f32 - 0.5
    }

    /// Canto de prueba: 1 s de silencio con ruido, luego una nota larga de 5 s con vibrato y armónicos
    /// hasta los agudos. Mide cuánto baja el ruido y cuánto cambia la nota (lo ideal: casi nada).
    fn prueba(sr: u32, canto: bool) -> (f32, f32, f32) {
        let n = sr as usize * 6;
        let mut s = 7u32;
        let ruido: Vec<f32> = (0..n).map(|_| rnd(&mut s) * 0.03).collect();
        let mut fase = 0.0f32;
        let nota: Vec<f32> = (0..n)
            .map(|i| {
                let t = i as f32 / sr as f32;
                if t < 1.0 {
                    return 0.0;
                }
                let f0 = 440.0 * (1.0 + 0.012 * (t * 5.5 * std::f32::consts::TAU).sin());
                fase += f0 / sr as f32;
                let ataque = ((t - 1.0) / 0.05).min(1.0);
                (1..20).map(|h| (fase * h as f32 * std::f32::consts::TAU).sin() / (h as f32).powf(1.3)).sum::<f32>() * 0.18 * ataque
            })
            .collect();
        let x: Vec<f32> = nota.iter().zip(&ruido).map(|(a, b)| a + b).collect();
        let t0 = std::time::Instant::now();
        let y = quitar_ruido(&x, sr, 0.7, canto);
        let dt = t0.elapsed().as_secs_f32() / 6.0;
        assert_eq!(y.len(), n);
        let e = |v: &[f32], a: f32, b: f32| v[(a * sr as f32) as usize..(b * sr as f32) as usize].iter().map(|z| z * z).sum::<f32>();
        // ruido: tramo sin voz (después de que el filtro aprendió el fondo)
        let baja = 10.0 * (e(&x, 0.4, 1.0) / e(&y, 0.4, 1.0).max(1e-15)).log10();
        // nota: cambio de nivel en el tramo final (donde un filtro mal hecho ya «se comió» la nota)
        let cambio = 10.0 * (e(&y, 4.0, 6.0) / e(&nota, 4.0, 6.0)).log10();
        // limpieza de la nota: cuánto se parece la salida a la nota sin ruido (antes y después)
        let dif = |v: &[f32]| -> Vec<f32> { v.iter().zip(&nota).map(|(a, b)| a - b).collect() };
        let snr_antes = 10.0 * (e(&nota, 2.0, 6.0) / e(&dif(&x), 2.0, 6.0)).log10();
        let snr_despues = 10.0 * (e(&nota, 2.0, 6.0) / e(&dif(&y), 2.0, 6.0)).log10();
        eprintln!("  canto={canto} sr {sr}: parecido a la nota limpia {snr_antes:.1} dB -> {snr_despues:.1} dB");
        (baja, cambio, dt)
    }

    #[test]
    fn cantar_respeta_la_nota_larga() {
        for sr in [44_100u32, 48_000] {
            let (baja, cambio, dt) = prueba(sr, true);
            let (baja_h, cambio_h, _) = prueba(sr, false);
            eprintln!("sr {sr}: CANTAR ruido -{baja:.1} dB, nota {cambio:+.2} dB, {dt:.3} s/s | HABLAR ruido -{baja_h:.1} dB, nota {cambio_h:+.2} dB");
            assert!(baja > 10.0, "cantar debe bajar el ruido");
            assert!(cambio.abs() < 1.0, "cantar no debe cambiar la nota larga");
            assert!(dt < 0.2);
        }
    }
}
