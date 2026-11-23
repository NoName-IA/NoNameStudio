//! Modelo de datos del proyecto (se guarda como JSON).
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const STEPS_PER_BAR: u32 = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Inst {
    Kick,
    Snare,
    Clap,
    Rim,
    Hat,
    OHat,
    Rhodes,
    Pad,
    Bass,
    Bell,
    /// NN Wave: sintetizador de tablas de onda
    Synth,
    /// Pista de voz grabada con micrófono (NN Tune)
    Vocal,
    /// NN Sampler: sample propio (WAV/MP3/FLAC/OGG), también en modo slicer
    Sampler,
    /// Pista de audio: clips de audio en la playlist (grabaciones, loops)
    Audio,
    /// Instrumento externo (plugin CLAP)
    Plugin,
}

impl Inst {
    pub const ALL: [Inst; 14] = [
        Inst::Kick,
        Inst::Snare,
        Inst::Clap,
        Inst::Rim,
        Inst::Hat,
        Inst::OHat,
        Inst::Rhodes,
        Inst::Pad,
        Inst::Bass,
        Inst::Bell,
        Inst::Synth,
        Inst::Vocal,
        Inst::Sampler,
        Inst::Audio,
    ];
    pub fn is_drum(self) -> bool {
        matches!(self, Inst::Kick | Inst::Snare | Inst::Clap | Inst::Rim | Inst::Hat | Inst::OHat)
    }
    pub fn name(self) -> &'static str {
        match self {
            Inst::Kick => "Kick",
            Inst::Snare => "Snare",
            Inst::Clap => "Clap",
            Inst::Rim => "Rim",
            Inst::Hat => "Hat",
            Inst::OHat => "Open Hat",
            Inst::Rhodes => "Rhodes",
            Inst::Pad => "Pad",
            Inst::Bass => "Sub Bass",
            Inst::Bell => "Campana",
            Inst::Synth => "NN Wave",
            Inst::Vocal => "Voz (NN Tune)",
            Inst::Sampler => "NN Sampler",
            Inst::Audio => "Pista de audio",
            Inst::Plugin => "Plugin",
        }
    }
    /// Canales que se escriben con el piano roll.
    pub fn melodic(self) -> bool {
        !self.is_drum() && !matches!(self, Inst::Vocal | Inst::Audio)
    }
    /// Canales que usan la rejilla de pasos.
    pub fn uses_steps(self) -> bool {
        self.is_drum() || self == Inst::Sampler
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Channel {
    pub id: u32,
    pub inst: Inst,
    pub name: String,
    pub vol: f32,
    pub pan: f32,
    pub mute: bool,
    pub solo: bool,
    /// EQ de graves/agudos en dB
    pub lo: f32,
    pub hi: f32,
    /// Filtro pasa-bajos en Hz (20000 = abierto)
    pub cut: f32,
    /// Envíos a reverb y delay (0..1)
    pub rev: f32,
    pub dly: f32,
    /// Afinación en semitonos
    pub tune: f32,
    /// Multiplicador de decaimiento (percusión)
    pub decay: f32,
    /// Rasgueo entre notas de un acorde (segundos)
    pub strum: f32,
    /// Envío a NN Space (0..1)
    #[serde(default)]
    pub space: f32,
    /// NN EQ del canal
    #[serde(default = "default_eq")]
    pub eq: [Band; 6],
    /// Compresor del canal (NN Dynamics)
    #[serde(default = "CompP::channel")]
    pub comp: CompP,
    /// Parámetros de NN Wave (solo canales Synth)
    #[serde(default)]
    pub synth: SynthP,
    /// Parámetros de NN Tune (solo canales de voz)
    #[serde(default)]
    pub tunefx: TuneP,
    /// Parámetros de NN Sampler (solo canales Sampler)
    #[serde(default)]
    pub smp: SampleP,
    /// Arpegiador (canales melódicos)
    #[serde(default)]
    pub arp: ArpP,
    /// Plugins de efecto (CLAP) insertados después de NN Dynamics
    #[serde(default)]
    pub inserts: Vec<PluginSlot>,
    /// Instrumento externo (solo canales Plugin)
    #[serde(default)]
    pub instrument: Option<PluginSlot>,
}

impl Channel {
    pub fn new(id: u32, inst: Inst) -> Self {
        Channel {
            id,
            inst,
            name: inst.name().to_string(),
            vol: 0.75,
            pan: 0.0,
            mute: false,
            solo: false,
            lo: 0.0,
            hi: 0.0,
            cut: if inst == Inst::Rhodes { 2600.0 } else { 20000.0 },
            rev: 0.0,
            dly: 0.0,
            tune: 0.0,
            decay: 1.0,
            strum: if matches!(inst, Inst::Rhodes | Inst::Pad) { 0.015 } else { 0.0 },
            space: 0.0,
            eq: default_eq(),
            comp: CompP::channel(),
            synth: SynthP::default(),
            tunefx: TuneP::default(),
            smp: SampleP::default(),
            arp: ArpP::default(),
            inserts: vec![],
            instrument: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Note {
    /// paso de inicio (semicorcheas)
    pub s: u32,
    /// duración en pasos
    pub l: u32,
    /// nota MIDI
    pub m: u8,
    /// velocidad 0..1
    pub v: f32,
    /// desplazamiento fino dentro del paso (0..1): humanizar y grabar MIDI sin cuantizar
    #[serde(default)]
    pub o: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Pattern {
    pub id: u32,
    pub name: String,
    pub bars: u32,
    pub color: [u8; 3],
    /// pasos de percusión por canal (velocidad 0 = apagado)
    pub steps: HashMap<u32, Vec<f32>>,
    /// notas por canal melódico
    pub notes: HashMap<u32, Vec<Note>>,
    /// Tomas de audio por canal de voz (nombre del archivo WAV en la carpeta de datos)
    #[serde(default)]
    pub audio: HashMap<u32, String>,
}

impl Pattern {
    pub fn len_steps(&self) -> u32 {
        self.bars * STEPS_PER_BAR
    }
    pub fn set_bars(&mut self, bars: u32) {
        self.bars = bars.max(1);
        let n = self.len_steps() as usize;
        for a in self.steps.values_mut() {
            a.resize(n, 0.0);
        }
        let ls = self.len_steps();
        for ns in self.notes.values_mut() {
            ns.retain(|x| x.s < ls);
            for x in ns.iter_mut() {
                x.l = x.l.min(ls - x.s).max(1);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Clip {
    pub track: u32,
    pub bar: u32,
    pub pat: u32,
}

/// Ajustes de una pista de la playlist.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PistaP {
    pub mute: bool,
    pub solo: bool,
    pub vol: f32,
    pub pan: f32,
}
impl Default for PistaP {
    fn default() -> Self {
        PistaP { mute: false, solo: false, vol: 1.0, pan: 0.0 }
    }
}

/// Clip de audio en la playlist (pertenece a un canal «Pista de audio»).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AudioClip {
    pub id: u32,
    pub ch: u32,
    pub track: u32,
    /// inicio y duración en pasos (semicorcheas)
    pub start: u32,
    pub len: u32,
    /// desplazamiento dentro del archivo (segundos del audio original)
    pub off: f32,
    /// tomas grabadas o importadas; `active` es la que suena (comping)
    pub takes: Vec<String>,
    pub active: usize,
    pub gain: f32,
    /// cambio de tono en semitonos sin cambiar la duración
    pub pitch: f32,
    /// tempo original del audio; si es > 0 se estira al tempo del proyecto
    pub bpm: f32,
    pub fade_in: f32,
    pub fade_out: f32,
    #[serde(default)]
    pub reverse: bool,
}
impl AudioClip {
    pub fn file(&self) -> &str {
        self.takes.get(self.active).or(self.takes.first()).map(String::as_str).unwrap_or("")
    }
    /// factor de tiempo (duración nueva / duración original) a un tempo dado
    pub fn ratio(&self, bpm: f32) -> f32 {
        if self.bpm > 0.0 { (self.bpm / bpm.max(1.0)).clamp(0.25, 4.0) } else { 1.0 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AutoTarget {
    Vol(u32),
    Pan(u32),
    Cut(u32),
    Rev(u32),
    Dly(u32),
    Space(u32),
    Master,
    /// parámetro de un plugin: canal (0 = master), uid de la ranura, id del parámetro
    Param(u32, u32, u32),
}
impl AutoTarget {
    pub fn channel(self) -> Option<u32> {
        match self {
            AutoTarget::Vol(c) | AutoTarget::Pan(c) | AutoTarget::Cut(c) | AutoTarget::Rev(c) | AutoTarget::Dly(c) | AutoTarget::Space(c) => Some(c),
            AutoTarget::Param(c, _, _) if c != 0 => Some(c),
            _ => None,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            AutoTarget::Vol(_) => "Volumen",
            AutoTarget::Pan(_) => "Paneo",
            AutoTarget::Cut(_) => "Filtro",
            AutoTarget::Rev(_) => "Reverb",
            AutoTarget::Dly(_) => "Delay",
            AutoTarget::Space(_) => "Space",
            AutoTarget::Master => "Volumen master",
            AutoTarget::Param(..) => "Parámetro de plugin",
        }
    }
    /// convierte 0..1 al valor real del parámetro
    pub fn value(self, v: f32) -> f32 {
        let v = v.clamp(0.0, 1.0);
        match self {
            AutoTarget::Vol(_) | AutoTarget::Master => v * 1.2,
            AutoTarget::Pan(_) => v * 2.0 - 1.0,
            AutoTarget::Cut(_) => 200.0 * 100f32.powf(v),
            AutoTarget::Dly(_) => v * 0.8,
            _ => v,
        }
    }
    /// valor real → 0..1
    pub fn norm(self, x: f32) -> f32 {
        (match self {
            AutoTarget::Vol(_) | AutoTarget::Master => x / 1.2,
            AutoTarget::Pan(_) => (x + 1.0) / 2.0,
            AutoTarget::Cut(_) => (x.max(200.0) / 200.0).ln() / 100f32.ln(),
            AutoTarget::Dly(_) => x / 0.8,
            _ => x,
        })
        .clamp(0.0, 1.0)
    }
    pub fn fmt(self, v: f32) -> String {
        let x = self.value(v);
        match self {
            AutoTarget::Vol(_) | AutoTarget::Master => {
                if x > 0.0001 { format!("{:.1} dB", 20.0 * x.log10()) } else { "-∞".into() }
            }
            AutoTarget::Pan(_) => {
                if x.abs() < 0.01 { "Centro".into() } else if x < 0.0 { format!("Izq {:.0}%", -x * 100.0) } else { format!("Der {:.0}%", x * 100.0) }
            }
            AutoTarget::Cut(_) => {
                if x >= 19_900.0 { "abierto".into() } else { format!("{:.0} Hz", x) }
            }
            _ => format!("{:.0}%", v * 100.0),
        }
    }
}

/// Clip de automatización: curva de puntos (x 0..1 dentro del clip, y 0..1).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AutoClip {
    pub id: u32,
    pub track: u32,
    pub start: u32,
    pub len: u32,
    pub target: AutoTarget,
    pub pts: Vec<[f32; 2]>,
}
impl AutoClip {
    /// valor (0..1) en la posición relativa x (0..1)
    pub fn at(&self, x: f32) -> f32 {
        let p = &self.pts;
        if p.is_empty() {
            return 0.5;
        }
        if x <= p[0][0] {
            return p[0][1];
        }
        for w in p.windows(2) {
            if x <= w[1][0] {
                let d = (w[1][0] - w[0][0]).max(1e-6);
                return w[0][1] + (w[1][1] - w[0][1]) * ((x - w[0][0]) / d);
            }
        }
        p[p.len() - 1][1]
    }
}

pub const SCALES: [(&str, [bool; 12]); 11] = [
    ("Mayor", [true, false, true, false, true, true, false, true, false, true, false, true]),
    ("Menor natural", [true, false, true, true, false, true, false, true, true, false, true, false]),
    ("Menor armónica", [true, false, true, true, false, true, false, true, true, false, false, true]),
    ("Dórica", [true, false, true, true, false, true, false, true, false, true, true, false]),
    ("Frigia", [true, true, false, true, false, true, false, true, true, false, true, false]),
    ("Lidia", [true, false, true, false, true, false, true, true, false, true, false, true]),
    ("Mixolidia", [true, false, true, false, true, true, false, true, false, true, true, false]),
    ("Pentatónica menor", [true, false, false, true, false, true, false, true, false, false, true, false]),
    ("Pentatónica mayor", [true, false, true, false, true, false, false, true, false, true, false, false]),
    ("Blues", [true, false, false, true, false, true, true, true, false, false, true, false]),
    ("Cromática", [true; 12]),
];
/// Máscara de 12 bits con las notas de una escala (índice en SCALES) sobre una tónica.
pub fn scale_mask(key: u8, scale: u8) -> u16 {
    let sc = SCALES[(scale as usize).min(SCALES.len() - 1)].1;
    let mut m = 0u16;
    for (d, on) in sc.iter().enumerate() {
        if *on {
            m |= 1 << ((key as usize + d) % 12);
        }
    }
    m
}

/// Tonalidad más probable de las notas del proyecto (perfiles de Krumhansl). Devuelve (tónica, escala).
pub fn detect_key(p: &Project) -> Option<(u8, u8)> {
    let mut h = [0f32; 12];
    let mut total = 0.0;
    for pat in &p.patterns {
        for (ch, ns) in &pat.notes {
            if p.channel(*ch).is_some_and(|c| c.inst.is_drum()) {
                continue;
            }
            for n in ns {
                h[(n.m % 12) as usize] += n.l as f32;
                total += n.l as f32;
            }
        }
    }
    if total < 1.0 {
        return None;
    }
    let mayor = [6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88];
    let menor = [6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17];
    let corr = |perfil: &[f32; 12], k: usize| -> f32 {
        let mh = h.iter().sum::<f32>() / 12.0;
        let mp = perfil.iter().sum::<f32>() / 12.0;
        let (mut a, mut b, mut c) = (0.0, 0.0, 0.0);
        for i in 0..12 {
            let x = h[(i + k) % 12] - mh;
            let y = perfil[i] - mp;
            a += x * y;
            b += x * x;
            c += y * y;
        }
        a / (b * c).sqrt().max(1e-9)
    };
    let mut best = (0u8, 1u8, f32::MIN);
    for k in 0..12 {
        for (escala, perfil) in [(0u8, &mayor), (1u8, &menor)] {
            let r = corr(perfil, k);
            if r > best.2 {
                best = (k as u8, escala, r);
            }
        }
    }
    Some((best.0, best.1))
}

pub const KEY_NAMES: [&str; 12] = ["Do", "Do#", "Re", "Mib", "Mi", "Fa", "Fa#", "Sol", "Lab", "La", "Sib", "Si"];

fn default_key() -> u8 {
    7
}
fn default_scale() -> u8 {
    1
}
fn default_loop_b() -> u32 {
    4
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Project {
    pub version: u32,
    pub name: String,
    pub bpm: f32,
    pub swing: f32,
    pub master: f32,
    pub vinyl: f32,
    pub channels: Vec<Channel>,
    pub patterns: Vec<Pattern>,
    pub playlist: Vec<Clip>,
    pub tracks: Vec<String>,
    /// silencio, solo, volumen y paneo de cada pista de la playlist (mismo orden que `tracks`)
    #[serde(default)]
    pub pistas: Vec<PistaP>,
    pub next_id: u32,
    #[serde(default = "default_eq")]
    pub master_eq: [Band; 6],
    #[serde(default = "CompP::master")]
    pub master_comp: CompP,
    #[serde(default)]
    pub limiter: LimP,
    #[serde(default)]
    pub verb: VerbP,
    #[serde(default)]
    pub space: SpaceP,
    #[serde(default)]
    pub audio_clips: Vec<AudioClip>,
    #[serde(default)]
    pub autos: Vec<AutoClip>,
    /// tonalidad del proyecto (0 = Do) y escala (índice en SCALES)
    #[serde(default = "default_key")]
    pub key: u8,
    #[serde(default = "default_scale")]
    pub scale: u8,
    /// swing aplicado a corcheas en vez de semicorcheas
    #[serde(default)]
    pub swing8: bool,
    /// región de bucle/punch de la playlist (compases)
    #[serde(default)]
    pub loop_on: bool,
    #[serde(default)]
    pub punch: bool,
    #[serde(default)]
    pub loop_a: u32,
    #[serde(default = "default_loop_b")]
    pub loop_b: u32,
    /// plugins CLAP en el master (antes del limitador)
    #[serde(default)]
    pub master_inserts: Vec<PluginSlot>,
    /// letras y notas de la canción
    #[serde(default)]
    pub letras: String,
}

pub const PALETTE: [[u8; 3]; 8] = [
    [74, 144, 255],
    [156, 110, 255],
    [36, 196, 178],
    [255, 100, 168],
    [94, 206, 112],
    [248, 198, 62],
    [64, 200, 255],
    [255, 122, 96],
];

impl Project {
    /// Ajustes de la pista `t` (los de siempre si no se tocaron).
    pub fn pista(&self, t: usize) -> PistaP {
        self.pistas.get(t).copied().unwrap_or_default()
    }
    /// Ganancia y paneo con que suena la pista `t` en la canción (None = no suena por silencio o solo).
    pub fn pista_suena(&self, t: usize) -> Option<(f32, f32)> {
        let p = self.pista(t);
        let solo = self.pistas.iter().any(|x| x.solo);
        if p.mute || (solo && !p.solo) { None } else { Some((p.vol, p.pan)) }
    }
    /// Ajustes de la pista `t` para cambiarlos.
    pub fn pista_mut(&mut self, t: usize) -> &mut PistaP {
        if self.pistas.len() <= t {
            self.pistas.resize(t + 1, PistaP::default());
        }
        &mut self.pistas[t]
    }
    pub fn step_secs(&self) -> f64 {
        60.0 / self.bpm.max(20.0) as f64 / 4.0
    }
    pub fn pattern(&self, id: u32) -> Option<&Pattern> {
        self.patterns.iter().find(|p| p.id == id)
    }
    pub fn pattern_mut(&mut self, id: u32) -> Option<&mut Pattern> {
        self.patterns.iter_mut().find(|p| p.id == id)
    }
    pub fn channel(&self, id: u32) -> Option<&Channel> {
        self.channels.iter().find(|c| c.id == id)
    }
    pub fn channel_mut(&mut self, id: u32) -> Option<&mut Channel> {
        self.channels.iter_mut().find(|c| c.id == id)
    }
    pub fn song_bars(&self) -> u32 {
        let pats = self.playlist.iter().filter_map(|c| self.pattern(c.pat).map(|p| c.bar + p.bars)).max().unwrap_or(0);
        let auds = self.audio_clips.iter().map(|c| (c.start + c.len).div_ceil(STEPS_PER_BAR)).max().unwrap_or(0);
        let auts = self.autos.iter().map(|c| (c.start + c.len).div_ceil(STEPS_PER_BAR)).max().unwrap_or(0);
        pats.max(auds).max(auts)
    }
    /// Notas (máscara de 12 bits, bit 0 = Do) de los canales melódicos que suenan en un paso.
    /// `pat` = Some(id) en modo patrón; None = en la canción (playlist).
    pub fn acorde_en(&self, pat: Option<u32>, paso: u64) -> u16 {
        let mut m = 0u16;
        let mut sumar = |pt: &Pattern, l: u32| {
            for (ch, ns) in &pt.notes {
                if self.channel(*ch).is_some_and(|c| c.inst.melodic() && !c.mute) {
                    for n in ns {
                        if n.s <= l && l < n.s + n.l {
                            m |= 1 << (n.m % 12);
                        }
                    }
                }
            }
        };
        match pat {
            Some(id) => {
                if let Some(pt) = self.pattern(id) {
                    let len = pt.len_steps().max(1) as u64;
                    sumar(pt, (paso % len) as u32);
                }
            }
            None => {
                for cl in &self.playlist {
                    if let Some(pt) = self.pattern(cl.pat) {
                        let s0 = cl.bar as u64 * STEPS_PER_BAR as u64;
                        if paso >= s0 && paso < s0 + pt.len_steps() as u64 {
                            sumar(pt, (paso - s0) as u32);
                        }
                    }
                }
            }
        }
        m
    }
    pub fn in_scale(&self, m: i32) -> bool {
        SCALES[(self.scale as usize).min(SCALES.len() - 1)].1[((m - self.key as i32).rem_euclid(12)) as usize]
    }
    /// Nota más cercana que pertenece a la escala del proyecto.
    pub fn snap_scale(&self, m: i32) -> i32 {
        for d in [0, -1, 1, -2, 2] {
            if self.in_scale(m + d) {
                return m + d;
            }
        }
        m
    }
    pub fn key_label(&self) -> String {
        format!("{} {}", KEY_NAMES[(self.key % 12) as usize], SCALES[(self.scale as usize).min(SCALES.len() - 1)].0.to_lowercase())
    }
    /// Región de bucle en pasos, si está activa y es válida.
    pub fn loop_steps(&self) -> Option<(u32, u32)> {
        (self.loop_on && self.loop_b > self.loop_a).then(|| (self.loop_a * STEPS_PER_BAR, self.loop_b * STEPS_PER_BAR))
    }
    pub fn audio_clip(&self, id: u32) -> Option<&AudioClip> {
        self.audio_clips.iter().find(|c| c.id == id)
    }
    pub fn auto_clip(&self, id: u32) -> Option<&AutoClip> {
        self.autos.iter().find(|c| c.id == id)
    }
    /// Proyecto vacío con un kit básico (base de las plantillas).
    pub fn empty(name: &str, bpm: f32) -> Project {
        Project {
            version: 1,
            name: name.into(),
            bpm,
            swing: 0.0,
            master: 0.9,
            vinyl: 0.0,
            channels: vec![],
            patterns: vec![],
            playlist: vec![],
            tracks: (1..=12).map(|i| format!("Pista {i}")).collect(),
            next_id: 0,
            master_eq: default_eq(),
            master_comp: CompP::master(),
            limiter: LimP::default(),
            verb: VerbP::default(),
            space: SpaceP::default(),
            audio_clips: vec![],
            autos: vec![],
            key: 0,
            scale: 1,
            swing8: false,
            loop_on: false,
            punch: false,
            loop_a: 0,
            loop_b: 4,
            master_inserts: vec![],
            letras: String::new(),
            pistas: vec![],
        }
    }
    pub fn new_id(&mut self) -> u32 {
        self.next_id += 1;
        self.next_id
    }
    pub fn any_solo(&self) -> bool {
        self.channels.iter().any(|c| c.solo)
    }
    pub fn audible(&self, c: &Channel) -> bool {
        !c.mute && (!self.any_solo() || c.solo)
    }

    /// Beat de ejemplo: boom bap en Sol menor con acordes abiertos.
    pub fn demo() -> Project {
        let mut p = Project {
            version: 1,
            name: "Beat Boom Bap Sol menor".into(),
            bpm: 88.0,
            swing: 0.28,
            master: 0.9,
            vinyl: 0.5,
            channels: vec![],
            patterns: vec![],
            playlist: vec![],
            tracks: (1..=8)
                .map(|i| match i {
                    1 => "Acordes".to_string(),
                    2 => "Batería".to_string(),
                    3 => "Melodía".to_string(),
                    n => format!("Pista {n}"),
                })
                .collect(),
            next_id: 0,
            master_eq: default_eq(),
            master_comp: CompP::master(),
            limiter: LimP::default(),
            verb: VerbP::default(),
            space: SpaceP::default(),
            audio_clips: vec![],
            autos: vec![],
            key: 7,
            scale: 1,
            swing8: false,
            loop_on: false,
            punch: false,
            loop_a: 0,
            loop_b: 4,
            master_inserts: vec![],
            letras: String::new(),
            pistas: vec![],
        };
        let mk = |p: &mut Project, inst: Inst, vol: f32, pan: f32| {
            let id = p.new_id();
            let mut c = Channel::new(id, inst);
            c.vol = vol;
            c.pan = pan;
            p.channels.push(c);
            id
        };
        let k = mk(&mut p, Inst::Kick, 0.95, 0.0);
        let s = mk(&mut p, Inst::Snare, 0.8, 0.02);
        let h = mk(&mut p, Inst::Hat, 0.55, 0.25);
        let o = mk(&mut p, Inst::OHat, 0.5, 0.25);
        let r = mk(&mut p, Inst::Rhodes, 0.75, 0.0);
        let b = mk(&mut p, Inst::Bass, 0.8, 0.0);
        let e = mk(&mut p, Inst::Bell, 0.6, -0.3);
        p.channel_mut(s).unwrap().rev = 0.12;
        {
            let c = p.channel_mut(r).unwrap();
            c.rev = 0.22;
            c.strum = 0.018;
        }
        p.channel_mut(e).unwrap().rev = 0.35;

        let steps = |n: usize, l: &[(usize, f32)]| {
            let mut a = vec![0.0; n];
            for &(i, v) in l {
                a[i] = v;
            }
            a
        };
        // Batería (2 compases)
        let pid = p.new_id();
        let mut drums = Pattern { id: pid, name: "Batería".into(), bars: 2, color: PALETTE[0], steps: HashMap::new(), notes: HashMap::new(), audio: HashMap::new() };
        drums.steps.insert(k, steps(32, &[(0, 0.95), (7, 0.95), (10, 0.95), (16, 0.95), (18, 0.55), (23, 0.95), (26, 0.95), (27, 0.55)]));
        drums.steps.insert(s, steps(32, &[(4, 0.8), (12, 0.8), (20, 0.8), (28, 0.8), (31, 0.25)]));
        let mut hh: Vec<(usize, f32)> = (0..32).step_by(2).filter(|&i| i != 30).map(|i| (i, if i % 4 == 0 { 0.65 } else { 0.5 })).collect();
        hh.extend([3, 9, 13, 19, 25, 29].iter().map(|&i| (i, 0.2)));
        drums.steps.insert(h, steps(32, &hh));
        drums.steps.insert(o, steps(32, &[(30, 0.55)]));
        // Acordes abiertos (4 compases): Gm9 · E♭maj9 · Cm11 · Dsus4 → D7♭13
        let cid = p.new_id();
        let mut chords = Pattern { id: cid, name: "Acordes".into(), bars: 4, color: PALETTE[1], steps: HashMap::new(), notes: HashMap::new(), audio: HashMap::new() };
        let mut rn = vec![];
        let voicings: [&[u8]; 3] = [&[43, 50, 58, 65, 69], &[39, 46, 55, 62, 65], &[48, 55, 58, 63, 65]];
        for (bar, v) in voicings.iter().enumerate() {
            for &m in v.iter() {
                rn.push(Note { s: bar as u32 * 16, l: 16, m, v: 0.8, o: 0.0 });
            }
        }
        for m in [50u8, 57, 60, 67] {
            rn.push(Note { s: 48, l: 8, m, v: 0.8, o: 0.0 });
        }
        for m in [50u8, 54, 60, 66, 70] {
            rn.push(Note { s: 56, l: 8, m, v: 0.75, o: 0.0 });
        }
        chords.notes.insert(r, rn);
        let mut bn = vec![];
        for (bar, m) in [31u8, 27, 36, 38].iter().enumerate() {
            bn.push(Note { s: bar as u32 * 16, l: 6, m: *m, v: 0.85, o: 0.0 });
            bn.push(Note { s: bar as u32 * 16 + 10, l: 5, m: *m, v: 0.7, o: 0.0 });
        }
        chords.notes.insert(b, bn);
        // Melodía (2 compases)
        let mid = p.new_id();
        let mut mel = Pattern { id: mid, name: "Melodía".into(), bars: 2, color: PALETTE[2], steps: HashMap::new(), notes: HashMap::new(), audio: HashMap::new() };
        mel.notes.insert(e, [(0u32, 74u8), (3, 77), (6, 74), (10, 72), (14, 70)].iter().map(|&(s, m)| Note { s, l: 4, m, v: 0.75, o: 0.0 }).collect());
        p.patterns = vec![drums, chords, mel];
        for bar in (0..20).step_by(4) {
            p.playlist.push(Clip { track: 0, bar, pat: cid });
        }
        for bar in (2..18).step_by(2) {
            p.playlist.push(Clip { track: 1, bar, pat: pid });
        }
        for bar in (10..18).step_by(2) {
            p.playlist.push(Clip { track: 2, bar, pat: mid });
        }
        p
    }
}

// ===================== Parámetros de los plugins =====================

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BandKind {
    Bell,
    LowShelf,
    HighShelf,
    LowCut,
    HighCut,
}
impl BandKind {
    pub const ALL: [BandKind; 5] = [BandKind::Bell, BandKind::LowShelf, BandKind::HighShelf, BandKind::LowCut, BandKind::HighCut];
    pub fn name(self) -> &'static str {
        match self {
            BandKind::Bell => "Campana",
            BandKind::LowShelf => "Estante grave",
            BandKind::HighShelf => "Estante agudo",
            BandKind::LowCut => "Corte de graves",
            BandKind::HighCut => "Corte de agudos",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Band {
    pub on: bool,
    pub kind: BandKind,
    pub freq: f32,
    pub gain: f32,
    pub q: f32,
}

pub fn default_eq() -> [Band; 6] {
    let b = |on, kind, freq, q| Band { on, kind, freq, gain: 0.0, q };
    [
        b(false, BandKind::LowCut, 30.0, 0.707),
        b(true, BandKind::LowShelf, 120.0, 0.707),
        b(true, BandKind::Bell, 450.0, 1.0),
        b(true, BandKind::Bell, 2200.0, 1.0),
        b(true, BandKind::HighShelf, 7000.0, 0.707),
        b(false, BandKind::HighCut, 18000.0, 0.707),
    ]
}
pub fn eq_is_flat(eq: &[Band; 6]) -> bool {
    eq.iter().all(|b| !b.on || (b.gain.abs() < 0.01 && matches!(b.kind, BandKind::Bell | BandKind::LowShelf | BandKind::HighShelf)))
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CompP {
    pub on: bool,
    pub threshold: f32,
    pub ratio: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
    pub makeup: f32,
    /// canal que dispara el compresor (sidechain); 0 = la propia señal
    #[serde(default)]
    pub sc: u32,
}
impl CompP {
    pub fn channel() -> CompP {
        CompP { on: false, threshold: -18.0, ratio: 3.0, attack_ms: 10.0, release_ms: 120.0, makeup: 0.0, sc: 0 }
    }
    /// Bombeo típico de sidechain: el canal se agacha cuando suena la fuente.
    pub fn ducking(src: u32) -> CompP {
        CompP { on: true, threshold: -32.0, ratio: 12.0, attack_ms: 1.5, release_ms: 170.0, makeup: 0.0, sc: src }
    }
    pub fn master() -> CompP {
        CompP { on: true, threshold: -14.0, ratio: 4.0, attack_ms: 10.0, release_ms: 200.0, makeup: 2.6, sc: 0 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct LimP {
    pub on: bool,
    pub gain_db: f32,
    pub ceiling_db: f32,
    pub release_ms: f32,
}
impl Default for LimP {
    fn default() -> Self {
        LimP { on: true, gain_db: 0.0, ceiling_db: -1.0, release_ms: 80.0 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct VerbP {
    pub size: f32,
    pub decay: f32,
    pub damp: f32,
    pub predelay_ms: f32,
    pub width: f32,
    pub lowcut: f32,
    pub ret: f32,
}
impl Default for VerbP {
    fn default() -> Self {
        VerbP { size: 0.6, decay: 0.55, damp: 0.25, predelay_ms: 20.0, width: 1.0, lowcut: 200.0, ret: 1.0 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SpaceP {
    pub delay_ms: f32,
    pub feedback: f32,
    pub density: f32,
    pub mod_depth: f32,
    pub mod_rate: f32,
    pub damp: f32,
    pub ret: f32,
}
impl Default for SpaceP {
    fn default() -> Self {
        SpaceP { delay_ms: 240.0, feedback: 0.82, density: 0.7, mod_depth: 0.4, mod_rate: 0.3, damp: 0.35, ret: 0.9 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct OscP {
    pub on: bool,
    /// posición en la tabla de ondas (0 seno → 4 formante)
    pub wt: f32,
    pub oct: i32,
    pub semi: i32,
    pub fine: f32,
    pub level: f32,
    pub unison: u32,
    pub detune: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SynthP {
    pub osc: [OscP; 2],
    /// 0 = pasa bajos, 1 = pasa altos, 2 = pasa banda
    pub ftype: u8,
    pub cutoff: f32,
    pub reso: f32,
    pub fenv: f32,
    pub amp: [f32; 4],
    pub fadsr: [f32; 4],
    pub lfo_rate: f32,
    pub lfo_depth: f32,
    /// 0 = filtro, 1 = tabla de ondas, 2 = tono
    pub lfo_target: u8,
    pub level: f32,
}
impl Default for SynthP {
    fn default() -> Self {
        SynthP::preset(0)
    }
}
pub const SYNTH_PRESETS: [&str; 12] = ["Supersaw", "Pluck", "Pad atmosférico", "Bajo reese", "Lead cuadrado", "Campanas FM", "Lead trap", "Cuerdas", "Bajo acid", "Arpegio brillante", "Órgano", "Pad oscuro"];
impl SynthP {
    pub fn preset(i: usize) -> SynthP {
        let o = |wt: f32, oct: i32, level: f32, unison: u32, detune: f32| OscP { on: true, wt, oct, semi: 0, fine: 0.0, level, unison, detune };
        let off = OscP { on: false, wt: 0.0, oct: 0, semi: 0, fine: 0.0, level: 0.5, unison: 1, detune: 0.0 };
        let mut p = SynthP {
            osc: [o(2.0, 0, 0.8, 7, 0.35), o(2.0, -1, 0.5, 3, 0.2)],
            ftype: 0,
            cutoff: 5200.0,
            reso: 0.15,
            fenv: 0.25,
            amp: [0.01, 0.3, 0.8, 0.35],
            fadsr: [0.01, 0.5, 0.4, 0.4],
            lfo_rate: 0.3,
            lfo_depth: 0.0,
            lfo_target: 0,
            level: 0.55,
        };
        match i {
            1 => {
                p.osc = [o(2.6, 0, 0.8, 3, 0.12), off];
                p.cutoff = 1200.0;
                p.fenv = 0.75;
                p.amp = [0.002, 0.35, 0.0, 0.25];
                p.fadsr = [0.001, 0.22, 0.0, 0.2];
            }
            2 => {
                p.osc = [o(1.4, 0, 0.7, 5, 0.25), o(3.3, 1, 0.3, 3, 0.15)];
                p.cutoff = 1800.0;
                p.fenv = 0.1;
                p.amp = [0.8, 1.0, 0.85, 1.6];
                p.lfo_depth = 0.35;
                p.lfo_rate = 0.25;
                p.lfo_target = 1;
            }
            3 => {
                p.osc = [o(2.0, -1, 0.9, 2, 0.08), o(2.0, -1, 0.9, 2, 0.11)];
                p.cutoff = 600.0;
                p.reso = 0.3;
                p.fenv = 0.2;
                p.amp = [0.005, 0.2, 1.0, 0.12];
                p.lfo_depth = 0.3;
                p.lfo_rate = 0.4;
            }
            4 => {
                p.osc = [o(3.0, 0, 0.8, 1, 0.0), o(3.0, 1, 0.25, 1, 0.0)];
                p.cutoff = 3500.0;
                p.fenv = 0.2;
                p.amp = [0.005, 0.2, 0.7, 0.2];
                p.lfo_depth = 0.08;
                p.lfo_rate = 5.5;
                p.lfo_target = 2;
            }
            5 => {
                p.osc = [o(0.0, 0, 0.8, 1, 0.0), o(4.0, 2, 0.35, 1, 0.0)];
                p.cutoff = 9000.0;
                p.fenv = 0.0;
                p.amp = [0.002, 1.2, 0.0, 1.0];
            }
            6 => {
                p.osc = [o(3.6, 0, 0.7, 3, 0.1), o(0.0, 1, 0.35, 1, 0.0)];
                p.cutoff = 6000.0;
                p.fenv = 0.15;
                p.amp = [0.003, 0.4, 0.5, 0.25];
                p.lfo_depth = 0.05;
                p.lfo_rate = 5.0;
                p.lfo_target = 2;
            }
            7 => {
                p.osc = [o(2.0, 0, 0.7, 6, 0.18), o(1.2, -1, 0.4, 4, 0.12)];
                p.cutoff = 2600.0;
                p.fenv = 0.05;
                p.amp = [0.35, 0.5, 0.9, 0.9];
                p.lfo_depth = 0.1;
                p.lfo_rate = 4.5;
                p.lfo_target = 2;
            }
            8 => {
                p.osc = [o(2.0, -1, 0.9, 1, 0.0), off];
                p.cutoff = 450.0;
                p.reso = 0.75;
                p.fenv = 0.8;
                p.amp = [0.002, 0.2, 0.8, 0.08];
                p.fadsr = [0.001, 0.18, 0.1, 0.1];
            }
            9 => {
                p.osc = [o(2.8, 0, 0.75, 2, 0.08), o(0.0, 1, 0.3, 1, 0.0)];
                p.cutoff = 4000.0;
                p.fenv = 0.5;
                p.amp = [0.001, 0.18, 0.0, 0.15];
                p.fadsr = [0.001, 0.12, 0.0, 0.1];
            }
            10 => {
                p.osc = [o(0.6, 0, 0.7, 1, 0.0), o(0.6, 1, 0.45, 1, 0.0)];
                p.cutoff = 7000.0;
                p.fenv = 0.0;
                p.amp = [0.01, 0.1, 1.0, 0.08];
                p.lfo_depth = 0.04;
                p.lfo_rate = 6.0;
                p.lfo_target = 2;
            }
            11 => {
                p.osc = [o(1.8, -1, 0.7, 5, 0.3), o(3.8, 0, 0.25, 3, 0.2)];
                p.cutoff = 900.0;
                p.reso = 0.25;
                p.fenv = 0.1;
                p.amp = [1.2, 1.0, 0.8, 2.0];
                p.lfo_depth = 0.5;
                p.lfo_rate = 0.15;
                p.lfo_target = 0;
            }
            _ => {}
        }
        p
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TuneP {
    pub on: bool,
    /// tónica 0..11 (7 = Sol)
    pub key: u8,
    /// 0 = menor, 1 = mayor, 2 = cromática
    pub scale: u8,
    /// velocidad de corrección: 0 = instantánea (efecto robot), 1 = natural
    pub speed: f32,
    pub mix: f32,
    pub gain: f32,
    /// notas permitidas (bit 0 = Do … bit 11 = Si); 0 = usar `key` y `scale`
    #[serde(default)]
    pub mask: u16,
    /// estilo de NN Tune (índice en fx::ESTILOS)
    #[serde(default)]
    pub estilo: u8,
    /// 0 = limpio (conserva el timbre), 1 = baja demora
    #[serde(default)]
    pub algoritmo: u8,
    /// volumen de las voces de armonía (1 = el del estilo)
    #[serde(default = "uno")]
    pub mezcla: f32,
}
fn uno() -> f32 {
    1.0
}
impl Default for TuneP {
    fn default() -> Self {
        TuneP { on: true, key: 7, scale: 0, speed: 0.15, mix: 1.0, gain: 1.0, mask: 0, estilo: 0, algoritmo: 0, mezcla: 1.0 }
    }
}

/// NN Sampler: reproducción de un archivo de audio.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SampleP {
    /// nombre del archivo en la carpeta de samples ("" = vacío)
    pub file: String,
    /// nota MIDI en la que suena el sample sin transponer
    pub root: u8,
    /// inicio y fin (fracción 0..1 del archivo)
    pub start: f32,
    pub end: f32,
    pub reverse: bool,
    pub attack: f32,
    pub release: f32,
    /// suena completo aunque la nota sea corta (percusión)
    pub oneshot: bool,
    pub gain: f32,
    /// modo slicer: cada nota desde `root` toca un trozo
    pub slice: bool,
    /// cortes en segundos (modo slicer)
    pub slices: Vec<f32>,
    /// estiramiento de tiempo (duración nueva / original) sin cambiar el tono
    pub stretch: f32,
    /// cambio de tono en semitonos sin cambiar la duración
    pub pitch: f32,
    /// tempo original del loop (0 = no ajustar); con valor, se estira al tempo del proyecto
    pub bpm: f32,
}
impl Default for SampleP {
    fn default() -> Self {
        SampleP { file: String::new(), root: 60, start: 0.0, end: 1.0, reverse: false, attack: 0.0, release: 0.05, oneshot: true, gain: 1.0, slice: false, slices: vec![], stretch: 1.0, pitch: 0.0, bpm: 0.0 }
    }
}
impl SampleP {
    /// factor de tiempo total a un tempo dado
    pub fn ratio(&self, bpm: f32) -> f32 {
        let b = if self.bpm > 0.0 { self.bpm / bpm.max(1.0) } else { 1.0 };
        (self.stretch * b).clamp(0.25, 4.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ArpP {
    pub on: bool,
    /// 0 = sube, 1 = baja, 2 = sube y baja, 3 = aleatorio, 4 = acorde repetido
    pub mode: u8,
    /// pasos por nota (1 = 1/16, 2 = 1/8, 4 = 1/4)
    pub rate: u32,
    pub oct: u8,
    pub gate: f32,
}
impl Default for ArpP {
    fn default() -> Self {
        ArpP { on: false, mode: 0, rate: 1, oct: 1, gate: 0.8 }
    }
}
pub const ARP_MODES: [&str; 5] = ["Sube", "Baja", "Sube y baja", "Aleatorio", "Acorde repetido"];

/// Plugin externo (CLAP) cargado en un canal o en el master.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PluginSlot {
    /// ruta del archivo .clap
    pub path: String,
    /// identificador del plugin dentro del archivo
    pub id: String,
    pub name: String,
    pub on: bool,
    /// estado guardado por el plugin (base64)
    #[serde(default)]
    pub state: String,
    /// identificador único de la ranura (para la automatización)
    #[serde(default)]
    pub uid: u32,
}
