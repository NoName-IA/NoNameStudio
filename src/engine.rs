//! Motor de audio: secuenciador + mezcla. El mismo motor sirve para tocar en vivo y para exportar.
//! Procesa por bloques: voces → canales (EQ, NN Dynamics con sidechain, plugins) → buses → master.
use crate::dsp::*;
use crate::fx::{Comp, Eq6, Limiter, Space, Verb};
use crate::loudness::Lufs;
use crate::media::{proc_key, Buf, Files};
use crate::project::*;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;

pub const MAX_CH: usize = 64;
pub const BLOCK: usize = 256;
/// muestras de anticipación al programar pasos
const LOOKAHEAD: f64 = 256.0;

/// Lo que entra por el micrófono, para escucharse mientras se graba.
pub struct Monitor {
    pub cola: std::sync::Mutex<std::collections::VecDeque<f32>>,
    pub sr: AtomicU32,
    pub on: AtomicBool,
    /// volumen (f32 en bits)
    pub gain: AtomicU32,
    /// autotune en vivo: encendido, velocidad de corrección (f32 en bits), tónica y escala de NN Tune
    pub tune_on: AtomicBool,
    pub tune_speed: AtomicU32,
    pub tune_key: AtomicU32,
    pub tune_scale: AtomicU32,
    pub tune_mask: AtomicU32,
    pub tune_estilo: AtomicU32,
    /// seguir los acordes de la canción (si hay un acorde de 3 notas o más)
    pub seguir: AtomicBool,
    /// algoritmo de NN Tune y volumen de la armonía (f32 en bits)
    pub tune_alg: AtomicU32,
    pub tune_mezcla: AtomicU32,
    /// paneo (-1…1) y reverberación (0…1) de la voz que te escuchas (f32 en bits)
    pub pan: AtomicU32,
    pub reverb: AtomicU32,
    /// acorde que suena ahora (máscara de 12 bits)
    pub acorde: AtomicU32,
    /// nota cantada (f32 en bits; 0 = nada) y nota corregida (+1; 0 = ninguna)
    pub cantada: AtomicU32,
    pub objetivo: AtomicU32,
}

/// Estado compartido con la interfaz (solo atómicos, sin bloqueos).
pub struct Status {
    pub playing: AtomicBool,
    pub pos_steps: AtomicU64,
    pub meters: Vec<AtomicU32>,
    pub master: [AtomicU32; 2],
    /// reducción de ganancia (dB) del compresor y del limitador del master
    pub gr: [AtomicU32; 2],
    /// nivel de entrada del micrófono mientras se graba
    pub input: AtomicU32,
    /// sonoridad: momentánea, corto plazo e integrada (LUFS) y pico (lineal)
    pub lufs: [AtomicU32; 3],
    pub tpeak: AtomicU32,
    /// carga del hilo de audio (0..1)
    pub load: AtomicU32,
    /// últimas muestras del master (mono) para el analizador de espectro
    pub scope: std::sync::Mutex<Vec<f32>>,
    pub monitor: Monitor,
}
impl Default for Status {
    fn default() -> Self {
        Status::new()
    }
}
impl Status {
    pub fn new() -> Status {
        let z = || AtomicU32::new(0);
        Status {
            playing: AtomicBool::new(false),
            pos_steps: AtomicU64::new(0f64.to_bits()),
            meters: (0..MAX_CH).map(|_| z()).collect(),
            master: [z(), z()],
            gr: [z(), z()],
            input: z(),
            lufs: [AtomicU32::new((-70f32).to_bits()), AtomicU32::new((-70f32).to_bits()), AtomicU32::new((-70f32).to_bits())],
            tpeak: z(),
            load: z(),
            scope: std::sync::Mutex::new(vec![0.0; 2048]),
            monitor: Monitor {
                cola: std::sync::Mutex::new(std::collections::VecDeque::with_capacity(48_000)),
                sr: AtomicU32::new(48_000),
                on: AtomicBool::new(false),
                gain: AtomicU32::new(0.8f32.to_bits()),
                tune_on: AtomicBool::new(false),
                tune_speed: AtomicU32::new(0.3f32.to_bits()),
                tune_key: AtomicU32::new(0),
                tune_scale: AtomicU32::new(0),
                tune_mask: AtomicU32::new(0),
                tune_estilo: AtomicU32::new(0),
                seguir: AtomicBool::new(false),
                tune_alg: AtomicU32::new(0),
                tune_mezcla: AtomicU32::new(1.0f32.to_bits()),
                pan: AtomicU32::new(0),
                reverb: AtomicU32::new(0),
                acorde: AtomicU32::new(0),
                cantada: AtomicU32::new(0),
                objetivo: AtomicU32::new(0),
            },
        }
    }
    pub fn pos(&self) -> f64 {
        f64::from_bits(self.pos_steps.load(Ordering::Relaxed))
    }
    pub fn meter(&self, i: usize) -> f32 {
        self.meters.get(i).map(|a| f32::from_bits(a.swap(0, Ordering::Relaxed))).unwrap_or(0.0)
    }
    pub fn master(&self) -> [f32; 2] {
        [f32::from_bits(self.master[0].swap(0, Ordering::Relaxed)), f32::from_bits(self.master[1].swap(0, Ordering::Relaxed))]
    }
    pub fn lufs(&self) -> [f32; 3] {
        [0, 1, 2].map(|i| f32::from_bits(self.lufs[i].load(Ordering::Relaxed)))
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Mode {
    Pattern(u32),
    Song,
}

/// Evento de nota para un plugin (desplazamiento en muestras dentro del bloque).
#[derive(Clone, Copy, Debug)]
pub struct Ev {
    pub at: u32,
    pub on: bool,
    pub note: u8,
    pub vel: f32,
}

/// Procesador externo (plugin) que vive en el hilo de audio.
pub trait Proc: Send {
    fn uid(&self) -> u32;
    /// Efecto: procesa en el lugar. Instrumento: escribe su salida (los buffers llegan en cero).
    fn process(&mut self, l: &mut [f32], r: &mut [f32], ev: &[Ev]);
    /// Automatización de un parámetro (valor normalizado 0..1).
    fn set_param(&mut self, _id: u32, _v: f64) {}
    /// Se llama en el hilo de audio antes de retirar el procesador.
    fn stop(&mut self) {}
}

pub enum Cmd {
    Project(Box<Project>),
    Play { mode: Mode, from_step: u32 },
    Stop,
    SetMode(Mode),
    Metronome(bool),
    Preview { ch: u32, m: u8, v: f32 },
    /// teclado MIDI: nota sostenida hasta NoteOff
    NoteOn { ch: u32, m: u8, v: f32 },
    NoteOff { ch: u32, m: u8 },
    /// Toma de audio procesada para (patrón, canal); None la borra.
    Sample { pat: u32, ch: u32, data: Option<(Arc<Vec<f32>>, f32)> },
    /// Archivo de audio (o variante procesada) disponible para samplers y clips.
    File { key: String, buf: Option<Arc<Buf>> },
    /// Escucha previa del navegador (None la detiene).
    PreviewFile(Option<Arc<Buf>>),
    /// Plugins de efecto de un canal (0 = master), en cualquier orden (se ordenan por uid).
    Inserts { ch: u32, procs: Vec<Box<dyn Proc>> },
    /// Instrumento externo de un canal.
    Instrument { ch: u32, proc: Option<Box<dyn Proc>> },
}

pub type Samples = HashMap<(u32, u32), (Arc<Vec<f32>>, f32)>;

/// Todo el audio que necesita el motor además del proyecto.
#[derive(Clone, Default)]
pub struct Bank {
    pub takes: Samples,
    pub files: Files,
}

struct Click {
    delay: u32,
    t: f32,
    f: f32,
    a: f32,
}

#[derive(Clone, Copy)]
struct Trig {
    /// ganancia y paneo de la pista de la playlist
    tg: f32,
    tp: f32,
    ci: usize,
    m: f32,
    v: f32,
    len: f32,
    delay: u32,
    seed: u32,
}

pub struct Engine {
    pub sr: f32,
    pub proj: Project,
    mode: Mode,
    playing: bool,
    /// muestras desde que empezó la reproducción
    pos: f64,
    /// paso donde empezó y próximo paso a programar (contado desde el inicio)
    start: u64,
    k: u64,
    song_done_at: Option<f64>,
    voices: Vec<Voice>,
    strips: Vec<Strip>,
    chl: Vec<Vec<f32>>,
    chr: Vec<Vec<f32>>,
    quiet: Vec<u32>,
    tl: Vec<f32>,
    tr: Vec<f32>,
    mixl: Vec<f32>,
    mixr: Vec<f32>,
    brev: Vec<f32>,
    bdly: Vec<f32>,
    bspc: Vec<f32>,
    peaks: Vec<f32>,
    prev_g: Vec<[f32; 5]>,
    prev_master: f32,
    verb: Verb,
    space: Space,
    delay: Delay,
    meq: Eq6,
    mcomp: Comp,
    limiter: Limiter,
    samples: Samples,
    files: Files,
    vinyl: Vinyl,
    /// nivel actual del vinilo (sube al reproducir y se apaga suave al detener)
    vinyl_g: f32,
    metro: bool,
    clicks: Vec<Click>,
    preview_n: u32,
    status: Arc<Status>,
    pub offline: bool,
    /// stems: sin la cadena del master (EQ, compresor, plugins, limitador, vinilo)
    pub bypass_master: bool,
    lufs: Lufs,
    lufs_blocks: usize,
    inserts: HashMap<u32, Vec<Box<dyn Proc>>>,
    insts: HashMap<u32, Box<dyn Proc>>,
    pev: Vec<(u32, u64, Ev)>,
    evbuf: Vec<Ev>,
    clock: u64,
    trash: Option<Sender<Box<dyn Proc>>>,
    prev_voice: Option<Voice>,
    trigs: Vec<Trig>,
    auto_now: Vec<(AutoTarget, f32, u64)>,
    mon_buf: Vec<f32>,
    mon_last: f32,
    mon_frac: f64,
    /// cuadros que pide la tarjeta de sonido en cada llamada (el colchón de la escucha debe cubrirlos)
    mon_cb: usize,
    mon_listo: bool,
    mon_tune: crate::fx::TuneLive,
    mon_tune_on: bool,
    /// nivelador de la voz (envolvente y ganancia) y reverberación de la escucha
    mon_env: f32,
    mon_agc: f32,
    mon_verb: crate::fx::Verb,
}

impl Engine {
    pub fn new(sr: f32, proj: Project, status: Arc<Status>) -> Engine {
        let buf = || vec![0f32; BLOCK];
        let mut e = Engine {
            sr,
            proj,
            mode: Mode::Song,
            playing: false,
            pos: 0.0,
            start: 0,
            k: 0,
            song_done_at: None,
            voices: Vec::with_capacity(1024),
            strips: vec![],
            chl: (0..MAX_CH).map(|_| buf()).collect(),
            chr: (0..MAX_CH).map(|_| buf()).collect(),
            quiet: vec![u32::MAX; MAX_CH],
            tl: buf(),
            tr: buf(),
            mixl: buf(),
            mixr: buf(),
            brev: buf(),
            bdly: buf(),
            bspc: buf(),
            peaks: vec![0.0; MAX_CH],
            prev_g: vec![[0.0; 5]; MAX_CH],
            prev_master: -1.0,
            verb: Verb::new(sr),
            space: Space::new(sr),
            delay: Delay::new(sr),
            meq: Eq6::default(),
            mcomp: Comp::default(),
            limiter: Limiter::default(),
            samples: HashMap::new(),
            files: HashMap::new(),
            vinyl: Vinyl::new(sr),
            vinyl_g: 0.0,
            metro: false,
            clicks: Vec::with_capacity(16),
            preview_n: 0,
            status,
            offline: false,
            bypass_master: false,
            lufs: Lufs::new(sr),
            lufs_blocks: 0,
            inserts: HashMap::new(),
            insts: HashMap::new(),
            pev: Vec::with_capacity(512),
            evbuf: Vec::with_capacity(256),
            clock: 0,
            trash: None,
            prev_voice: None,
            trigs: Vec::with_capacity(256),
            auto_now: Vec::with_capacity(64),
            mon_buf: Vec::with_capacity(4096),
            mon_last: 0.0,
            mon_frac: 0.0,
            mon_cb: BLOCK,
            mon_listo: false,
            mon_tune: crate::fx::TuneLive::new(sr),
            mon_tune_on: false,
            mon_env: 0.0,
            mon_agc: 1.0,
            mon_verb: crate::fx::Verb::new(sr),
        };
        e.sync_strips();
        e
    }

    pub fn set_bank(&mut self, b: &Bank) {
        self.samples = b.takes.clone();
        self.files = b.files.clone();
    }
    /// Los plugins retirados se devuelven por aquí para destruirlos fuera del hilo de audio.
    pub fn set_trash(&mut self, tx: Sender<Box<dyn Proc>>) {
        self.trash = Some(tx);
    }
    pub fn set_inserts(&mut self, ch: u32, procs: Vec<Box<dyn Proc>>) {
        if let Some(old) = self.inserts.insert(ch, procs) {
            self.discard(old);
        }
    }
    pub fn set_instrument(&mut self, ch: u32, proc: Option<Box<dyn Proc>>) {
        let old = match proc {
            Some(p) => self.insts.insert(ch, p),
            None => self.insts.remove(&ch),
        };
        if let Some(o) = old {
            self.discard(vec![o]);
        }
    }
    fn discard(&mut self, v: Vec<Box<dyn Proc>>) {
        for mut p in v {
            p.stop();
            match &self.trash {
                Some(tx) => {
                    let _ = tx.send(p);
                }
                None => drop(p),
            }
        }
    }

    fn sps(&self) -> f64 {
        self.proj.step_secs() * self.sr as f64
    }

    fn sync_strips(&mut self) {
        let ids: Vec<u32> = self.proj.channels.iter().map(|c| c.id).collect();
        if self.strips.iter().map(|s| s.id).ne(ids.iter().copied()) {
            let mut old = std::mem::take(&mut self.strips);
            let old_g = self.prev_g.clone();
            let mut newg = vec![[0.0; 5]; MAX_CH];
            self.strips = ids
                .iter()
                .enumerate()
                .map(|(ni, &id)| match old.iter().position(|s| s.id == id) {
                    Some(i) => {
                        if i < MAX_CH && ni < MAX_CH {
                            newg[ni] = old_g[i];
                        }
                        old.swap_remove(i)
                    }
                    None => Strip::new(id),
                })
                .collect();
            self.prev_g = newg;
            // los índices de canal cambiaron: las voces activas ya no apuntan bien
            self.voices.clear();
            // plugins de canales que ya no existen
            let gone: Vec<u32> = self.inserts.keys().copied().filter(|k| *k != 0 && !ids.contains(k)).collect();
            for g in gone {
                if let Some(v) = self.inserts.remove(&g) {
                    self.discard(v);
                }
            }
            let gone: Vec<u32> = self.insts.keys().copied().filter(|k| !ids.contains(k)).collect();
            for g in gone {
                if let Some(v) = self.insts.remove(&g) {
                    self.discard(vec![v]);
                }
            }
        }
    }

    pub fn handle(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::Project(p) => {
                let old_sps = self.sps();
                self.proj = *p;
                let new_sps = self.sps();
                if (old_sps - new_sps).abs() > 1e-9 {
                    let steps = self.pos / old_sps;
                    self.pos = steps * new_sps;
                }
                self.sync_strips();
            }
            Cmd::Play { mode, from_step } => self.start(mode, from_step),
            Cmd::Stop => {
                self.status.monitor.acorde.store(0, Ordering::Relaxed);
                self.playing = false;
                self.status.playing.store(false, Ordering::Relaxed);
                self.voices.retain(|v| v.held || !matches!(v.kind, Inst::Rhodes | Inst::Pad | Inst::Bass));
                for v in self.voices.iter_mut() {
                    if !v.held && matches!(v.kind, Inst::Audio | Inst::Vocal | Inst::Sampler) {
                        v.kill();
                    }
                }
                // plugins: se descartan las notas pendientes y se sueltan las que suenan
                let now = self.clock;
                self.pev.retain(|e| !e.2.on);
                for e in self.pev.iter_mut() {
                    e.1 = now;
                }
            }
            Cmd::SetMode(m) => self.mode = m,
            Cmd::Sample { pat, ch, data } => match data {
                Some(d) => {
                    self.samples.insert((pat, ch), d);
                }
                None => {
                    self.samples.remove(&(pat, ch));
                }
            },
            Cmd::File { key, buf } => match buf {
                Some(b) => {
                    self.files.insert(key, b);
                }
                None => {
                    self.files.remove(&key);
                }
            },
            Cmd::PreviewFile(b) => {
                self.prev_voice = b.map(|b| {
                    let n = b.frames() as f64;
                    let st = SmpState { rate: b.sr as f64 / self.sr as f64, buf: b, pos: 0.0, lo: 0.0, hi: n, gain: 0.8, att: 0.0, rel: 0.01, fade_in: 0.0, fade_out: 0.0, t0: 0.0, dur: 0.0 };
                    Voice::file(0, Inst::Audio, 0, 1.0, st, 600.0, 600.0, self.sr)
                });
            }
            Cmd::Metronome(b) => self.metro = b,
            Cmd::Preview { ch, m, v } => self.live(ch, m, v, false),
            Cmd::NoteOn { ch, m, v } => self.live(ch, m, v, true),
            Cmd::NoteOff { ch, m } => {
                if let Some(ci) = self.proj.channels.iter().position(|c| c.id == ch) {
                    for v in self.voices.iter_mut().filter(|v| v.ch == ci && v.held && v.note == m) {
                        v.release();
                    }
                    if self.proj.channels[ci].inst == Inst::Plugin {
                        let at = self.clock;
                        self.pev.push((ch, at, Ev { at: 0, on: false, note: m, vel: 0.0 }));
                    }
                }
            }
            Cmd::Inserts { ch, procs } => self.set_inserts(ch, procs),
            Cmd::Instrument { ch, proc } => self.set_instrument(ch, proc),
        }
    }

    /// Nota tocada desde la interfaz o un teclado MIDI.
    fn live(&mut self, ch: u32, m: u8, v: f32, held: bool) {
        let Some(ci) = self.proj.channels.iter().position(|c| c.id == ch) else { return };
        self.preview_n = self.preview_n.wrapping_add(1);
        let seed = hash(self.preview_n, 99);
        let len = if held { 600.0 } else { 0.45 };
        if self.voices.len() >= 900 {
            return;
        }
        let before = self.voices.len();
        self.dispatch(Trig { tg: 1.0, tp: 0.0, ci, m: m as f32, v, len, delay: 0, seed });
        if held {
            for x in self.voices[before..].iter_mut() {
                x.held = true;
                x.note = m;
            }
        }
    }

    pub fn start(&mut self, mode: Mode, from_step: u32) {
        self.mode = mode;
        self.playing = true;
        self.start = from_step as u64;
        self.k = 0;
        self.pos = 0.0;
        self.song_done_at = None;
        self.lufs.reset();
        self.lufs_blocks = 0;
        for v in self.voices.iter_mut() {
            if v.is_file() && !v.held {
                v.kill();
            }
        }
        self.status.playing.store(true, Ordering::Relaxed);
    }

    pub fn finished(&self) -> bool {
        !self.playing && self.voices.is_empty()
    }

    fn looping(&self) -> Option<(u64, u64)> {
        if self.mode != Mode::Song || self.offline {
            return None;
        }
        self.proj.loop_steps().map(|(a, b)| (a as u64, b as u64)).filter(|&(_, b)| self.start < b)
    }

    /// Paso de la canción para el paso k desde el inicio (con el bucle aplicado).
    fn song_step(&self, k: u64) -> u64 {
        let s = self.start + k;
        match self.looping() {
            Some((a, b)) if s >= b => a + (s - b) % (b - a),
            _ => s,
        }
    }
    /// Posición actual (pasos, fraccionaria) para la interfaz y la automatización.
    fn song_pos(&self) -> f64 {
        let s = self.start as f64 + self.pos / self.sps();
        match self.looping() {
            Some((a, b)) if s >= b as f64 => a as f64 + (s - b as f64).rem_euclid((b - a) as f64),
            _ => s,
        }
    }

    fn swing(&self, l: u32, sps: f64) -> f64 {
        let sw = self.proj.swing as f64;
        if self.proj.swing8 {
            if l % 4 == 2 { sps * sw * 1.33 } else { 0.0 }
        } else if l % 2 == 1 {
            sps * sw
        } else {
            0.0
        }
    }

    fn trigger_step(&mut self, k: u64, base_delay: f64) {
        let sps = self.sps();
        let sr = self.sr;
        let s = self.song_step(k);
        let wrapped = k > 0 && self.song_step(k - 1) + 1 != s;
        let jump = k == 0 || wrapped;
        if wrapped {
            for v in self.voices.iter_mut() {
                if v.kind == Inst::Audio && !v.held {
                    v.kill();
                }
            }
        }
        let any_solo = self.proj.any_solo();
        let mut events: [(usize, u32, f32, f32); 32] = [(0, 0, 1.0, 0.0); 32];
        let mut ne = 0;
        match self.mode {
            Mode::Pattern(id) => {
                if let Some(pi) = self.proj.patterns.iter().position(|p| p.id == id) {
                    let len = self.proj.patterns[pi].len_steps() as u64;
                    events[0] = (pi, (s % len) as u32, 1.0, 0.0);
                    ne = 1;
                }
            }
            Mode::Song => {
                for cl in &self.proj.playlist {
                    if let Some(pi) = self.proj.patterns.iter().position(|p| p.id == cl.pat) {
                        let s0 = cl.bar as u64 * STEPS_PER_BAR as u64;
                        let len = self.proj.patterns[pi].len_steps() as u64;
                        if s >= s0 && s < s0 + len && ne < events.len() {
                            if let Some((g, pn)) = self.proj.pista_suena(cl.track as usize) {
                                events[ne] = (pi, (s - s0) as u32, g, pn);
                                ne += 1;
                            }
                        }
                    }
                }
            }
        }
        let pat_id = match self.mode {
            Mode::Pattern(id) => Some(id),
            Mode::Song => None,
        };
        let acorde = self.proj.acorde_en(pat_id, s);
        self.status.monitor.acorde.store(acorde as u32, Ordering::Relaxed);
        let mut trigs = std::mem::take(&mut self.trigs);
        trigs.clear();
        for &(pi, l, tg, tp) in &events[..ne] {
            let t0 = trigs.len();
            let sw = self.swing(l, sps);
            let pat = &self.proj.patterns[pi];
            for (ci, c) in self.proj.channels.iter().enumerate() {
                if c.mute || (any_solo && !c.solo) {
                    // un canal que no suena igual dispara el sidechain de los que sí suenan
                    let feeds = self.proj.channels.iter().any(|o| o.comp.on && o.comp.sc == c.id && !(o.mute || (any_solo && !o.solo)));
                    if !feeds {
                        continue;
                    }
                }
                let seed = hash(s as u32, c.id.wrapping_mul(7919));
                if c.inst == Inst::Vocal {
                    if l == 0 {
                        if let Some((data, dsr)) = self.samples.get(&(pat.id, c.id)) {
                            let max_len = (pat.len_steps() as f64 * sps / sr as f64) as f32;
                            self.voices.push(Voice::sample(ci, (base_delay + sw) as u32, data.clone(), *dsr, max_len, sr));
                        }
                    }
                    continue;
                }
                if c.inst.uses_steps() {
                    if let Some(&vel) = pat.steps.get(&c.id).and_then(|a| a.get(l as usize)) {
                        if vel > 0.0 {
                            let r = (seed >> 8) as f32 / (1u32 << 24) as f32;
                            let human = if c.inst.is_drum() { (r - 0.5) as f64 * 0.006 * sr as f64 } else { 0.0 };
                            let d = (base_delay + sw + human).max(0.0) as u32;
                            let v = if c.inst.is_drum() { vel * (0.92 + r * 0.16) } else { vel };
                            let m = if c.inst == Inst::Sampler { c.smp.root as f32 } else { 60.0 };
                            trigs.push(Trig { tg: 1.0, tp: 0.0, ci, m, v, len: sps as f32 / sr, delay: d, seed });
                        }
                    }
                }
                if !c.inst.melodic() {
                    continue;
                }
                let Some(ns) = pat.notes.get(&c.id) else { continue };
                if c.arp.on {
                    // arpegiador: recorre las notas que están sonando en este paso
                    let rate = c.arp.rate.max(1);
                    if l % rate != 0 {
                        continue;
                    }
                    let mut held: [(u8, f32, u32); 24] = [(0, 0.0, 0); 24];
                    let mut kk = 0;
                    for x in ns.iter().filter(|x| x.s <= l && l < x.s + x.l) {
                        if kk < held.len() {
                            held[kk] = (x.m, x.v, x.s);
                            kk += 1;
                        }
                    }
                    if kk == 0 {
                        continue;
                    }
                    held[..kk].sort_by_key(|x| x.0);
                    let first = held[..kk].iter().map(|x| x.2).min().unwrap_or(0);
                    let oct = c.arp.oct.clamp(1, 4) as usize;
                    let n = kk * oct;
                    let idx = ((l - first) / rate) as usize;
                    let note_at = |i: usize| -> (f32, f32) {
                        let h = held[i % kk];
                        (h.0 as f32 + 12.0 * (i / kk) as f32, h.1)
                    };
                    let glen = (rate as f32 * c.arp.gate.clamp(0.05, 1.0)) * sps as f32 / sr;
                    let d = (base_delay + sw).max(0.0) as u32;
                    if c.arp.mode == 4 {
                        for i in 0..kk {
                            let (m, v) = note_at(i);
                            trigs.push(Trig { tg: 1.0, tp: 0.0, ci, m, v, len: glen, delay: d + (i as f64 * c.strum as f64 * sr as f64) as u32, seed: hash(seed, i as u32) });
                        }
                    } else {
                        let i = match c.arp.mode {
                            1 => n - 1 - idx % n,
                            2 => {
                                let p = (2 * n).saturating_sub(2).max(1);
                                let j = idx % p;
                                if j < n { j } else { p - j }
                            }
                            3 => (hash(idx as u32, c.id) as usize) % n,
                            _ => idx % n,
                        };
                        let (m, v) = note_at(i);
                        trigs.push(Trig { tg: 1.0, tp: 0.0, ci, m, v, len: glen, delay: d, seed });
                    }
                    continue;
                }
                let mut on: [(u8, u32, f32, f32); 24] = [(0, 0, 0.0, 0.0); 24];
                let mut kk = 0;
                for x in ns.iter().filter(|x| x.s == l) {
                    if kk < on.len() {
                        on[kk] = (x.m, x.l, x.v, x.o);
                        kk += 1;
                    }
                }
                on[..kk].sort_by_key(|x| x.0);
                for (i, &(m, len, v, o)) in on[..kk].iter().enumerate() {
                    let d = (base_delay + sw + o.clamp(0.0, 0.99) as f64 * sps + i as f64 * c.strum as f64 * sr as f64).max(0.0) as u32;
                    let len_s = len as f32 * sps as f32 / sr;
                    trigs.push(Trig { tg: 1.0, tp: 0.0, ci, m: m as f32, v, len: len_s, delay: d, seed: hash(seed, i as u32) });
                }
            }
            // volumen y paneo de la pista de la playlist
            for t in &mut trigs[t0..] {
                t.v *= tg;
                t.tp = tp;
            }
        }
        for t in trigs.iter() {
            let n0 = self.voices.len();
            self.dispatch(*t);
            if t.tp != 0.0 {
                for v in &mut self.voices[n0..] {
                    v.pan = t.tp;
                }
            }
        }
        self.trigs = trigs;
        // clips de audio de la playlist
        if self.mode == Mode::Song {
            let n = self.proj.audio_clips.len();
            for i in 0..n {
                let cl = &self.proj.audio_clips[i];
                let (cs, ce) = (cl.start as u64, (cl.start + cl.len) as u64);
                let inside = s >= cs && s < ce;
                if !inside || !(s == cs || jump) {
                    continue;
                }
                let Some(ci) = self.proj.channels.iter().position(|c| c.id == cl.ch) else { continue };
                let c = &self.proj.channels[ci];
                if c.mute || (any_solo && !c.solo) {
                    continue;
                }
                let Some((tg, tp)) = self.proj.pista_suena(cl.track as usize) else { continue };
                if let Some(mut v) = self.clip_voice(ci, i, (s - cs) as f64, base_delay as u32, tg) {
                    v.pan = tp;
                    self.voices.push(v);
                }
            }
        }
        if self.metro && !self.offline && s % 4 == 0 {
            self.clicks.push(Click { delay: base_delay as u32, t: 0.0, f: if s % 16 == 0 { 1600.0 } else { 1050.0 }, a: if s % 16 == 0 { 0.35 } else { 0.2 } });
        }
        if self.voices.len() > 900 {
            let extra = self.voices.len() - 900;
            self.voices.drain(..extra);
        }
    }

    /// Crea la voz (o el evento de plugin) de una nota.
    fn dispatch(&mut self, t: Trig) {
        let sr = self.sr;
        let c = &self.proj.channels[t.ci];
        match c.inst {
            Inst::Kick | Inst::Snare | Inst::Clap | Inst::Rim | Inst::Hat | Inst::OHat => {
                self.voices.push(Voice::new(t.ci, c.inst, t.delay, t.v, 60.0, 0.0, c.tune, c.decay, t.seed, sr));
            }
            Inst::Rhodes | Inst::Pad | Inst::Bass | Inst::Bell => {
                self.voices.push(Voice::new(t.ci, c.inst, t.delay, t.v, t.m + c.tune, t.len, c.tune, c.decay, t.seed, sr));
            }
            Inst::Synth => self.voices.push(Voice::synth(t.ci, t.delay, t.v, t.m + c.tune, t.len, c.synth, t.seed, sr)),
            Inst::Sampler => {
                if let Some(v) = self.sampler_voice(t.ci, t.m, t.v, t.len, t.delay) {
                    self.voices.push(v);
                }
            }
            Inst::Plugin => {
                let id = c.id;
                let m = (t.m + c.tune).round().clamp(0.0, 127.0) as u8;
                let at = self.clock + t.delay as u64;
                self.pev.push((id, at, Ev { at: 0, on: true, note: m, vel: t.v }));
                if t.len < 500.0 {
                    self.pev.push((id, at + (t.len * sr).max(1.0) as u64, Ev { at: 0, on: false, note: m, vel: 0.0 }));
                }
            }
            Inst::Vocal | Inst::Audio => {}
        }
    }

    fn sampler_voice(&self, ci: usize, m: f32, vel: f32, len: f32, delay: u32) -> Option<Voice> {
        let c = &self.proj.channels[ci];
        let sp = &c.smp;
        if sp.file.is_empty() {
            return None;
        }
        let ratio = sp.ratio(self.proj.bpm);
        let key = proc_key(&sp.file, ratio, sp.pitch, false);
        let (buf, r) = match self.files.get(&key) {
            Some(b) => (b.clone(), ratio),
            None => (self.files.get(&sp.file)?.clone(), 1.0),
        };
        let n = buf.frames() as f64;
        let base_rate = buf.sr as f64 / self.sr as f64;
        let (lo, hi, rate) = if sp.slice && !sp.slices.is_empty() {
            let i = m.round() as i32 - sp.root as i32;
            if i < 0 || i as usize >= sp.slices.len() {
                return None;
            }
            let i = i as usize;
            let a = (sp.slices[i] * r) as f64 * buf.sr as f64;
            let b = sp.slices.get(i + 1).map(|x| (*x * r) as f64 * buf.sr as f64).unwrap_or(n);
            (a.clamp(0.0, n), b.clamp(0.0, n), base_rate * 2f64.powf(c.tune as f64 / 12.0))
        } else {
            let a = (sp.start.clamp(0.0, 1.0) as f64 * n).floor();
            let b = (sp.end.clamp(0.0, 1.0) as f64 * n).max(a + 1.0);
            (a, b, base_rate * 2f64.powf((m - sp.root as f32 + c.tune) as f64 / 12.0))
        };
        if hi - lo < 2.0 {
            return None;
        }
        let (pos, rate) = if sp.reverse { (hi - 1.0, -rate) } else { (lo, rate) };
        let st = SmpState { buf, pos, rate, lo, hi, gain: sp.gain, att: sp.attack, rel: sp.release.max(0.003), fade_in: 0.0, fade_out: 0.0, t0: 0.0, dur: 0.0 };
        let note_len = if sp.oneshot && len < 500.0 { 1e6 } else { len };
        let mut v = Voice::file(ci, Inst::Sampler, delay, vel, st, note_len, 120.0, self.sr);
        v.note = m.round().clamp(0.0, 127.0) as u8;
        Some(v)
    }

    /// Voz de un clip de audio empezando `off_steps` pasos dentro del clip.
    fn clip_voice(&self, ci: usize, idx: usize, off_steps: f64, delay: u32, vol: f32) -> Option<Voice> {
        let cl = &self.proj.audio_clips[idx];
        let file = cl.file();
        let ratio = cl.ratio(self.proj.bpm);
        let key = proc_key(file, ratio, cl.pitch, cl.reverse);
        let (buf, r) = match self.files.get(&key) {
            Some(b) => (b.clone(), ratio),
            None => (self.files.get(file)?.clone(), 1.0),
        };
        let sps = self.sps();
        let clip_t = (off_steps * sps) as f32 / self.sr;
        let dur = (cl.len as f64 * sps) as f32 / self.sr;
        let src_t = cl.off * r + clip_t;
        let n = buf.frames() as f64;
        let pos = src_t as f64 * buf.sr as f64;
        if pos >= n {
            return None;
        }
        let rest = dur - clip_t;
        let st = SmpState { rate: buf.sr as f64 / self.sr as f64, buf, pos, lo: 0.0, hi: n, gain: cl.gain, att: 0.0, rel: 0.004, fade_in: cl.fade_in, fade_out: cl.fade_out, t0: clip_t, dur };
        Some(Voice::file(ci, Inst::Audio, delay, vol, st, rest, rest, self.sr))
    }

    /// Suma la voz del micrófono a la salida. Mantiene un pequeño colchón de audio y ajusta la
    /// velocidad de lectura (±0,5 %) para que los relojes del micrófono y la salida no se separen.
    fn mix_monitor(&mut self, out: &mut [f32]) {
        let m = &self.status.monitor;
        if self.offline || !m.on.load(Ordering::Relaxed) {
            self.mon_frac = 0.0;
            self.mon_listo = false;
            return;
        }
        let frames = out.len() / 2;
        let sr_in = m.sr.load(Ordering::Relaxed).max(1) as f64;
        // 15 ms de colchón más lo que se lleva de una vez cada llamada de la tarjeta (con bloques
        // grandes, 1024 o 2048, se consumen varios bloques seguidos y un colchón fijo se vacía)
        let objetivo = sr_in * 0.015 + self.mon_cb.saturating_sub(BLOCK) as f64 * sr_in / self.sr as f64;
        let gain = f32::from_bits(m.gain.load(Ordering::Relaxed));
        let Ok(mut q) = m.cola.try_lock() else { return };
        let hay = q.len() as f64;
        if !self.mon_listo {
            if hay < objetivo {
                return;
            }
            self.mon_listo = true;
        }
        // demasiado atraso acumulado: se recorta hasta el colchón
        while q.len() as f64 > objetivo * 4.0 {
            q.pop_front();
        }
        let hay = q.len() as f64;
        let ajuste = ((hay - objetivo) / objetivo).clamp(-1.0, 1.0) * 0.005;
        let ratio = sr_in / self.sr as f64 * (1.0 + ajuste);
        let fin = self.mon_frac + frames as f64 * ratio;
        let take = fin.floor() as usize;
        if q.len() < take {
            // se quedó sin audio: se apaga suave y se vuelve a llenar el colchón
            let last = self.mon_last;
            for f in 0..frames {
                let v = last * (1.0 - (f + 1) as f32 / frames as f32) * gain;
                out[f * 2] += v;
                out[f * 2 + 1] += v;
            }
            self.mon_last = 0.0;
            self.mon_frac = 0.0;
            self.mon_listo = false;
            return;
        }
        self.mon_buf.clear();
        self.mon_buf.push(self.mon_last);
        self.mon_buf.extend(q.drain(..take));
        drop(q);
        let tune_on = m.tune_on.load(Ordering::Relaxed);
        let acorde = m.acorde.load(Ordering::Relaxed) as u16;
        let mascara = if m.seguir.load(Ordering::Relaxed) && acorde.count_ones() >= 3 { acorde as u32 } else { m.tune_mask.load(Ordering::Relaxed) };
        if tune_on && !self.mon_tune_on {
            self.mon_tune.reset();
        }
        self.mon_tune_on = tune_on;
        let tp = TuneP {
            on: true,
            key: m.tune_key.load(Ordering::Relaxed) as u8,
            scale: m.tune_scale.load(Ordering::Relaxed) as u8,
            speed: f32::from_bits(m.tune_speed.load(Ordering::Relaxed)),
            mix: 1.0,
            gain: 1.0,
            mask: mascara as u16,
            estilo: m.tune_estilo.load(Ordering::Relaxed) as u8,
            algoritmo: m.tune_alg.load(Ordering::Relaxed) as u8,
            mezcla: f32::from_bits(m.tune_mezcla.load(Ordering::Relaxed)),
        };
        let sr = self.sr;
        let at_env = 1.0 - (-1.0 / (0.004 * sr)).exp();
        let rel_env = 1.0 - (-1.0 / (0.25 * sr)).exp();
        let at_agc = 1.0 - (-1.0 / (0.01 * sr)).exp();
        let rel_agc = 1.0 - (-1.0 / (0.4 * sr)).exp();
        let pan = f32::from_bits(m.pan.load(Ordering::Relaxed)).clamp(-1.0, 1.0);
        let ang = (pan + 1.0) * std::f32::consts::FRAC_PI_4;
        let (pl, pr) = (ang.cos() * std::f32::consts::SQRT_2, ang.sin() * std::f32::consts::SQRT_2);
        let verb_ret = f32::from_bits(m.reverb.load(Ordering::Relaxed)).clamp(0.0, 1.0);
        let vp = crate::project::VerbP { size: 0.55, decay: 0.5, damp: 0.35, predelay_ms: 25.0, width: 1.0, lowcut: 250.0, ret: verb_ret * 0.9 };
        for f in 0..frames {
            let p = self.mon_frac + f as f64 * ratio;
            let i = p as usize;
            let fr = (p - i as f64) as f32;
            let a = self.mon_buf[i.min(take)];
            let b = self.mon_buf[(i + 1).min(take)];
            let mut v = a + (b - a) * fr;
            if tune_on {
                v = self.mon_tune.process(v, &tp);
            } else {
                self.mon_tune.escuchar(v, &tp);
            }
            // nivelador: sube la voz a un nivel fuerte y parejo (no sube el ruido de cuando no cantas)
            let a = v.abs();
            self.mon_env += (a - self.mon_env) * if a > self.mon_env { at_env } else { rel_env };
            let deseo = if self.mon_env > 0.004 { (0.42 / self.mon_env).clamp(1.0, 8.0) } else { 1.0 };
            self.mon_agc += (deseo - self.mon_agc) * if deseo < self.mon_agc { at_agc } else { rel_agc };
            let v = v * self.mon_agc * gain;
            let (rl, rr) = if verb_ret > 0.001 { self.mon_verb.run(v, &vp) } else { (0.0, 0.0) };
            let lim = |x: f32| {
                let a = x.abs();
                if a < 0.8 { x } else { x.signum() * (0.8 + 0.2 * ((a - 0.8) / 0.2).tanh()) }
            };
            out[f * 2] += lim(v * pl + rl);
            out[f * 2 + 1] += lim(v * pr + rr);
        }
        self.mon_last = self.mon_buf[take];
        self.mon_frac = fin - take as f64;
        m.cantada.store(self.mon_tune.cantada.to_bits(), Ordering::Relaxed);
        m.objetivo.store((self.mon_tune.objetivo + 1).max(0) as u32, Ordering::Relaxed);
    }

    /// Valores de automatización en la posición actual (solo en modo canción).
    fn update_auto(&mut self) {
        self.auto_now.clear();
        if !self.playing || self.mode != Mode::Song || self.proj.autos.is_empty() {
            return;
        }
        let pos = self.song_pos();
        for a in &self.proj.autos {
            let (s0, s1) = (a.start as f64, (a.start + a.len) as f64);
            if pos < s0 {
                continue;
            }
            let (v, rank) = if pos < s1 {
                (a.at(((pos - s0) / (s1 - s0).max(1.0)) as f32), u64::MAX / 2 + a.start as u64)
            } else {
                (a.at(1.0), s1 as u64)
            };
            match self.auto_now.iter_mut().find(|x| x.0 == a.target) {
                Some(x) => {
                    if rank >= x.2 {
                        x.1 = v;
                        x.2 = rank;
                    }
                }
                None => self.auto_now.push((a.target, v, rank)),
            }
        }
        // parámetros de plugins
        for &(t, v, _) in &self.auto_now {
            if let AutoTarget::Param(ch, uid, pid) = t {
                if let Some(p) = self.insts.get_mut(&ch).filter(|p| p.uid() == uid) {
                    p.set_param(pid, v as f64);
                }
                if let Some(list) = self.inserts.get_mut(&ch) {
                    for p in list.iter_mut().filter(|p| p.uid() == uid) {
                        p.set_param(pid, v as f64);
                    }
                }
            }
        }
    }
    fn auto(&self, t: AutoTarget) -> Option<f32> {
        self.auto_now.iter().find(|x| x.0 == t).map(|x| t.value(x.1))
    }

    /// Llena `out` (estéreo intercalado) con `frames` cuadros.
    pub fn render(&mut self, out: &mut [f32]) {
        let t0 = if self.offline { None } else { Some(std::time::Instant::now()) };
        let total = out.len() / 2;
        self.mon_cb = total.max(BLOCK);
        let mut done = 0;
        while done < total {
            let n = (total - done).min(BLOCK);
            self.render_block(&mut out[done * 2..(done + n) * 2]);
            done += n;
        }
        if self.offline {
            return;
        }
        // estado para la interfaz
        let st = &self.status;
        if let Some(t0) = t0 {
            let budget = total as f32 / self.sr;
            let l = (t0.elapsed().as_secs_f32() / budget.max(1e-6)).min(2.0);
            let prev = f32::from_bits(st.load.load(Ordering::Relaxed));
            st.load.store((l.max(prev * 0.95)).to_bits(), Ordering::Relaxed);
        }
        let sp = if self.playing { self.song_pos() } else { 0.0 };
        st.pos_steps.store(sp.to_bits(), Ordering::Relaxed);
        let nch = self.proj.channels.len().min(MAX_CH);
        for i in 0..nch {
            let p = self.peaks[i];
            let prev = f32::from_bits(st.meters[i].load(Ordering::Relaxed));
            st.meters[i].store(prev.max(p).to_bits(), Ordering::Relaxed);
            self.peaks[i] = 0.0;
        }
        if let Ok(mut sc) = st.scope.try_lock() {
            let n = sc.len();
            let f = total.min(n);
            sc.copy_within(f.., 0);
            for i in 0..f {
                let j = total - f + i;
                sc[n - f + i] = (out[j * 2] + out[j * 2 + 1]) * 0.5;
            }
        }
    }

    fn render_block(&mut self, out: &mut [f32]) {
        let frames = out.len() / 2;
        let sps = self.sps();
        let sr = self.sr;
        if self.playing {
            // se programa un poco por adelantado para que la humanización pueda adelantar golpes
            let end = self.pos + frames as f64 + LOOKAHEAD;
            let song_steps = (self.proj.song_bars() * STEPS_PER_BAR) as u64;
            let looping = self.looping().is_some();
            loop {
                let st = self.k as f64 * sps;
                if st >= end {
                    break;
                }
                if self.mode == Mode::Song && !looping && self.start + self.k >= song_steps {
                    if self.song_done_at.is_none() {
                        self.song_done_at = Some(st);
                    }
                    break;
                }
                let base = (st - self.pos).max(0.0);
                let k = self.k;
                self.trigger_step(k, base);
                self.k += 1;
            }
            if let Some(done) = self.song_done_at {
                // deja sonar las colas antes de parar
                if self.pos > done + 2.5 * sr as f64 {
                    self.playing = false;
                    self.status.playing.store(false, Ordering::Relaxed);
                }
            }
        }
        self.update_auto();

        let nch = self.proj.channels.len().min(MAX_CH);
        for i in 0..nch {
            self.chl[i][..frames].fill(0.0);
            self.chr[i][..frames].fill(0.0);
        }
        for b in [&mut self.mixl, &mut self.mixr, &mut self.brev, &mut self.bdly, &mut self.bspc] {
            b[..frames].fill(0.0);
        }
        // voces → canales
        for v in self.voices.iter_mut() {
            if v.ch >= nch {
                v.done = true;
                continue;
            }
            let (l, r) = (&mut self.chl[v.ch], &mut self.chr[v.ch]);
            // paneo de la pista (balance: al centro no cambia nada)
            let (gl, gr) = ((1.0 - v.pan).min(1.0), (1.0 + v.pan).min(1.0));
            for f in 0..frames {
                if v.done {
                    break;
                }
                let (a, b) = v.next2(sr);
                l[f] += a * gl;
                r[f] += b * gr;
            }
        }
        self.voices.retain(|v| !v.done);
        // instrumentos externos
        if !self.insts.is_empty() {
            let end = self.clock + frames as u64;
            for i in 0..nch {
                let id = self.proj.channels[i].id;
                let Some(p) = self.insts.get_mut(&id) else { continue };
                self.evbuf.clear();
                let clock = self.clock;
                let evbuf = &mut self.evbuf;
                self.pev.retain(|e| {
                    if e.0 == id && e.1 < end {
                        let mut ev = e.2;
                        ev.at = e.1.saturating_sub(clock) as u32;
                        evbuf.push(ev);
                        false
                    } else {
                        true
                    }
                });
                self.evbuf.sort_by_key(|e| (e.at, e.on));
                p.process(&mut self.chl[i][..frames], &mut self.chr[i][..frames], &self.evbuf);
            }
        }
        // eventos de plugins cuyo canal ya no tiene instrumento
        if self.pev.len() > 400 {
            let insts = &self.insts;
            self.pev.retain(|e| insts.contains_key(&e.0));
        }

        // canales: EQ rápida, filtro, NN EQ, NN Dynamics (sidechain), plugins, fader → buses
        let any_solo = self.proj.any_solo();
        let mut any_space = false;
        for i in 0..nch {
            let c = &self.proj.channels[i];
            let id = c.id;
            let (mut vol, mut pan, mut cut, mut rev, mut dly, mut spc) = (c.vol, c.pan, c.cut, c.rev, c.dly, c.space);
            if !self.auto_now.is_empty() {
                vol = self.auto(AutoTarget::Vol(id)).unwrap_or(vol);
                pan = self.auto(AutoTarget::Pan(id)).unwrap_or(pan);
                cut = self.auto(AutoTarget::Cut(id)).unwrap_or(cut);
                rev = self.auto(AutoTarget::Rev(id)).unwrap_or(rev);
                dly = self.auto(AutoTarget::Dly(id)).unwrap_or(dly);
                spc = self.auto(AutoTarget::Space(id)).unwrap_or(spc);
            }
            let audible = !(c.mute || (any_solo && !c.solo));
            let vol = if audible { vol } else { 0.0 };
            let (gl, gr) = pan_gains(pan);
            let target = [gl * vol, gr * vol, rev * vol, dly * vol, spc * vol];
            if spc > 0.0 {
                any_space = true;
            }
            let has_ins = self.inserts.get(&id).is_some_and(|v| !v.is_empty()) && c.inserts.iter().any(|s| s.on);
            let silent = self.chl[i][..frames].iter().all(|x| *x == 0.0) && self.chr[i][..frames].iter().all(|x| *x == 0.0);
            if silent {
                self.quiet[i] = self.quiet[i].saturating_add(frames as u32);
            } else {
                self.quiet[i] = 0;
            }
            if !has_ins && self.quiet[i] > (sr * 0.5) as u32 {
                self.prev_g[i] = target;
                continue;
            }
            let comp = c.comp;
            let cut_open = cut >= 19_900.0;
            let sc = if comp.on && comp.sc != 0 && comp.sc != id { self.proj.channels.iter().take(nch).position(|x| x.id == comp.sc) } else { None };
            let sc_vol = sc.map(|j| self.proj.channels[j].vol).unwrap_or(1.0);
            self.strips[i].update(c.lo, c.hi, cut, sr);
            self.strips[i].update_fx(&c.eq, sr);
            for f in 0..frames {
                let (l, r) = self.strips[i].run(self.chl[i][f], self.chr[i][f], cut_open);
                let key = sc.map(|j| self.chl[j][f].abs().max(self.chr[j][f].abs()) * sc_vol);
                let (l, r) = self.strips[i].run_fx(l, r, &comp, key, sr);
                self.tl[f] = l;
                self.tr[f] = r;
            }
            if has_ins {
                if let Some(list) = self.inserts.get_mut(&id) {
                    for slot in c.inserts.iter().filter(|s| s.on) {
                        if let Some(p) = list.iter_mut().find(|p| p.uid() == slot.uid) {
                            p.process(&mut self.tl[..frames], &mut self.tr[..frames], &[]);
                        }
                    }
                }
            }
            let prev = self.prev_g[i];
            let inv = 1.0 / frames as f32;
            let mut pk = self.peaks[i];
            for f in 0..frames {
                let a = (f + 1) as f32 * inv;
                let g = |k: usize| prev[k] + (target[k] - prev[k]) * a;
                let (l, r) = (self.tl[f], self.tr[f]);
                let m = (l + r) * 0.5;
                self.mixl[f] += l * g(0);
                self.mixr[f] += r * g(1);
                self.brev[f] += m * g(2);
                self.bdly[f] += m * g(3);
                self.bspc[f] += m * g(4);
                let v = l.abs().max(r.abs()) * vol;
                if v > pk {
                    pk = v;
                }
            }
            self.peaks[i] = pk;
            self.prev_g[i] = target;
        }

        // master
        let (verb_p, space_p, mcomp_p, lim_p) = (self.proj.verb, self.proj.space, self.proj.master_comp, self.proj.limiter);
        self.meq.set(&self.proj.master_eq, sr);
        let dly_samples = (sps * 3.0) as usize;
        // el vinilo es parte de la canción: solo suena mientras se reproduce
        let vinyl_to = if self.bypass_master || !self.playing { 0.0 } else { self.proj.vinyl * 0.6 };
        let vinyl_from = self.vinyl_g;
        let vinyl_step = (vinyl_to - vinyl_from) / frames as f32;
        self.vinyl_g = if (vinyl_to - vinyl_from).abs() < 1e-4 { vinyl_to } else { vinyl_from + (vinyl_to - vinyl_from) * 0.25 };
        let vinyl_on = vinyl_from > 0.0 || vinyl_to > 0.0;
        let mtarget = self.auto(AutoTarget::Master).unwrap_or(self.proj.master);
        let mprev = if self.prev_master < 0.0 { mtarget } else { self.prev_master };
        let inv = 1.0 / frames as f32;
        let bypass = self.bypass_master;
        for f in 0..frames {
            let (rl, rr) = self.verb.run(self.brev[f], &verb_p);
            let (dl, dr) = self.delay.run(self.bdly[f] * 0.8, self.bdly[f] * 0.8, dly_samples);
            let (sl, sr2) = if any_space { self.space.run(self.bspc[f], &space_p) } else { (0.0, 0.0) };
            let vn = if vinyl_on { self.vinyl.run() * (vinyl_from + vinyl_step * 0.25 * (f + 1) as f32) } else { 0.0 };
            let mut l = self.mixl[f] + rl + dl + sl + vn;
            let mut r = self.mixr[f] + rr + dr + sr2 + vn;
            if !bypass {
                let (el, er) = self.meq.stereo(l, r);
                let cg = self.mcomp.gain(el.abs().max(er.abs()), &mcomp_p, sr);
                l = el * cg;
                r = er * cg;
            }
            let mg = mprev + (mtarget - mprev) * (f + 1) as f32 * inv;
            self.mixl[f] = l * mg;
            self.mixr[f] = r * mg;
        }
        self.prev_master = mtarget;
        if !bypass && !self.proj.master_inserts.is_empty() {
            if let Some(list) = self.inserts.get_mut(&0) {
                for slot in self.proj.master_inserts.iter().filter(|s| s.on) {
                    if let Some(p) = list.iter_mut().find(|p| p.uid() == slot.uid) {
                        p.process(&mut self.mixl[..frames], &mut self.mixr[..frames], &[]);
                    }
                }
            }
        }
        let mut gr = [0f32; 2];
        let mut mpk = [0f32; 2];
        for f in 0..frames {
            let (mut ol, mut or) = if bypass { (self.mixl[f], self.mixr[f]) } else { self.limiter.run(self.mixl[f], self.mixr[f], &lim_p, sr) };
            gr[0] = gr[0].max(self.mcomp.gr_db);
            gr[1] = gr[1].max(self.limiter.gr_db);
            if !self.offline {
                self.lufs.push(ol, or);
            }
            mpk[0] = mpk[0].max(ol.abs());
            mpk[1] = mpk[1].max(or.abs());
            if let Some(v) = &mut self.prev_voice {
                let (a, b) = v.next2(sr);
                ol += a;
                or += b;
            }
            if !self.clicks.is_empty() {
                let dt = 1.0 / sr;
                for c in self.clicks.iter_mut() {
                    if c.delay > 0 {
                        c.delay -= 1;
                        continue;
                    }
                    let s = (c.t * c.f * std::f32::consts::TAU).sin() * c.a * (-c.t / 0.012).exp();
                    ol += s;
                    or += s;
                    c.t += dt;
                }
            }
            out[f * 2] = ol;
            out[f * 2 + 1] = or;
        }
        self.clicks.retain(|c| c.t < 0.08);
        self.mix_monitor(out);
        if self.prev_voice.as_ref().is_some_and(|v| v.done) {
            self.prev_voice = None;
        }
        if self.playing {
            self.pos += frames as f64;
        }
        self.clock += frames as u64;
        if !self.offline {
            let st = &self.status;
            for k in 0..2 {
                let prev = f32::from_bits(st.master[k].load(Ordering::Relaxed));
                st.master[k].store(prev.max(mpk[k]).to_bits(), Ordering::Relaxed);
                st.gr[k].store(gr[k].to_bits(), Ordering::Relaxed);
            }
            self.lufs_blocks += frames;
            if self.lufs_blocks >= (sr * 0.1) as usize {
                self.lufs_blocks = 0;
                st.lufs[0].store(self.lufs.momentary.to_bits(), Ordering::Relaxed);
                st.lufs[1].store(self.lufs.short.to_bits(), Ordering::Relaxed);
                if self.playing {
                    st.lufs[2].store(self.lufs.integrated().to_bits(), Ordering::Relaxed);
                }
                st.tpeak.store(self.lufs.true_peak.to_bits(), Ordering::Relaxed);
            }
        }
    }
}

/// Renderiza la canción completa (modo SONG) a estéreo intercalado.
pub fn render_song(proj: &Project, bank: &Bank, sr: f32, progress: &AtomicU32, cancel: &AtomicBool) -> Vec<f32> {
    render_song_with(proj, bank, sr, progress, cancel, &mut |_| {})
}

/// Igual que `render_song`, con una función para preparar el motor (plugins, stems…).
pub fn render_song_with(proj: &Project, bank: &Bank, sr: f32, progress: &AtomicU32, cancel: &AtomicBool, setup: &mut dyn FnMut(&mut Engine)) -> Vec<f32> {
    let mut e = Engine::new(sr, proj.clone(), Arc::new(Status::new()));
    e.set_bank(bank);
    e.offline = true;
    setup(&mut e);
    e.start(Mode::Song, 0);
    let steps = (proj.song_bars() * STEPS_PER_BAR) as f64;
    let total = (steps * e.sps() + 2.6 * sr as f64) as usize;
    let mut out = Vec::with_capacity(total * 2 + 2048);
    let mut buf = vec![0f32; 1024];
    while out.len() / 2 < total && !cancel.load(Ordering::Relaxed) {
        e.render(&mut buf);
        out.extend_from_slice(&buf);
        progress.store(((out.len() / 2) as f64 / total as f64 * 1000.0) as u32, Ordering::Relaxed);
    }
    out.truncate(total * 2);
    out
}
