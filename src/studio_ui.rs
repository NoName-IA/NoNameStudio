//! Funciones de estudio: navegador de archivos, playlist con audio y automatización, NN Sampler,
//! grabación por tomas (bucle/punch/comping), MIDI, mastering, plantillas, presets y versiones.
use super::*;
use crate::media::{self, Buf};
use crate::midi;
use crate::templates;

// ============================ tipos ============================

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub(super) struct Settings {
    /// compensación de latencia de grabación (ms)
    pub latency_ms: f32,
    /// minutos entre versiones guardadas automáticamente
    pub version_min: u32,
    /// carpetas extra con plugins CLAP
    pub plugin_dirs: Vec<String>,
    /// carpetas favoritas del navegador
    pub fav_dirs: Vec<String>,
    /// nombre del teclado MIDI preferido
    pub midi_port: String,
    /// cuantizar lo que se graba con el teclado MIDI
    pub midi_quant: bool,
    /// ya vio (o saltó) el tutorial de bienvenida
    pub tutorial_done: bool,
    // ---- general
    /// abrir el último proyecto al iniciar (si no, siempre empieza uno nuevo)
    pub abrir_ultimo: bool,
    /// guardar solo cada tantos minutos (0 = nunca)
    pub autoguardar_min: u32,
    /// guardar copias de respaldo (versiones)
    pub versiones: bool,
    /// preguntar antes de cerrar o reemplazar un proyecto con cambios sin guardar
    pub preguntar: bool,
    /// plantilla y tempo de los proyectos nuevos
    pub plantilla_nueva: usize,
    pub bpm_nuevo: f32,
    // ---- audio
    /// salida y entrada de audio ("" = las de Windows)
    pub salida: String,
    pub entrada: String,
    /// tamaño de bloque del audio (0 = automático)
    pub buffer: u32,
    /// metrónomo mientras se graba
    pub metro_grabar: bool,
    /// escucharse por el micrófono (se apaga al cerrar el programa)
    #[serde(skip)]
    pub escucharme: bool,
    pub vol_escucha: f32,
    /// canal de la entrada que se graba (0 = mezcla de todos)
    #[serde(default)]
    pub canal_entrada: u8,
    /// la voz grabada pasa por el mismo nivelador que cuando te escuchas
    pub nivelar_grabacion: bool,
    /// metrónomo: volumen, sonido, acento en el primer tiempo y subdivisión
    pub metro_vol: f32,
    pub metro_sonido: u8,
    pub metro_acento: bool,
    pub metro_sub: u8,
    /// compases de precuenta antes de grabar (0 = sin precuenta)
    pub precuenta: u8,
    /// paneo y reverberación de la voz que te escuchas; ajustes avanzados de NN Tune a la vista
    #[serde(default)]
    pub pan_escucha: f32,
    #[serde(default)]
    pub reverb_escucha: f32,
    #[serde(default)]
    pub tune_avanzado: bool,
    /// autotune en vivo al escucharse
    pub tune_vivo: bool,
    /// intensidad del autotune (0 = suave, 1 = lo más intenso)
    pub tune_int: f32,
    /// tónica y escala del autotune (255 = las de la canción) y estilo
    pub tune_tonica: u8,
    pub tune_escala: u8,
    pub tune_estilo: u8,
    /// el autotune sigue los acordes de la canción
    pub tune_acordes: bool,
    /// categoría de estilos a la vista, algoritmo (0 limpio, 1 baja demora) y volumen de la armonía
    pub tune_categoria: u8,
    pub tune_alg: u8,
    pub tune_mezcla: f32,
    // ---- exportar
    /// carpeta de exportación ("" = Escritorio)
    pub export_dir: String,
    pub mp3_kbps: u32,
    pub wav_bits: u16,
    pub sr_export: u32,
    /// 0 = automático, 1 = siempre, 2 = nunca
    pub normalizar: u8,
    // ---- pantalla
    /// tamaño de la interfaz (1 = normal)
    pub zoom: f32,
    /// mostrar textos de ayuda
    pub ayudas: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Settings {
            latency_ms: 20.0,
            version_min: 5,
            plugin_dirs: vec![],
            fav_dirs: vec![],
            midi_port: String::new(),
            midi_quant: true,
            tutorial_done: false,
            abrir_ultimo: false,
            autoguardar_min: 0,
            versiones: false,
            preguntar: true,
            plantilla_nueva: 0,
            bpm_nuevo: 120.0,
            salida: String::new(),
            entrada: String::new(),
            buffer: 0,
            metro_grabar: true,
            escucharme: false,
            vol_escucha: 0.5,
            canal_entrada: 0,
            nivelar_grabacion: true,
            metro_vol: 0.8,
            metro_sonido: 0,
            metro_acento: true,
            metro_sub: 0,
            precuenta: 1,
            pan_escucha: 0.0,
            reverb_escucha: 0.0,
            tune_avanzado: false,
            tune_vivo: false,
            tune_int: 0.5,
            tune_tonica: 255,
            tune_escala: 255,
            tune_estilo: 0,
            tune_acordes: false,
            tune_categoria: 0,
            tune_alg: 0,
            tune_mezcla: 1.0,
            export_dir: String::new(),
            mp3_kbps: 320,
            wav_bits: 16,
            sr_export: 44_100,
            normalizar: 0,
            zoom: 1.0,
            ayudas: true,
        }
    }
}

pub(super) struct Browser {
    pub dir: Option<PathBuf>,
    pub list: Vec<(PathBuf, bool)>,
    pub loaded: Option<PathBuf>,
    pub filter: String,
    pub sel: Option<PathBuf>,
}
impl Default for Browser {
    fn default() -> Self {
        Browser { dir: None, list: vec![], loaded: None, filter: String::new(), sel: None }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Kind {
    Pat,
    Audio,
    Auto,
}

pub(super) enum PlDrag {
    Pat { idx: usize, bar0: i32, tr0: i32, orig: Clip, moved: bool },
    Move { kind: Kind, id: u32, s0: i32, tr0: i32, start: u32, track: u32, moved: bool },
    Right { kind: Kind, id: u32 },
    Left { id: u32, start: u32, len: u32, off: f32 },
    Loop { a: u32 },
}

pub(super) struct PlMenu {
    pub kind: Kind,
    pub id: u32,
    pub idx: usize,
    pub pos: Pos2,
    pub step: u32,
    pub opened: u32,
}

pub(super) struct TrackRec {
    pub r: audio::Recorder,
    pub ch: u32,
    pub from: u32,
    pub region: Option<(u32, u32)>,
    pub looped: bool,
    pub started: Instant,
    /// carril de la playlist donde se dibuja y se guardan las tomas
    pub track: u32,
    /// segundos de precuenta al principio de la grabación (se descartan)
    pub precuenta_s: f32,
}

pub(super) enum FileMsg {
    /// análisis del Retune de un clip
    Retune(u32, Result<super::retune_view::Retune, String>),
    /// voz calculada con los cambios del Retune (clip, clave de los cambios, audio)
    RetuneListo(u32, u64, Vec<f32>),
    /// armonía lista: (clip de origen, nombre de la armonía, archivo nuevo y audio)
    Armonia(u32, String, Result<(String, Buf), String>),
    Built(String, Result<Buf, String>),
    Preview(Result<Buf, String>),
    /// toma afinada con NN Tune para un clip: (clip, nombre del archivo nuevo, audio)
    Afinado(u32, Result<(String, Buf), String>),
}

pub(super) type MasterRes = Result<(f32, f32, [Band; 6], CompP), String>;

/// Cadenas de efectos para voz (como los presets de «Efectos» de BandLab):
/// nombre, ayuda, corte de graves (Hz, 0 = apagado), corte de agudos (Hz, 0 = apagado),
/// (450 Hz, 2,2 kHz, agudos) en dB, (umbral, razón, ganancia) del compresor, reverb, delay, espacio.
pub(super) const VOZ_FX: [(&str, &str, f32, f32, [f32; 3], (f32, f32, f32), f32, f32, f32); 8] = [
    ("Ninguno", "Sin efectos: la voz tal cual", 0.0, 0.0, [0.0, 0.0, 0.0], (0.0, 1.0, 0.0), 0.0, 0.0, 0.0),
    ("Voz limpia", "Clara y pareja, lista para cualquier género", 90.0, 0.0, [-2.0, 2.0, 2.5], (-20.0, 3.0, 3.0), 0.12, 0.0, 0.0),
    ("Rap al frente", "Seca, fuerte y con presencia", 100.0, 0.0, [-3.0, 3.5, 3.0], (-24.0, 4.0, 5.0), 0.05, 0.08, 0.0),
    ("Trap brillante", "Brillo arriba, reverb y delay cortos", 110.0, 0.0, [-2.0, 3.0, 5.0], (-22.0, 4.0, 4.0), 0.18, 0.15, 0.0),
    ("Balada", "Suave, cálida y con sala", 80.0, 0.0, [-1.0, 1.0, 2.0], (-18.0, 2.5, 2.0), 0.35, 0.1, 0.0),
    ("Reggaetón", "Al frente con delay rítmico", 100.0, 0.0, [-2.5, 3.0, 3.5], (-22.0, 3.5, 4.0), 0.12, 0.22, 0.0),
    ("Radio", "Sonido de radio o teléfono", 500.0, 3500.0, [0.0, 4.0, 0.0], (-20.0, 4.0, 4.0), 0.0, 0.0, 0.0),
    ("Ambiente amplio", "Voz grande y espacial", 90.0, 0.0, [-1.5, 1.5, 3.0], (-20.0, 3.0, 3.0), 0.45, 0.25, 0.3),
];

/// Aplica una cadena de efectos de voz a un canal.
pub(super) fn aplicar_voz_fx(c: &mut Channel, i: usize) {
    let (_, _, lc, hc, eq, (thr, ratio, mk), rev, dly, space) = VOZ_FX[i.min(VOZ_FX.len() - 1)];
    c.eq = default_eq();
    if lc > 0.0 {
        c.eq[0].on = true;
        c.eq[0].freq = lc;
    }
    if hc > 0.0 {
        c.eq[5].on = true;
        c.eq[5].freq = hc;
    }
    c.eq[2].gain = eq[0];
    c.eq[3].gain = eq[1];
    c.eq[4].gain = eq[2];
    c.comp = CompP { on: i > 0, threshold: thr, ratio, attack_ms: 8.0, release_ms: 120.0, makeup: mk, sc: 0 };
    c.rev = rev;
    c.dly = dly;
    c.space = space;
}

/// Estilos de mastering: nombre, descripción, (graves, medios, presencia, agudos) en dB y razón del compresor.
pub(super) const MASTER_ESTILOS: [(&str, &str, [f32; 4], f32); 8] = [
    ("Universal", "Equilibrado para cualquier género", [0.6, 0.0, 0.3, 0.9], 2.0),
    ("Fuego", "Graves gordos y mucha pegada (trap, reggaetón, drill)", [2.6, -1.0, 0.6, 1.6], 3.0),
    ("Claridad", "Voces al frente, limpio y brillante", [-0.4, -1.5, 1.6, 2.0], 2.2),
    ("Cinta", "Cálido y suave, como en una cinta analógica", [1.2, 0.6, -0.4, -2.2], 1.7),
    ("Equilibrio", "Suave y parejo, sin exagerar nada (acústico, baladas)", [0.3, 0.3, 0.2, 0.4], 1.6),
    ("Espacio", "Abierto y aireado, con agudos sedosos (R&B, ambient)", [0.0, -0.8, 0.4, 2.4], 1.8),
    ("Cine", "Grande y profundo: graves amplios y más dinámica", [1.8, -0.6, 0.2, 1.0], 1.5),
    ("Brillo", "Pop moderno: brillante y con pegada", [1.0, -0.8, 1.2, 2.6], 2.6),
];
/// Valores de una intensidad continua (0 = ligero, 1 = normal, 2 = pesado): (LUFS objetivo, cuánto se marca el estilo).
pub(super) fn int_valores(x: f32) -> (f32, f32) {
    let x = x.clamp(0.0, 2.0);
    let i = (x.floor() as usize).min(1);
    let f = x - i as f32;
    let (a, b) = (MASTER_INT[i], MASTER_INT[i + 1]);
    (a.1 + (b.1 - a.1) * f, a.2 + (b.2 - a.2) * f)
}
/// Intensidad del mastering: nombre, sonoridad objetivo (LUFS) y cuánto se marca el estilo.
pub(super) const MASTER_INT: [(&str, f32, f32); 3] = [("Ligero", -16.0, 0.6), ("Normal", -13.0, 1.0), ("Pesado", -9.5, 1.3)];

/// Balance de graves (< 250 Hz), medios y agudos (> 4 kHz) de un audio estéreo, en dB respecto al total.
pub(super) fn balance_3(a: &[f32], sr: f32) -> [f32; 3] {
    let k_lo = 1.0 - (-std::f32::consts::TAU * 250.0 / sr).exp();
    let k_hi = 1.0 - (-std::f32::consts::TAU * 4000.0 / sr).exp();
    let (mut lo, mut lp4) = (0.0f32, 0.0f32);
    let (mut e_lo, mut e_mid, mut e_hi) = (1e-9f64, 1e-9f64, 1e-9f64);
    for f in a.chunks(2) {
        let x = (f[0] + f.get(1).copied().unwrap_or(f[0])) * 0.5;
        lo += (x - lo) * k_lo;
        lp4 += (x - lp4) * k_hi;
        let hi = x - lp4;
        let mid = lp4 - lo;
        e_lo += (lo * lo) as f64;
        e_mid += (mid * mid) as f64;
        e_hi += (hi * hi) as f64;
    }
    let t = e_lo + e_mid + e_hi;
    [(10.0 * (e_lo / t).log10()) as f32, (10.0 * (e_mid / t).log10()) as f32, (10.0 * (e_hi / t).log10()) as f32]
}

/// Ecualizador y compresor del master para un estilo, intensidad y ajustes (graves/medios/agudos en dB).
pub(super) fn cadena_master(estilo: usize, int: f32, eq3: [f32; 3]) -> ([Band; 6], CompP) {
    let (_, _, e, ratio) = MASTER_ESTILOS[estilo.min(MASTER_ESTILOS.len() - 1)];
    let int = int.clamp(0.0, 2.0);
    // los estilos se marcan bastante (como en BandLab): se nota la diferencia al prender y apagar
    let k = int_valores(int).1 * 1.6;
    let mut eq = default_eq();
    eq[0].on = true;
    eq[0].freq = 28.0;
    eq[1].gain = (e[0] * k + eq3[0]).clamp(-9.0, 9.0);
    eq[2].gain = (e[1] * k + eq3[1] * 0.6).clamp(-9.0, 9.0);
    eq[3].gain = (e[2] * k + eq3[1] * 0.4).clamp(-9.0, 9.0);
    eq[4].gain = (e[3] * k + eq3[2]).clamp(-9.0, 9.0);
    let comp = CompP { on: true, threshold: -16.0 - 2.0 * int, ratio: ratio * (0.85 + 0.15 * int), attack_ms: if estilo == 3 || estilo == 6 { 30.0 } else { 20.0 }, release_ms: 180.0, makeup: 0.0, sc: 0 };
    (eq, comp)
}

pub(super) const MASTER_TARGETS: [(&str, f32); 4] = [("Streaming (-14 LUFS)", -14.0), ("Fuerte / club (-9 LUFS)", -9.0), ("Dinámico (-16 LUFS)", -16.0), ("Lo-fi cálido (-12 LUFS)", -12.0)];

fn ago(secs: u64) -> String {
    match secs {
        0..=59 => "hace un momento".into(),
        60..=3599 => format!("hace {} min", secs / 60),
        3600..=86_399 => format!("hace {} h", secs / 3600),
        _ => format!("hace {} días", secs / 86_400),
    }
}
fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

impl StudioApp {
    // ============================ archivos de audio ============================

    pub(super) fn files_sync(&mut self) {
        if !self.files_dirty {
            return;
        }
        self.files_dirty = false;
        let keys = media::needed_keys(&self.p);
        // variantes procesadas que ya no se usan
        let stale: Vec<String> = self.files.keys().filter(|k| k.contains('#') && !keys.contains(k)).cloned().collect();
        for k in stale {
            self.files.remove(&k);
            self.cmd(Cmd::File { key: k, buf: None });
        }
        for k in keys {
            if self.files.contains_key(&k) || self.pending.contains(&k) || self.bad_files.contains(&k) {
                continue;
            }
            let raw = media::parse_key(&k).0;
            if self.bad_files.contains(&raw) {
                continue;
            }
            if k != raw && !self.files.contains_key(&raw) {
                // primero el original; la variante se pide cuando esté
                self.files_dirty = true;
                continue;
            }
            self.pending.insert(k.clone());
            let tx = self.file_tx.clone();
            let data = self.plat.data_dir.clone();
            let mut base = media::Files::new();
            if let Some(b) = self.files.get(&raw) {
                base.insert(raw.clone(), b.clone());
            }
            let ctx = self.ctx.clone();
            std::thread::spawn(move || {
                let r = media::build(&data, &k, &base);
                let _ = tx.send(FileMsg::Built(k, r));
                if let Some(c) = ctx {
                    c.request_repaint();
                }
            });
        }
    }

    pub(super) fn files_poll(&mut self) {
        while let Ok(m) = self.file_rx.try_recv() {
            match m {
                FileMsg::Built(k, r) => {
                    self.pending.remove(&k);
                    match r {
                        Ok(b) => {
                            let b = Arc::new(b);
                            if !k.contains('#') {
                                self.wave_peaks.insert(k.clone(), (Arc::new(b.peaks(4000)), b.secs()));
                            }
                            self.files.insert(k.clone(), b.clone());
                            self.cmd(Cmd::File { key: k, buf: Some(b) });
                            self.files_dirty = true;
                        }
                        Err(e) => {
                            // no se reintenta en esta sesión (archivo borrado o dañado)
                            self.bad_files.insert(k);
                            self.msg = format!("Audio: {e}");
                        }
                    }
                }
                FileMsg::Afinado(id, r) => match r {
                    Ok((name, b)) => {
                        let b = Arc::new(b);
                        self.wave_peaks.insert(name.clone(), (Arc::new(b.peaks(4000)), b.secs()));
                        self.files.insert(name.clone(), b.clone());
                        self.cmd(Cmd::File { key: name.clone(), buf: Some(b) });
                        self.edit_clip(id, |c| {
                            c.takes.push(name);
                            c.active = c.takes.len() - 1;
                        });
                        self.msg = "Listo: la voz quedó afinada como una toma nueva. La original sigue en el menú «Tomas» del clip.".into();
                    }
                    Err(e) => self.msg = format!("No se pudo afinar: {e}"),
                },
                FileMsg::Retune(id, r) => {
                    // un análisis viejo (se pidió otro clip mientras tanto) no reemplaza al que se espera
                    if self.retune_cargando != Some(id) {
                        continue;
                    }
                    self.retune_cargando = None;
                    match r {
                        Ok(rt) => {
                            self.msg = format!("Retune: {} notas encontradas. Arrástralas para cambiarlas.", rt.notas.len());
                            self.retune = Some(rt);
                        }
                        Err(e) => self.msg = format!("No se pudo analizar la voz: {e}"),
                    }
                }
                FileMsg::RetuneListo(id, clave, data) => self.retune_listo(id, clave, data),
                FileMsg::Armonia(id, que, r) => match r {
                    Ok((name, b)) => {
                        let b = Arc::new(b);
                        self.wave_peaks.insert(name.clone(), (Arc::new(b.peaks(4000)), b.secs()));
                        self.files.insert(name.clone(), b.clone());
                        self.cmd(Cmd::File { key: name.clone(), buf: Some(b) });
                        if let Some(c) = self.p.audio_clip(id).cloned() {
                            self.snap();
                            let track = self.free_track(c.start, c.len);
                            self.ensure_tracks(track + 1);
                            let nid = self.p.new_id();
                            // la armonía va un poco más baja que la voz principal
                            self.p.audio_clips.push(AudioClip { id: nid, track, takes: vec![name], active: 0, gain: c.gain * 0.7, ..c });
                            if (track as usize) < self.p.tracks.len() && self.p.tracks[track as usize].starts_with("Pista ") {
                                self.p.tracks[track as usize] = format!("Armonía ({que})");
                            }
                            self.msg = format!("Armonía ({que}) creada en la pista {}. Ábrela en «Retune» para ajustar notas sueltas.", track + 1);
                            self.send();
                        }
                    }
                    Err(e) => self.msg = format!("No se pudo crear la armonía: {e}"),
                },
                FileMsg::Preview(r) => match r {
                    Ok(b) => self.cmd(Cmd::PreviewFile(Some(Arc::new(b)))),
                    Err(e) => self.msg = format!("No se puede escuchar: {e}"),
                },
            }
        }
    }

    /// Decodifica y registra un archivo ya copiado en la carpeta de samples.
    fn load_now(&mut self, name: &str) -> Option<Arc<Buf>> {
        if let Some(b) = self.files.get(name) {
            return Some(b.clone());
        }
        let path = media::resolve(&self.plat.data_dir, name)?;
        match media::decode(&path) {
            Ok(b) => {
                let b = Arc::new(b);
                self.wave_peaks.insert(name.to_string(), (Arc::new(b.peaks(4000)), b.secs()));
                self.files.insert(name.to_string(), b.clone());
                self.cmd(Cmd::File { key: name.to_string(), buf: Some(b.clone()) });
                Some(b)
            }
            Err(e) => {
                self.msg = e;
                None
            }
        }
    }

    pub(super) fn preview_path(&mut self, path: &Path) {
        let tx = self.file_tx.clone();
        let p = path.to_path_buf();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(FileMsg::Preview(media::decode(&p)));
            if let Some(c) = ctx {
                c.request_repaint();
            }
        });
    }
    pub(super) fn stop_preview(&mut self) {
        self.cmd(Cmd::PreviewFile(None));
    }

    /// Archivo externo → canal NN Sampler nuevo.
    pub(super) fn import_to_rack(&mut self, path: &Path) {
        let name = match media::import_file(&self.plat.data_dir, path) {
            Ok(n) => n,
            Err(e) => {
                self.msg = e;
                return;
            }
        };
        let Some(b) = self.load_now(&name) else { return };
        if self.p.channels.len() >= crate::engine::MAX_CH {
            self.msg = "No caben más canales.".into();
            return;
        }
        self.snap();
        let id = self.p.new_id();
        let mut c = Channel::new(id, Inst::Sampler);
        c.name = path.file_stem().map(|s| s.to_string_lossy().chars().take(22).collect()).unwrap_or_else(|| "Sample".into());
        c.smp.file = name;
        c.smp.oneshot = true;
        c.vol = 0.8;
        if b.secs() > 1.2 {
            // probablemente un loop: modo slicer listo para usar
            c.smp.slices = media::onsets(&b, 0.5, 16);
        }
        self.p.channels.push(c);
        self.sel_ch = id;
        self.msg = format!("Sample «{}» en el Channel Rack ({:.1} s). Ábrelo con su botón NN Sampler.", self.p.channel(id).map(|c| c.name.clone()).unwrap_or_default(), b.secs());
        self.send();
    }

    /// Archivo externo → clip de audio en la playlist (en una pista de audio).
    pub(super) fn import_to_playlist(&mut self, path: &Path, track: u32, step: u32) {
        let name = match media::import_file(&self.plat.data_dir, path) {
            Ok(n) => n,
            Err(e) => {
                self.msg = e;
                return;
            }
        };
        let Some(b) = self.load_now(&name) else { return };
        self.snap();
        let ch = self.audio_channel_for(path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default());
        let low = name.to_lowercase();
        let is_loop = low.contains("bpm") || low.contains("loop");
        let bpm = if is_loop { media::guess_bpm(&name, b.secs()) } else { 0.0 };
        let secs = if bpm > 0.0 { b.secs() * bpm / self.p.bpm } else { b.secs() };
        let len = ((secs as f64 / self.p.step_secs()).ceil() as u32).max(1);
        let len = if bpm > 0.0 { len.div_ceil(4) * 4 } else { len };
        let id = self.p.new_id();
        self.p.audio_clips.push(AudioClip { id, ch, track, start: step, len, off: 0.0, takes: vec![name], active: 0, gain: 1.0, pitch: 0.0, bpm, fade_in: 0.0, fade_out: 0.0, reverse: false });
        self.ensure_tracks(track + 1);
        self.msg = if bpm > 0.0 { format!("Loop importado y ajustado de {bpm:.0} a {:.0} bpm.", self.p.bpm) } else { format!("Audio importado ({:.1} s).", b.secs()) };
        self.send();
    }

    /// Canal de audio para un clip nuevo: el seleccionado si es de audio, si no uno nuevo.
    fn audio_channel_for(&mut self, name: String) -> u32 {
        if let Some(c) = self.p.channel(self.sel_ch).filter(|c| c.inst == Inst::Audio) {
            return c.id;
        }
        let id = self.p.new_id();
        let mut c = Channel::new(id, Inst::Audio);
        c.name = if name.is_empty() { "Audio".into() } else { name.chars().take(22).collect() };
        c.vol = 0.8;
        self.p.channels.push(c);
        self.sel_ch = id;
        id
    }

    pub(super) fn ensure_tracks(&mut self, n: u32) {
        while (self.p.tracks.len() as u32) < n {
            let k = self.p.tracks.len() + 1;
            self.p.tracks.push(format!("Pista {k}"));
        }
    }

    pub(super) fn pick_audio(&self) -> Option<PathBuf> {
        #[cfg(not(target_os = "android"))]
        {
            rfd::FileDialog::new().set_title("Elegir audio").add_filter("Audio", &media::AUDIO_EXT).set_directory(&self.plat.export_dir).pick_file()
        }
        #[cfg(target_os = "android")]
        {
            None
        }
    }

    /// Selector de audio del teléfono: muestra tus carpetas y archivos de audio; al tocar uno se importa.
    pub(super) fn selector_audio(&mut self, ctx: &egui::Context) {
        let Some(dir) = self.elegir_audio.clone() else { return };
        // al abrir el selector se pide (una vez) el permiso de los archivos de audio
        #[cfg(target_os = "android")]
        if dir.is_none() && self.elegir_cache.is_none() {
            audio::pedir_permisos(&["android.permission.READ_MEDIA_AUDIO", "android.permission.READ_EXTERNAL_STORAGE"]);
            self.elegir_cache = Some((PathBuf::new(), vec![]));
        }
        // contenido de la carpeta (se lee una vez al entrar)
        let lista: Vec<(String, PathBuf, bool)> = match &dir {
            None => {
                let mut v: Vec<(String, PathBuf, bool)> = self.browser_roots().into_iter().map(|(n, p)| (n, p, true)).collect();
                #[cfg(target_os = "android")]
                for (n, d) in [("Audios de WhatsApp", "/storage/emulated/0/Android/media/com.whatsapp/WhatsApp/Media/WhatsApp Audio"), ("Documentos", "/storage/emulated/0/Documents"), ("Todo el teléfono", "/storage/emulated/0")] {
                    let p = PathBuf::from(d);
                    if p.is_dir() {
                        v.push((n.to_string(), p, true));
                    }
                }
                v
            }
            Some(d) => {
                if self.elegir_cache.as_ref().map(|c| &c.0) != Some(d) {
                    let mut v: Vec<(String, PathBuf, bool)> = std::fs::read_dir(d)
                        .map(|it| {
                            it.filter_map(|e| e.ok())
                                .map(|e| e.path())
                                .filter(|p| !p.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.')))
                                .filter(|p| p.is_dir() || media::is_audio(p))
                                .map(|p| (p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(), p.clone(), p.is_dir()))
                                .collect()
                        })
                        .unwrap_or_default();
                    v.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.to_lowercase().cmp(&b.0.to_lowercase())));
                    self.elegir_cache = Some((d.clone(), v));
                }
                self.elegir_cache.as_ref().map(|c| c.1.clone()).unwrap_or_default()
            }
        };
        let pantalla = ctx.content_rect();
        let mut ir: Option<Option<PathBuf>> = None;
        let mut cerrar = false;
        let mut elegido: Option<PathBuf> = None;
        egui::Area::new(egui::Id::new("sel_fondo")).order(egui::Order::Foreground).fixed_pos(pantalla.min).show(ctx, |ui| {
            let (r, resp) = ui.allocate_exact_size(pantalla.size(), Sense::click());
            ui.painter().rect_filled(r, 0, Color32::from_black_alpha(160));
            if resp.clicked() {
                cerrar = true;
            }
        });
        let ancho = (pantalla.width() - 40.0).min(620.0).max(200.0);
        let alto = (pantalla.height() - 40.0).min(560.0);
        egui::Area::new(egui::Id::new("sel_audio")).order(egui::Order::Tooltip).anchor(Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
            Frame::new().fill(PANEL).stroke(Stroke::new(1.0, LINE)).corner_radius(16).inner_margin(Margin::same(14)).show(ui, |ui| {
                ui.set_width(ancho);
                ui.set_height(alto);
                ui.horizontal(|ui| {
                    if dir.is_some() && ui.add(egui::Button::new("Atrás").min_size(Vec2::new(80.0, 34.0))).clicked() {
                        // a la carpeta de arriba (o a la lista de lugares)
                        let arriba = dir.as_ref().and_then(|d| d.parent().map(|p| p.to_path_buf())).filter(|p| p.starts_with("/storage/emulated/0") || !cfg!(target_os = "android"));
                        let raiz = self.browser_roots().iter().any(|(_, p)| Some(p) == dir.as_ref());
                        ir = Some(if raiz { None } else { arriba });
                    }
                    ui.label(RichText::new(if dir.is_none() { "Elegir audio" } else { "Elige un audio o una carpeta" }).strong().size(16.0).color(Color32::WHITE));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.add(egui::Button::new("Cerrar").min_size(Vec2::new(80.0, 34.0))).clicked() {
                            cerrar = true;
                        }
                    });
                });
                if let Some(d) = &dir {
                    ui.add(egui::Label::new(RichText::new(d.display().to_string()).small().color(DIM)).truncate());
                }
                ui.add_space(6.0);
                egui::ScrollArea::vertical().id_salt("sel_lista").auto_shrink([false, false]).show(ui, |ui| {
                    if lista.is_empty() {
                        ui.label(RichText::new("No hay audios aquí. Si el teléfono pidió permiso para acceder a tus archivos de música, acéptalo.").color(DIM));
                    }
                    for (n, p, carpeta) in &lista {
                        let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 42.0), Sense::click());
                        let pt = ui.painter();
                        if resp.hovered() || resp.is_pointer_button_down_on() {
                            pt.rect_filled(r, 10, Color32::from_gray(32));
                        }
                        // ícono: carpeta o nota
                        let c = Pos2::new(r.left() + 20.0, r.center().y);
                        if *carpeta {
                            pt.rect_filled(Rect::from_center_size(c + Vec2::new(0.0, 2.0), Vec2::new(20.0, 13.0)), 2, Color32::from_gray(150));
                            pt.rect_filled(Rect::from_min_size(c + Vec2::new(-10.0, -7.0), Vec2::new(9.0, 4.0)), 1, Color32::from_gray(150));
                        } else {
                            pt.circle_filled(c + Vec2::new(-3.0, 5.0), 4.0, ACCENT);
                            pt.line_segment([c + Vec2::new(1.0, 5.0), c + Vec2::new(1.0, -8.0)], Stroke::new(1.8, ACCENT));
                            pt.line_segment([c + Vec2::new(1.0, -8.0), c + Vec2::new(7.0, -5.0)], Stroke::new(1.8, ACCENT));
                        }
                        pt.with_clip_rect(r.shrink(2.0)).text(Pos2::new(r.left() + 42.0, r.center().y), Align2::LEFT_CENTER, n, FontId::proportional(14.5), if *carpeta { Color32::WHITE } else { Color32::from_gray(225) });
                        if resp.clicked() {
                            if *carpeta {
                                ir = Some(Some(p.clone()));
                            } else {
                                elegido = Some(p.clone());
                            }
                        }
                    }
                });
            });
        });
        if let Some(p) = elegido {
            let t = self.free_track(self.play_from, 16);
            self.import_to_playlist(&p, t, self.play_from);
            cerrar = true;
        }
        if let Some(d) = ir {
            self.elegir_audio = Some(d);
            self.elegir_cache = None;
        }
        if cerrar {
            self.elegir_audio = None;
            self.elegir_cache = None;
        }
    }

    /// Archivos soltados sobre la ventana desde el explorador.
    pub(super) fn handle_dropped(&mut self, ctx: &egui::Context) {
        let files: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|p| p.is_file()).collect());
        if files.is_empty() {
            return;
        }
        let over_pl = ctx.input(|i| i.pointer.hover_pos()).zip(self.pl_rect).filter(|(p, r)| r.contains(*p));
        for (k, f) in files.iter().enumerate() {
            let ext = f.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
            if ext == "mid" || ext == "midi" {
                self.import_midi_path(f);
            } else if ext == "nns" || ext == "json" {
                self.open_path(f.clone());
            } else if media::is_audio(f) {
                match over_pl {
                    Some((p, _)) => {
                        let (step, track) = self.pl_cell(p);
                        self.import_to_playlist(f, track + k as u32, step);
                    }
                    None => self.import_to_rack(f),
                }
            }
        }
    }

    // ============================ navegador ============================

    fn browser_roots(&self) -> Vec<(String, PathBuf)> {
        let mut v = vec![("Samples del proyecto".to_string(), media::samples_dir(&self.plat.data_dir)), ("Grabaciones".to_string(), self.plat.data_dir.join("audio"))];
        #[cfg(not(target_os = "android"))]
        {
            let home = std::env::var("USERPROFILE").map(PathBuf::from).unwrap_or_else(|_| ".".into());
            for (n, d) in [("Escritorio", "Desktop"), ("Música", "Music"), ("Descargas", "Downloads"), ("Documentos", "Documents")] {
                let p = home.join(d);
                if p.is_dir() {
                    v.push((n.to_string(), p));
                }
            }
        }
        #[cfg(target_os = "android")]
        {
            for (n, d) in [("Música", "/storage/emulated/0/Music"), ("Descargas", "/storage/emulated/0/Download"), ("Grabaciones del teléfono", "/storage/emulated/0/Recordings")] {
                let p = PathBuf::from(d);
                if p.is_dir() {
                    v.push((n.to_string(), p));
                }
            }
        }
        for f in &self.settings.fav_dirs {
            let p = PathBuf::from(f);
            if p.is_dir() {
                v.push((p.file_name().map(|n| format!("Favorito: {}", n.to_string_lossy())).unwrap_or_else(|| f.clone()), p));
            }
        }
        v
    }

    fn browser_load(&mut self) {
        let Some(d) = self.browser.dir.clone() else { return };
        if self.browser.loaded.as_ref() == Some(&d) {
            return;
        }
        let _ = std::fs::create_dir_all(&d);
        let mut v: Vec<(PathBuf, bool)> = std::fs::read_dir(&d)
            .map(|rd| {
                rd.flatten()
                    .filter_map(|e| {
                        let p = e.path();
                        let name = p.file_name()?.to_string_lossy().to_string();
                        if name.starts_with('.') {
                            return None;
                        }
                        let dir = p.is_dir();
                        let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
                        (dir || media::is_audio(&p) || ext == "mid" || ext == "midi").then_some((p, dir))
                    })
                    .collect()
            })
            .unwrap_or_default();
        v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.file_name().map(|n| n.to_string_lossy().to_lowercase()).cmp(&b.0.file_name().map(|n| n.to_string_lossy().to_lowercase()))));
        self.browser.list = v;
        self.browser.loaded = Some(d);
    }

    pub(super) fn browser_panel(&mut self, ui: &mut Ui) {
        #[cfg(target_os = "android")]
        {
            let _ = audio::request_perm("android.permission.READ_MEDIA_AUDIO") || audio::request_perm("android.permission.READ_EXTERNAL_STORAGE");
        }
        ui.label(RichText::new("NAVEGADOR").small().color(DIM));
        match self.browser.dir.clone() {
            None => {
                for (n, p) in self.browser_roots() {
                    if ui.add(egui::Button::new(RichText::new(format!("{n} /")).size(12.5)).min_size(Vec2::new(ui.available_width(), 24.0))).clicked() {
                        self.browser.dir = Some(p);
                        self.browser.loaded = None;
                    }
                }
                ui.add_space(6.0);
                ui.label(RichText::new("Clic: escuchar · doble clic: al Channel Rack · arrastrar: a la playlist o al rack").small().color(DIM));
            }
            Some(d) => {
                self.browser_load();
                ui.horizontal(|ui| {
                    if ui.small_button("Subir").clicked() {
                        let roots: Vec<PathBuf> = self.browser_roots().into_iter().map(|r| r.1).collect();
                        self.browser.dir = if roots.contains(&d) { None } else { d.parent().map(|p| p.to_path_buf()) };
                        self.browser.loaded = None;
                    }
                    if ui.small_button("Inicio").clicked() {
                        self.browser.dir = None;
                    }
                    let fav = self.settings.fav_dirs.contains(&d.to_string_lossy().to_string());
                    if ui.add(egui::Button::selectable(fav, "Favorito")).on_hover_text("Mostrar esta carpeta al inicio del navegador").clicked() {
                        let s = d.to_string_lossy().to_string();
                        if fav {
                            self.settings.fav_dirs.retain(|x| *x != s);
                        } else {
                            self.settings.fav_dirs.push(s);
                        }
                        self.save_settings();
                    }
                    ui.label(RichText::new(d.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()).small().strong());
                });
                ui.add(egui::TextEdit::singleline(&mut self.browser.filter).hint_text("Buscar…").desired_width(f32::INFINITY));
                let filt = self.browser.filter.to_lowercase();
                let list = self.browser.list.clone();
                let mut go = None;
                let mut prev = None;
                let mut to_rack = None;
                let mut midi_in = None;
                if list.is_empty() {
                    ui.label(RichText::new("Carpeta vacía").small().color(DIM));
                }
                for (p, dir) in list {
                    let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    if !filt.is_empty() && !name.to_lowercase().contains(&filt) {
                        continue;
                    }
                    let sel = self.browser.sel.as_ref() == Some(&p);
                    let label = if dir { format!("{name} /") } else if name.to_lowercase().ends_with(".mid") || name.to_lowercase().ends_with(".midi") { format!("MIDI · {name}") } else { name.clone() };
                    let id = egui::Id::new(("br", &p));
                    let resp = if dir {
                        ui.add(egui::Button::selectable(sel, RichText::new(label).size(12.0)).min_size(Vec2::new(ui.available_width(), 20.0)))
                    } else {
                        let r = ui.dnd_drag_source(id, p.clone(), |ui| {
                            ui.add(egui::Button::selectable(sel, RichText::new(label.clone()).size(12.0)).min_size(Vec2::new(ui.available_width(), 20.0)))
                        });
                        r.inner
                    };
                    if resp.clicked() {
                        if dir {
                            go = Some(p.clone());
                        } else {
                            self.browser.sel = Some(p.clone());
                            if media::is_audio(&p) {
                                prev = Some(p.clone());
                            }
                        }
                    }
                    if !dir && resp.double_clicked() {
                        if media::is_audio(&p) {
                            to_rack = Some(p.clone());
                        } else {
                            midi_in = Some(p.clone());
                        }
                    }
                }
                if let Some(g) = go {
                    self.browser.dir = Some(g);
                    self.browser.loaded = None;
                }
                if let Some(p) = prev {
                    self.preview_path(&p);
                }
                if let Some(p) = to_rack {
                    self.stop_preview();
                    self.import_to_rack(&p);
                }
                if let Some(p) = midi_in {
                    self.import_midi_path(&p);
                }
                if let Some(sel) = self.browser.sel.clone().filter(|s| media::is_audio(s)) {
                    ui.separator();
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("Parar").clicked() {
                            self.stop_preview();
                        }
                        if ui.button("Al rack").on_hover_text("Nuevo canal NN Sampler").clicked() {
                            self.stop_preview();
                            self.import_to_rack(&sel);
                        }
                        if ui.button("A la playlist").on_hover_text("Clip de audio al inicio de una pista libre").clicked() {
                            self.stop_preview();
                            let t = self.free_track(0, 64);
                            self.import_to_playlist(&sel, t, self.play_from);
                        }
                    });
                }
            }
        }
    }

    // ============================ playlist ============================

    /// (paso, pista) bajo una posición de pantalla de la playlist.
    pub(super) fn pl_cell(&self, p: Pos2) -> (u32, u32) {
        let org = self.pl_org;
        let step = (((p.x - org.x) / self.pl_bw * STEPS_PER_BAR as f32).max(0.0)) as u32;
        let step = step / self.snap_steps.max(1) * self.snap_steps.max(1);
        let tr = (((p.y - org.y) / TH).floor().max(0.0) as u32).min(self.p.tracks.len().saturating_sub(1) as u32);
        (step, tr)
    }

    /// Primera pista sin clips entre [start, start+len).
    pub(super) fn free_track(&self, start: u32, len: u32) -> u32 {
        for t in 0..64u32 {
            let busy = self.p.playlist.iter().any(|c| c.track == t && self.p.pattern(c.pat).is_some_and(|p| c.bar * 16 < start + len && (c.bar + p.bars) * 16 > start))
                || self.p.audio_clips.iter().any(|c| c.track == t && c.start < start + len && c.start + c.len > start)
                || self.p.autos.iter().any(|c| c.track == t && c.start < start + len && c.start + c.len > start);
            if !busy {
                return t;
            }
        }
        self.p.tracks.len() as u32
    }

    pub(super) fn playlist_toolbar(&mut self, ui: &mut Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.add_space(6.0);
            ui.label(RichText::new("Playlist").strong());
            ui.label(RichText::new("REJILLA").small().color(DIM));
            let names = [(16u32, "Compás"), (4, "Tiempo"), (1, "1/16")];
            let cur = names.iter().find(|n| n.0 == self.snap_steps).map(|n| n.1).unwrap_or("…");
            egui::ComboBox::from_id_salt("snap").width(70.0).selected_text(cur).show_ui(ui, |ui| {
                for (v, n) in names {
                    ui.selectable_value(&mut self.snap_steps, v, n);
                }
            });
            let mut lp = self.p.loop_on;
            if ui.add(egui::Button::selectable(lp, "Bucle")).on_hover_text("Repite la región marcada en la regla (arrastra sobre la regla para marcarla)").clicked() {
                self.snap();
                lp = !lp;
                self.p.loop_on = lp;
                self.send();
            }
            let pn = self.p.punch;
            if ui.add(egui::Button::selectable(pn, "Punch")).on_hover_text("Al grabar audio solo se conserva lo que cae dentro de la región").clicked() {
                self.snap();
                self.p.punch = !pn;
                self.send();
            }
            if ui.button("Alejar").on_hover_text("Ver más compases (Ctrl + rueda)").clicked() {
                self.pl_bw = (self.pl_bw / 1.3).max(16.0);
            }
            if ui.button("Acercar").on_hover_text("Ver los compases más anchos (Ctrl + rueda)").clicked() {
                self.pl_bw = (self.pl_bw * 1.3).min(240.0);
            }
            if ui.button("Importar audio…").clicked() {
                // en el teléfono se abre el selector de archivos del teléfono; en la PC, el de Windows
                if self.plat.mobile {
                    self.elegir_audio = Some(None);
                } else if let Some(p) = self.pick_audio() {
                    let t = self.free_track(self.play_from, 16);
                    self.import_to_playlist(&p, t, self.play_from);
                }
            }
            if self.settings.ayudas {
                if !self.plat.mobile {
                    ui.label(RichText::new("Clic: patrón · arrastrar: mover · borde: largo · clic derecho: menú · regla: tocar / arrastrar: bucle").small().color(DIM));
                }
            }
        });
    }

    /// Cabeceras de las pistas (número, nombre, M, S, volumen y paneo) y la esquina «+ Añadir pista».
    fn cabeceras_pista(&mut self, ui: &mut Ui, rect: Rect, org: Pos2, tn: f32, rul: f32) {
        let clip = ui.clip_rect().intersect(rect);
        let hx = clip.left().max(rect.left());
        let hp = ui.painter().with_clip_rect(clip);
        let ntr = self.p.tracks.len();
        let solo_alguna = self.p.pistas.iter().any(|p| p.solo);
        let mut cambio = false;
        let ambar = Color32::from_rgb(248, 198, 62);
        for t in 0..ntr {
            let y0 = org.y + t as f32 * TH;
            let hr = Rect::from_min_size(Pos2::new(hx, y0), Vec2::new(tn, TH));
            if !clip.intersects(hr) {
                continue;
            }
            let fondo = ui.interact(hr, ui.id().with(("pista_cab", t)), Sense::click());
            let pista = self.p.pista(t);
            let suena = !(pista.mute || (solo_alguna && !pista.solo));
            if !suena {
                // la pista no suena: su carril se ve apagado
                let carril = Rect::from_min_size(Pos2::new(org.x, y0), Vec2::new(rect.right() - org.x, TH));
                hp.rect_filled(carril, 0, Color32::from_black_alpha(90));
            }
            hp.rect_filled(hr, 0, if fondo.hovered() { Color32::from_gray(32) } else { Color32::from_gray(26) });
            hp.hline(hr.x_range(), hr.bottom() - 0.5, Stroke::new(1.0, Color32::from_gray(15)));
            hp.vline(hr.right() - 0.5, hr.y_range(), Stroke::new(1.0, Color32::from_gray(15)));
            // color de la pista: el de su primer clip
            let tiene_audio = self.p.audio_clips.iter().any(|c| c.track as usize == t);
            let col = self.p.playlist.iter().find(|c| c.track as usize == t).and_then(|c| self.p.pattern(c.pat)).map(|p| c32(p.color)).unwrap_or(if tiene_audio { ACCENT } else { Color32::from_gray(76) });
            hp.rect_filled(Rect::from_min_size(hr.min, Vec2::new(3.0, TH)), 0, if suena { col } else { col.gamma_multiply(0.4) });
            // ícono: micrófono (audio) o nota (patrones)
            let ic = Pos2::new(hr.left() + 17.0, y0 + 19.0);
            let tinta = if suena { ACCENT2 } else { DIM };
            if tiene_audio {
                hp.rect_filled(Rect::from_center_size(ic - Vec2::new(0.0, 2.0), Vec2::new(6.0, 10.0)), 3, tinta);
                hp.add(egui::Shape::line((0..=10).map(|i| {
                    let a = std::f32::consts::PI * i as f32 / 10.0;
                    ic + Vec2::new(-a.cos() * 5.5, a.sin() * 5.0)
                }).collect(), Stroke::new(1.3, tinta)));
                hp.line_segment([ic + Vec2::new(0.0, 5.0), ic + Vec2::new(0.0, 8.0)], Stroke::new(1.3, tinta));
            } else {
                hp.circle_filled(ic + Vec2::new(-2.0, 4.0), 3.2, tinta);
                hp.line_segment([ic + Vec2::new(1.0, 4.0), ic + Vec2::new(1.0, -7.0)], Stroke::new(1.5, tinta));
                hp.line_segment([ic + Vec2::new(1.0, -7.0), ic + Vec2::new(5.5, -4.5)], Stroke::new(1.5, tinta));
            }
            // número y nombre
            let nombre_r = Rect::from_min_max(Pos2::new(hr.left() + 30.0, y0 + 8.0), Pos2::new(hr.right() - 66.0, y0 + 30.0));
            hp.with_clip_rect(nombre_r.intersect(clip)).text(Pos2::new(nombre_r.left(), nombre_r.center().y), Align2::LEFT_CENTER, format!("{:02}  {}", t + 1, self.p.tracks[t]), FontId::proportional(12.5), if suena { Color32::WHITE } else { DIM });
            // M y S
            for k in 0..2 {
                let r = Rect::from_min_size(Pos2::new(hr.right() - 60.0 + k as f32 * 27.0, y0 + 9.0), Vec2::new(23.0, 20.0));
                let on = if k == 0 { pista.mute } else { pista.solo };
                let rr = ui.interact(r, ui.id().with(("pista_ms", t, k)), Sense::click());
                let base = if rr.hovered() { Color32::from_gray(61) } else { Color32::from_gray(43) };
                hp.rect_filled(r, 5, if on { if k == 0 { ambar } else { ACCENT } } else { base });
                hp.text(r.center(), Align2::CENTER_CENTER, if k == 0 { "M" } else { "S" }, FontId::proportional(11.5), if on { Color32::from_gray(17) } else { Color32::WHITE });
                if rr.on_hover_text(if k == 0 { "Silenciar la pista" } else { "Solo: escuchar solo esta pista" }).clicked() {
                    self.snap();
                    let p = self.p.pista_mut(t);
                    if k == 0 {
                        p.mute = !p.mute;
                    } else {
                        p.solo = !p.solo;
                    }
                    cambio = true;
                }
            }
            // volumen: riel con perilla blanca
            let vr = Rect::from_min_size(Pos2::new(hr.left() + 16.0, y0 + TH - 28.0), Vec2::new(tn - 90.0, 18.0));
            let vresp = ui.interact(vr, ui.id().with(("pista_vol", t)), Sense::click_and_drag());
            let maxv = 1.5f32;
            if vresp.drag_started() || vresp.clicked() {
                self.snap();
            }
            if vresp.dragged() || vresp.clicked() {
                if let Some(pp) = vresp.interact_pointer_pos() {
                    self.p.pista_mut(t).vol = ((pp.x - vr.left()) / vr.width() * maxv).clamp(0.0, maxv);
                    cambio = true;
                }
            }
            if vresp.double_clicked() {
                self.p.pista_mut(t).vol = 1.0;
                cambio = true;
            }
            let vol = self.p.pista(t).vol;
            let cy = vr.center().y;
            let xk = vr.left() + vol / maxv * vr.width();
            hp.line_segment([Pos2::new(vr.left(), cy), Pos2::new(vr.right(), cy)], Stroke::new(3.0, Color32::from_gray(52)));
            hp.line_segment([Pos2::new(vr.left(), cy), Pos2::new(xk, cy)], Stroke::new(3.0, if suena { ACCENT } else { DIM }));
            hp.circle_filled(Pos2::new(xk, cy), if vresp.hovered() || vresp.dragged() { 7.0 } else { 6.0 }, Color32::WHITE);
            let db = if vol > 0.0001 { format!("{:+.1} dB", 20.0 * vol.log10()) } else { "-∞".into() };
            vresp.on_hover_text(format!("Volumen de la pista: {db} · doble clic: 0 dB"));
            // paneo: perilla blanca con L y R
            let pc = Pos2::new(hr.right() - 32.0, y0 + TH - 22.0);
            let pr = Rect::from_center_size(pc, Vec2::new(28.0, 28.0));
            let presp = ui.interact(pr, ui.id().with(("pista_pan", t)), Sense::click_and_drag());
            if presp.drag_started() {
                self.snap();
            }
            if presp.dragged() {
                let d = presp.drag_delta();
                let p = self.p.pista_mut(t);
                p.pan = (p.pan + (d.x - d.y) / 90.0).clamp(-1.0, 1.0);
                cambio = true;
            }
            if presp.double_clicked() {
                self.p.pista_mut(t).pan = 0.0;
                cambio = true;
            }
            let pan = self.p.pista(t).pan;
            hp.circle_filled(pc, 10.0, if suena { Color32::WHITE } else { Color32::from_gray(155) });
            let a = -std::f32::consts::FRAC_PI_2 + pan * std::f32::consts::PI * 0.75;
            hp.line_segment([pc + Vec2::new(a.cos(), a.sin()) * 2.5, pc + Vec2::new(a.cos(), a.sin()) * 8.5], Stroke::new(2.0, Color32::from_gray(17)));
            hp.text(pc + Vec2::new(-15.0, 9.0), Align2::CENTER_CENTER, "L", FontId::proportional(8.5), DIM);
            hp.text(pc + Vec2::new(15.0, 9.0), Align2::CENTER_CENTER, "R", FontId::proportional(8.5), DIM);
            let pt = if pan.abs() < 0.01 { "al centro".to_string() } else if pan < 0.0 { format!("{:.0} % izquierda", -pan * 100.0) } else { format!("{:.0} % derecha", pan * 100.0) };
            presp.on_hover_text(format!("Paneo: {pt} · doble clic: al centro"));
        }
        // esquina de arriba: «+ Añadir pista»
        let esq = Rect::from_min_size(Pos2::new(hx, rect.top()), Vec2::new(tn, rul));
        hp.rect_filled(esq, 0, Color32::from_gray(19));
        hp.vline(esq.right() - 0.5, esq.y_range(), Stroke::new(1.0, Color32::from_gray(15)));
        let br = Rect::from_min_size(Pos2::new(esq.left() + 8.0, esq.top() + 3.0), Vec2::new(122.0, rul - 6.0));
        let bresp = ui.interact(br, ui.id().with("pista_nueva"), Sense::click());
        hp.rect_filled(br, 11, if bresp.hovered() { Color32::from_gray(56) } else { Color32::from_gray(41) });
        hp.text(br.center(), Align2::CENTER_CENTER, "+  Añadir pista", FontId::proportional(12.0), Color32::WHITE);
        if bresp.clicked() {
            self.snap();
            let n = self.p.tracks.len() as u32 + 1;
            self.ensure_tracks(n);
            self.send();
        }
        if cambio {
            self.cambios = true;
            self.send();
        }
    }

    pub(super) fn playlist_canvas(&mut self, ui: &mut Ui) {
        let bw = self.pl_bw;
        let song = self.p.song_bars();
        let bars = (song + 8).max(32);
        // cabeceras de pista más angostas en el teléfono
        let (tn, rul) = (if self.plat.mobile { 168.0 } else { 236.0 }, 28.0);
        let ntr = self.p.tracks.len();
        let size = Vec2::new(tn + bars as f32 * bw, rul + ntr as f32 * TH);
        let (rect, resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        let org = rect.min + Vec2::new(tn, rul);
        self.pl_org = org;
        self.pl_rect = Some(ui.clip_rect().intersect(rect));
        self.tut_mark("playlist", ui.clip_rect().intersect(rect));
        let spx = bw / STEPS_PER_BAR as f32;
        for t in 0..ntr {
            let r = Rect::from_min_size(Pos2::new(org.x, org.y + t as f32 * TH), Vec2::new(bars as f32 * bw, TH));
            painter.rect_filled(r, 0, if t % 2 == 0 { Color32::from_gray(31) } else { Color32::from_gray(27) });
            painter.hline(rect.left()..=rect.right(), r.bottom() - 0.5, Stroke::new(1.0, Color32::from_gray(20)));
        }
        painter.rect_filled(Rect::from_min_size(rect.min, Vec2::new(rect.width(), rul)), 0, Color32::from_gray(19));
        painter.hline(rect.x_range(), rect.top() + rul - 0.5, Stroke::new(1.0, Color32::from_gray(44)));
        // región de bucle / punch
        if self.p.loop_b > self.p.loop_a {
            let x0 = org.x + self.p.loop_a as f32 * bw;
            let x1 = org.x + self.p.loop_b as f32 * bw;
            let on = self.p.loop_on || self.p.punch;
            let col = if self.p.punch { Color32::from_rgb(200, 70, 70) } else { ACCENT };
            painter.rect_filled(Rect::from_min_max(Pos2::new(x0, rect.top() + 2.0), Pos2::new(x1, rect.top() + rul - 2.0)), 3, col.gamma_multiply(if on { 0.55 } else { 0.18 }));
            if on {
                painter.rect_filled(Rect::from_min_max(Pos2::new(x0, org.y), Pos2::new(x1, rect.bottom())), 0, col.gamma_multiply(0.06));
            }
        }
        for b in 0..=bars {
            let x = org.x + b as f32 * bw;
            painter.vline(x, org.y..=rect.bottom(), Stroke::new(1.0, if b % 4 == 0 { Color32::from_gray(17) } else { Color32::from_gray(23) }));
            if b < bars {
                painter.vline(x, (rect.top() + rul - 9.0)..=(rect.top() + rul), Stroke::new(1.0, Color32::from_gray(76)));
                painter.text(Pos2::new(x + 4.0, rect.top() + 13.0), Align2::LEFT_CENTER, format!("{}", b + 1), FontId::proportional(12.0), if b < song { Color32::WHITE } else { Color32::from_gray(64) });
            }
        }
        let pf = org.x + self.play_from as f32 * spx;
        painter.add(egui::Shape::convex_polygon(vec![Pos2::new(pf - 5.0, rect.top() + rul - 8.0), Pos2::new(pf + 5.0, rect.top() + rul - 8.0), Pos2::new(pf, rect.top() + rul)], ACCENT2, Stroke::NONE));
        // clips de patrón
        let pat_rect = |cl: &Clip, p: &Pattern| Rect::from_min_size(Pos2::new(org.x + cl.bar as f32 * bw + 1.0, org.y + cl.track as f32 * TH + 2.0), Vec2::new(p.bars as f32 * bw - 2.0, TH - 4.0));
        let step_rect = |track: u32, start: u32, len: u32| Rect::from_min_size(Pos2::new(org.x + start as f32 * spx + 1.0, org.y + track as f32 * TH + 2.0), Vec2::new((len as f32 * spx - 2.0).max(4.0), TH - 4.0));
        for cl in &self.p.playlist {
            let Some(p) = self.p.pattern(cl.pat) else { continue };
            let r = pat_rect(cl, p);
            let c = c32(p.color);
            painter.rect_filled(r, 5, lerp_col(c, BG, 0.5));
            let banda = Rect::from_min_size(r.min, Vec2::new(r.width(), 16.0));
            painter.rect_filled(banda, CornerRadius { nw: 5, ne: 5, sw: 0, se: 0 }, lerp_col(c, BG, 0.18));
            painter.with_clip_rect(banda.intersect(painter.clip_rect())).text(banda.left_center() + Vec2::new(5.0, 0.0), Align2::LEFT_CENTER, &p.name, FontId::proportional(11.0), Color32::WHITE);
            painter.rect_stroke(r, 5, Stroke::new(1.0, lerp_col(c, BG, 0.2)), StrokeKind::Inside);
            draw_pattern_preview(&painter, Rect::from_min_max(Pos2::new(r.left() + 1.0, r.top() + 18.0), r.right_bottom() - Vec2::new(1.0, 3.0)), p);
        }
        // clips de audio
        let ss = self.p.step_secs() as f32;
        for cl in &self.p.audio_clips {
            let r = step_rect(cl.track, cl.start, cl.len);
            let chn = self.p.channel(cl.ch);
            // como en BandLab: cuerpo azul, banda con el nombre y onda celeste llena
            let cuerpo = Color32::from_rgb(30, 86, 172);
            let onda = Color32::from_rgb(88, 164, 255);
            painter.rect_filled(r, 4, cuerpo);
            let banda = Rect::from_min_size(r.min, Vec2::new(r.width(), 16.0));
            painter.rect_filled(banda, CornerRadius { nw: 4, ne: 4, sw: 0, se: 0 }, Color32::from_rgb(25, 72, 146));
            painter.rect_stroke(r, 4, Stroke::new(1.0, Color32::from_rgb(64, 132, 236)), StrokeKind::Inside);
            let file = cl.file().to_string();
            if let Some((pk, secs)) = self.wave_peaks.get(&file) {
                let ratio = cl.ratio(self.p.bpm);
                let (arriba, abajo) = (r.top() + 18.0, r.bottom() - 3.0);
                let mid = (arriba + abajo) / 2.0;
                let h = (abajo - arriba) / 2.0;
                let n = pk.len();
                let por_px = ss / spx / ratio; // segundos del audio por píxel
                painter.hline((r.left() + 1.0)..=(r.right() - 1.0), mid, Stroke::new(1.0, Color32::from_rgb(52, 118, 214)));
                let mut x = r.left() + 1.0;
                while x < r.right() - 1.0 {
                    let t0 = cl.off + (x - r.left()) / spx * ss / ratio;
                    let (a0, a1) = if cl.reverse { (secs - t0 - por_px, secs - t0) } else { (t0, t0 + por_px) };
                    if a1 > 0.0 && a0 < *secs {
                        // el pico más alto de todo lo que cae en este píxel (la onda no parpadea al hacer zoom)
                        let i0 = ((a0.max(0.0) / secs) * n as f32) as usize;
                        let i1 = (((a1 / secs) * n as f32).ceil() as usize).clamp(i0 + 1, n);
                        let v = pk[i0.min(n - 1)..i1].iter().fold(0f32, |m, p| m.max(*p));
                        let v = (v * cl.gain).min(1.0).max(0.02);
                        painter.vline(x, (mid - v * h)..=(mid + v * h), Stroke::new(1.0, onda));
                    }
                    x += 1.0;
                }
            } else if !file.is_empty() {
                painter.text(r.center(), Align2::CENTER_CENTER, "cargando…", FontId::proportional(10.0), Color32::from_rgb(200, 220, 230));
            }
            let mut label = chn.map(|c| c.name.clone()).unwrap_or_default();
            if cl.takes.len() > 1 {
                label = format!("{label} · toma {}/{}", cl.active + 1, cl.takes.len());
            }
            if cl.bpm > 0.0 {
                label = format!("{label} · {:.0} a {:.0} bpm", cl.bpm, self.p.bpm);
            }
            if cl.pitch.abs() > 0.01 {
                label = format!("{label} · {:+.0} st", cl.pitch);
            }
            painter.with_clip_rect(banda.intersect(painter.clip_rect())).text(banda.left_center() + Vec2::new(5.0, 0.0), Align2::LEFT_CENTER, label, FontId::proportional(11.0), Color32::WHITE);
            if cl.fade_in > 0.0 {
                let w = (cl.fade_in / ss * spx).min(r.width());
                painter.line_segment([r.left_bottom(), Pos2::new(r.left() + w, r.top())], Stroke::new(1.0, Color32::from_black_alpha(160)));
            }
            if cl.fade_out > 0.0 {
                let w = (cl.fade_out / ss * spx).min(r.width());
                painter.line_segment([Pos2::new(r.right() - w, r.top()), r.right_bottom()], Stroke::new(1.0, Color32::from_black_alpha(160)));
            }
        }
        // grabación en curso
        if let Some(tr) = &self.rec_track {
            let now = self.status.pos();
            let s0 = tr.region.map(|r| r.0).unwrap_or(tr.from) as f32;
            let track = tr.track;
            let w = if tr.looped { tr.region.map(|r| (r.1 - r.0) as f32).unwrap_or(16.0) } else { (now as f32 - s0).max(0.5) };
            let r = Rect::from_min_size(Pos2::new(org.x + s0 * spx, org.y + track as f32 * TH + 2.0), Vec2::new(w * spx, TH - 4.0));
            painter.rect(r, 4, Color32::from_gray(28), Stroke::new(1.5, RED), StrokeKind::Inside);
            // forma de onda en vivo: en bucle se ve la vuelta actual
            let px_s = spx / self.p.step_secs() as f32;
            let grabado = self.rec_secs();
            let (desde, hasta) = if tr.looped {
                let vuelta = w * self.p.step_secs() as f32;
                let k = (grabado / vuelta.max(0.01)).floor();
                (k * vuelta, grabado)
            } else {
                (0.0, grabado)
            };
            self.rec_draw(&painter.with_clip_rect(r.intersect(painter.clip_rect())), r, px_s, desde, hasta);
            let vueltas = if tr.looped { format!(" · toma {}", (grabado / (w * self.p.step_secs() as f32).max(0.01)).floor() as u32 + 1) } else { String::new() };
            painter.with_clip_rect(r.intersect(painter.clip_rect())).text(r.left_top() + Vec2::new(5.0, 3.0), Align2::LEFT_TOP, format!("GRABANDO{vueltas}"), FontId::proportional(11.0), Color32::WHITE);
        }
        // automatización
        for a in &self.p.autos {
            let r = step_rect(a.track, a.start, a.len);
            painter.rect(r, 4, Color32::from_gray(41), Stroke::new(1.0, Color32::from_rgb(130, 110, 190)), StrokeKind::Inside);
            let inner = r.shrink2(Vec2::new(2.0, 4.0));
            let mut pts = vec![];
            let n = (inner.width() / 3.0).max(2.0) as usize;
            for i in 0..=n {
                let x = i as f32 / n as f32;
                pts.push(Pos2::new(inner.left() + x * inner.width(), inner.bottom() - a.at(x) * inner.height()));
            }
            painter.add(egui::Shape::line(pts, Stroke::new(1.5, Color32::from_rgb(190, 170, 250))));
            let name = a.target.channel().and_then(|c| self.p.channel(c)).map(|c| c.name.clone()).unwrap_or_else(|| "Master".into());
            painter.with_clip_rect(r.intersect(painter.clip_rect())).text(r.left_top() + Vec2::new(5.0, 2.0), Align2::LEFT_TOP, format!("{} · {}", name, self.auto_label(a.target)), FontId::proportional(10.5), Color32::from_rgb(220, 210, 255));
        }
        // cabezal
        if self.song_mode && self.playing() {
            let x = org.x + (self.status.pos() / 16.0) as f32 * bw;
            painter.vline(x, (rect.top() + 4.0)..=rect.bottom(), Stroke::new(1.5, Color32::WHITE));
            painter.circle_filled(Pos2::new(x, rect.top() + 4.0), 4.0, Color32::WHITE);
        }
        // zoom con Ctrl + rueda del ratón
        if resp.hovered() {
            // egui convierte Ctrl + rueda en un factor de zoom
            let z = ui.input(|i| i.zoom_delta());
            if (z - 1.0).abs() > 1e-3 {
                self.pl_bw = (self.pl_bw * z).clamp(16.0, 240.0);
            }
        }

        // ---------- cabeceras de pista: fijas a la izquierda, como en BandLab ----------
        self.cabeceras_pista(ui, rect, org, tn, rul);

        // ---------- interacción ----------
        let hit = |this: &Self, pos: Pos2| -> Option<(Kind, usize, Rect)> {
            for (i, a) in this.p.autos.iter().enumerate().rev() {
                let r = step_rect(a.track, a.start, a.len);
                if r.contains(pos) {
                    return Some((Kind::Auto, i, r));
                }
            }
            for (i, a) in this.p.audio_clips.iter().enumerate().rev() {
                let r = step_rect(a.track, a.start, a.len);
                if r.contains(pos) {
                    return Some((Kind::Audio, i, r));
                }
            }
            for (i, cl) in this.p.playlist.iter().enumerate().rev() {
                if let Some(p) = this.p.pattern(cl.pat) {
                    let r = pat_rect(cl, p);
                    if r.contains(pos) {
                        return Some((Kind::Pat, i, r));
                    }
                }
            }
            None
        };
        // arrastrar desde el navegador
        if let Some(path) = resp.dnd_release_payload::<PathBuf>() {
            if let Some(pos) = ui.input(|i| i.pointer.interact_pos()) {
                let (s, t) = self.pl_cell(pos);
                if media::is_audio(&path) {
                    self.stop_preview();
                    self.import_to_playlist(&path, t, s);
                }
            }
        }
        if resp.drag_started() {
            if let Some(pos) = ui.input(|i| i.pointer.press_origin()) {
                let (s, t) = self.pl_cell(pos);
                let raw_step = (((pos.x - org.x) / spx).max(0.0)) as i32;
                if pos.y < org.y {
                    let a = (s / 16).min(999);
                    self.snap();
                    self.p.loop_a = a;
                    self.p.loop_b = a + 1;
                    self.pl_drag = Some(PlDrag::Loop { a });
                } else if let Some((k, i, r)) = hit(self, pos) {
                    self.snap();
                    let edge = 7.0f32.min(r.width() / 3.0);
                    match k {
                        Kind::Pat => {
                            let b = ((pos.x - org.x) / bw).floor() as i32;
                            self.pl_drag = Some(PlDrag::Pat { idx: i, bar0: b, tr0: t as i32, orig: self.p.playlist[i], moved: false });
                        }
                        Kind::Audio => {
                            let c = &self.p.audio_clips[i];
                            self.pl_drag = Some(if pos.x > r.right() - edge {
                                PlDrag::Right { kind: k, id: c.id }
                            } else if pos.x < r.left() + edge {
                                PlDrag::Left { id: c.id, start: c.start, len: c.len, off: c.off }
                            } else {
                                PlDrag::Move { kind: k, id: c.id, s0: raw_step, tr0: t as i32, start: c.start, track: c.track, moved: false }
                            });
                        }
                        Kind::Auto => {
                            let c = &self.p.autos[i];
                            self.pl_drag = Some(if pos.x > r.right() - edge { PlDrag::Right { kind: k, id: c.id } } else { PlDrag::Move { kind: k, id: c.id, s0: raw_step, tr0: t as i32, start: c.start, track: c.track, moved: false } });
                        }
                    }
                }
            }
        }
        if self.pl_drag.is_some() {
            if let Some(pos) = resp.interact_pointer_pos() {
                let snap = self.snap_steps.max(1) as i32;
                let raw = (((pos.x - org.x) / spx).max(0.0)) as i32;
                let (_, t) = self.pl_cell(pos);
                let mut changed = false;
                match self.pl_drag.as_mut().unwrap() {
                    PlDrag::Loop { a } => {
                        let b = ((pos.x - org.x) / bw).max(0.0).ceil() as u32;
                        let (x, y) = if b > *a { (*a, b) } else { (b.saturating_sub(1).min(*a), *a + 1) };
                        if self.p.loop_a != x || self.p.loop_b != y {
                            self.p.loop_a = x;
                            self.p.loop_b = y.max(x + 1);
                            changed = true;
                        }
                    }
                    PlDrag::Pat { idx, bar0, tr0, orig, moved } => {
                        let b = ((pos.x - org.x) / bw).floor() as i32;
                        let nb = (orig.bar as i32 + b - *bar0).max(0) as u32;
                        let nt = (orig.track as i32 + t as i32 - *tr0).clamp(0, ntr as i32 - 1) as u32;
                        if let Some(cl) = self.p.playlist.get_mut(*idx) {
                            if cl.bar != nb || cl.track != nt {
                                cl.bar = nb;
                                cl.track = nt;
                                *moved = true;
                                changed = true;
                            }
                        }
                    }
                    PlDrag::Move { kind, id, s0, tr0, start, track, moved } => {
                        let d = ((raw - *s0) as f32 / snap as f32).round() as i32 * snap;
                        let ns = (*start as i32 + d).max(0) as u32;
                        let nt = (*track as i32 + t as i32 - *tr0).clamp(0, ntr as i32 - 1) as u32;
                        let id = *id;
                        let (cs, ct) = match kind {
                            Kind::Audio => self.p.audio_clips.iter_mut().find(|c| c.id == id).map(|c| (&mut c.start, &mut c.track)).unzip(),
                            _ => self.p.autos.iter_mut().find(|c| c.id == id).map(|c| (&mut c.start, &mut c.track)).unzip(),
                        };
                        if let (Some(cs), Some(ct)) = (cs, ct) {
                            if *cs != ns || *ct != nt {
                                *cs = ns;
                                *ct = nt;
                                *moved = true;
                                changed = true;
                            }
                        }
                    }
                    PlDrag::Right { kind, id } => {
                        let id = *id;
                        let start = match kind {
                            Kind::Audio => self.p.audio_clips.iter().find(|c| c.id == id).map(|c| c.start),
                            _ => self.p.autos.iter().find(|c| c.id == id).map(|c| c.start),
                        };
                        if let Some(st) = start {
                            let end = ((raw + snap / 2) / snap * snap).max(st as i32 + 1) as u32;
                            let nl = (end - st).max(1);
                            let l = match kind {
                                Kind::Audio => self.p.audio_clips.iter_mut().find(|c| c.id == id).map(|c| &mut c.len),
                                _ => self.p.autos.iter_mut().find(|c| c.id == id).map(|c| &mut c.len),
                            };
                            if let Some(l) = l {
                                if *l != nl {
                                    *l = nl;
                                    changed = true;
                                }
                            }
                        }
                    }
                    PlDrag::Left { id, start, len, off } => {
                        let end = *start + *len;
                        let ns = (((raw + snap / 2) / snap * snap).max(0) as u32).min(end - 1);
                        let (id, st0, off0) = (*id, *start, *off);
                        let ss = self.p.step_secs() as f32;
                        let bpm = self.p.bpm;
                        if let Some(c) = self.p.audio_clips.iter_mut().find(|c| c.id == id) {
                            let ratio = c.ratio(bpm);
                            let noff = off0 + (ns as f32 - st0 as f32) * ss / ratio;
                            if noff >= 0.0 && c.start != ns {
                                c.start = ns;
                                c.len = end - ns;
                                c.off = noff;
                                changed = true;
                            }
                        }
                    }
                }
                if changed {
                    self.cmd(Cmd::Project(Box::new(self.p.clone())));
                }
            }
            if resp.drag_stopped() {
                let d = self.pl_drag.take();
                let moved = match &d {
                    Some(PlDrag::Pat { moved, .. }) | Some(PlDrag::Move { moved, .. }) => *moved,
                    _ => true,
                };
                if let Some(PlDrag::Loop { .. }) = d {
                    self.p.loop_on = true;
                    self.msg = format!("Bucle: compases {} a {}.", self.p.loop_a + 1, self.p.loop_b);
                }
                if !moved {
                    self.hist.pop();
                }
                self.send();
            }
        }
        if resp.clicked() {
            if let Some(pos) = resp.interact_pointer_pos() {
                self.pl_menu = None;
                if pos.y < org.y {
                    let b = ((pos.x - org.x) / bw).floor().max(0.0) as u32;
                    self.play_from = b * STEPS_PER_BAR;
                    self.song_mode = true;
                    self.play(b * STEPS_PER_BAR);
                } else if pos.x >= org.x {
                    match hit(self, pos) {
                        Some((Kind::Pat, i, _)) => {
                            let id = self.p.playlist[i].pat;
                            self.select_pat(id);
                        }
                        Some((Kind::Audio, i, _)) => {
                            self.sel_ch = self.p.audio_clips[i].ch;
                        }
                        Some((Kind::Auto, i, _)) => {
                            let id = self.p.autos[i].id;
                            self.open_win(Win::Auto(id));
                        }
                        None => {
                            let b = ((pos.x - org.x) / bw).floor().max(0.0) as u32;
                            let (_, t) = self.pl_cell(pos);
                            self.snap();
                            let pat = self.sel_pat;
                            self.p.playlist.push(Clip { track: t, bar: b, pat });
                            self.msg = format!("Clip «{}» en el compás {}.", self.cur_pat().name, b + 1);
                            self.send();
                        }
                    }
                }
            }
        }
        if resp.secondary_clicked() {
            if let Some(pos) = resp.interact_pointer_pos() {
                if let Some((k, i, _)) = hit(self, pos) {
                    let id = match k {
                        Kind::Pat => self.p.playlist[i].pat,
                        Kind::Audio => self.p.audio_clips[i].id,
                        Kind::Auto => self.p.autos[i].id,
                    };
                    let step = (((pos.x - org.x) / spx).max(0.0)) as u32;
                    self.pl_menu = Some(PlMenu { kind: k, id, idx: i, pos, step, opened: 0 });
                } else if pos.y < org.y {
                    self.snap();
                    self.p.loop_on = !self.p.loop_on;
                    self.send();
                }
            }
        }
    }

    pub(super) fn auto_label(&self, t: AutoTarget) -> String {
        match t {
            AutoTarget::Param(ch, uid, pid) => {
                let slot = if ch == 0 { self.p.master_inserts.iter().find(|s| s.uid == uid) } else { self.p.channel(ch).and_then(|c| c.inserts.iter().chain(c.instrument.iter()).find(|s| s.uid == uid)) };
                let pname = self.plugin_param_name(uid, pid);
                format!("{} · {}", slot.map(|s| s.name.clone()).unwrap_or_else(|| "Plugin".into()), pname)
            }
            _ => t.label().to_string(),
        }
    }

    /// Menú contextual de un clip de la playlist.
    pub(super) fn playlist_menu(&mut self, ctx: &egui::Context) {
        let Some(m) = &mut self.pl_menu else { return };
        m.opened += 1;
        let (kind, id, idx, pos, step, opened) = (m.kind, m.id, m.idx, m.pos, m.step, m.opened);
        let mut close = false;
        let area = egui::Area::new(egui::Id::new("plmenu")).fixed_pos(pos).order(egui::Order::Foreground).show(ctx, |ui| desplazable(ui, |ui| {
            Frame::menu(ui.style()).show(ui, |ui| {
                ui.set_min_width(190.0);
                match kind {
                    Kind::Pat => {
                        if ui.button("Editar patrón").clicked() {
                            self.select_pat(id);
                            close = true;
                        }
                        if ui.button("Duplicar a la derecha").clicked() {
                            if let Some(c) = self.p.playlist.get(idx).copied() {
                                let bars = self.p.pattern(c.pat).map(|p| p.bars).unwrap_or(1);
                                self.snap();
                                self.p.playlist.push(Clip { bar: c.bar + bars, ..c });
                                self.send();
                            }
                            close = true;
                        }
                        if ui.button("Borrar clip").clicked() {
                            if idx < self.p.playlist.len() {
                                self.snap();
                                self.p.playlist.remove(idx);
                                self.send();
                            }
                            close = true;
                        }
                    }
                    Kind::Audio => {
                        let Some(c) = self.p.audio_clip(id).cloned() else {
                            close = true;
                            return;
                        };
                        ui.label(RichText::new(c.file()).small().color(DIM));
                        if c.takes.len() > 1 {
                            ui.label(RichText::new("TOMAS (comping)").small().color(DIM));
                            for (i, t) in c.takes.iter().enumerate() {
                                if ui.add(egui::Button::selectable(i == c.active, format!("Toma {} — {}", i + 1, t.chars().take(26).collect::<String>()))).clicked() {
                                    self.edit_clip(id, |x| x.active = i);
                                    close = true;
                                }
                            }
                            ui.separator();
                        }
                        ui.label(RichText::new("AFINAR LA VOZ (NN TUNE)").small().color(DIM));
                        ui.horizontal(|ui| {
                            if ui.button("Natural").on_hover_text("Corrige las notas desafinadas sin que se note").clicked() {
                                let tp = TuneP { estilo: 1, speed: 0.6, ..self.tune_params() };
                                self.afinar_clip(id, tp);
                                close = true;
                            }
                            if ui.button("Efecto robot").on_hover_text("Corrección instantánea: el sonido típico del autotune").clicked() {
                                let tp = TuneP { estilo: 2, ..self.tune_params() };
                                self.afinar_clip(id, tp);
                                close = true;
                            }
                            if ui.button("Como en el panel").on_hover_text("Con el estilo, la escala y la intensidad del panel Micrófono").clicked() {
                                let tp = self.tune_params();
                                self.afinar_clip(id, tp);
                                close = true;
                            }
                        });
                        if ui.button("Retune: editar nota por nota…").on_hover_text("Mueve cada nota de la voz a mano, como en un editor de afinación").clicked() {
                            self.abrir_retune(id);
                            close = true;
                        }
                        ui.label(RichText::new("CREAR ARMONÍA (DUPLICA LA TOMA EN OTRA PISTA)").small().color(DIM));
                        ui.horizontal_wrapped(|ui| {
                            for (n, k, semis, ayuda) in [
                                ("Tercera arriba", 2, None, "La armonía más común: suena dulce y llena"),
                                ("Tercera abajo", -2, None, "Una voz más grave que acompaña"),
                                ("Quinta arriba", 4, None, "Armonía abierta, muy usada en coros"),
                                ("Octava arriba", 0, Some(12), "Tu voz una octava más aguda, para doblar"),
                                ("Octava abajo", 0, Some(-12), "Tu voz una octava más grave, para dar cuerpo"),
                            ] {
                                if ui.button(n).on_hover_text(format!("{ayuda} · en la escala de {}", self.tune_key_label())).clicked() {
                                    self.crear_armonia(id, n, k, semis);
                                    close = true;
                                }
                            }
                        });
                        ui.label(RichText::new(format!("Tonalidad: {}", self.tune_key_label())).small().color(DIM));
                        ui.separator();
                        if step > c.start && step < c.start + c.len && ui.button(format!("Cortar aquí (compás {}.{})", step / 16 + 1, (step % 16) / 4 + 1)).clicked() {
                            self.split_clip(id, step);
                            close = true;
                        }
                        let fit = c.bpm > 0.0;
                        if ui.add(egui::Button::selectable(fit, "Ajustar al tempo del proyecto")).clicked() {
                            let secs = self.wave_peaks.get(c.file()).map(|w| w.1).unwrap_or(4.0);
                            let guess = media::guess_bpm(c.file(), secs);
                            let bpm = self.p.bpm;
                            self.edit_clip(id, |x| {
                                if fit {
                                    x.bpm = 0.0;
                                } else {
                                    x.bpm = if guess > 0.0 { guess } else { bpm };
                                    let nl = ((secs * x.bpm / bpm) as f64 / (60.0 / bpm as f64 / 4.0)).round() as u32;
                                    x.len = nl.max(1);
                                }
                            });
                            close = true;
                        }
                        ui.horizontal(|ui| {
                            ui.label("Tono");
                            if ui.button("-1").clicked() {
                                self.edit_clip(id, |x| x.pitch -= 1.0);
                            }
                            if ui.button("+1").clicked() {
                                self.edit_clip(id, |x| x.pitch += 1.0);
                            }
                            if ui.button("0").clicked() {
                                self.edit_clip(id, |x| x.pitch = 0.0);
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label("Ganancia");
                            if ui.button("-3 dB").clicked() {
                                self.edit_clip(id, |x| x.gain *= 0.708);
                            }
                            if ui.button("+3 dB").clicked() {
                                self.edit_clip(id, |x| x.gain = (x.gain * 1.413).min(4.0));
                            }
                        });
                        if ui.button("Normalizar (subir al máximo sin saturar)").clicked() {
                            self.normalizar_clip(id);
                            close = true;
                        }
                        if ui.button("Recortar silencio del principio y del final").clicked() {
                            self.recortar_silencio(id);
                            close = true;
                        }
                        if ui.add(egui::Button::selectable(c.reverse, "Invertir (al revés)")).clicked() {
                            self.edit_clip(id, |x| x.reverse = !x.reverse);
                            close = true;
                        }
                        if ui.add(egui::Button::selectable(c.fade_in > 0.0 || c.fade_out > 0.0, "Fundidos de entrada y salida")).clicked() {
                            let on = c.fade_in > 0.0 || c.fade_out > 0.0;
                            self.edit_clip(id, |x| {
                                x.fade_in = if on { 0.0 } else { 0.08 };
                                x.fade_out = if on { 0.0 } else { 0.25 };
                            });
                            close = true;
                        }
                        if ui.button("Propiedades…").clicked() {
                            self.open_win(Win::Clip(id));
                            close = true;
                        }
                        if ui.button("Abrir en NN Sampler").clicked() {
                            if let Some(p) = media::resolve(&self.plat.data_dir, c.file()) {
                                self.import_to_rack(&p);
                            }
                            close = true;
                        }
                        if ui.button("Duplicar a la derecha").clicked() {
                            self.snap();
                            let nid = self.p.new_id();
                            self.p.audio_clips.push(AudioClip { id: nid, start: c.start + c.len, ..c.clone() });
                            self.send();
                            close = true;
                        }
                        if ui.button("Borrar clip").clicked() {
                            self.snap();
                            self.p.audio_clips.retain(|x| x.id != id);
                            self.send();
                            close = true;
                        }
                    }
                    Kind::Auto => {
                        if ui.button("Editar curva…").clicked() {
                            self.open_win(Win::Auto(id));
                            close = true;
                        }
                        if ui.button("Duplicar a la derecha").clicked() {
                            if let Some(c) = self.p.auto_clip(id).cloned() {
                                self.snap();
                                let nid = self.p.new_id();
                                self.p.autos.push(AutoClip { id: nid, start: c.start + c.len, ..c });
                                self.send();
                            }
                            close = true;
                        }
                        if ui.button("Borrar automatización").clicked() {
                            self.snap();
                            self.p.autos.retain(|x| x.id != id);
                            self.send();
                            close = true;
                        }
                    }
                }
            });
        }));
        let outside = ctx.input(|i| i.pointer.any_pressed()) && ctx.input(|i| i.pointer.interact_pos()).is_some_and(|p| !area.response.rect.contains(p));
        if close || (outside && opened > 1) || ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.pl_menu = None;
        }
    }

    /// Afina la toma activa de un clip con NN Tune (en segundo plano) y la agrega como toma nueva.
    /// Duplica la toma de un clip en otra pista, llevada a una armonía dentro de la escala.
    pub(super) fn crear_armonia(&mut self, id: u32, que: &str, k: i32, semis: Option<i32>) {
        let Some(c) = self.p.audio_clip(id).cloned() else { return };
        let file = c.file().to_string();
        let Some(path) = media::resolve(&self.plat.data_dir, &file) else {
            self.msg = "No se encuentra el audio del clip.".into();
            return;
        };
        let mask = self.tune_params().mask;
        let dir = self.plat.data_dir.join("audio");
        let tx = self.file_tx.clone();
        let ctx = self.ctx.clone();
        let que = que.to_string();
        self.msg = format!("Creando la armonía ({que})…");
        std::thread::spawn(move || {
            let r = (|| -> Result<(String, Buf), String> {
                let b = media::decode(&path)?;
                let mono = b.mono();
                let data = crate::fx::armonia(&mono, b.sr, mask, k, semis);
                let _ = std::fs::create_dir_all(&dir);
                let stem = std::path::Path::new(&file).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "voz".into());
                let base = format!("{stem} (armonía {})", que.to_lowercase());
                let mut name = format!("{base}.wav");
                let mut n = 2;
                while dir.join(&name).exists() {
                    name = format!("{base} {n}.wav");
                    n += 1;
                }
                let out = Buf { ch: 1, sr: b.sr, data };
                media::write_wav(&dir.join(&name), &out).map_err(|e| e.to_string())?;
                Ok((name, out))
            })();
            let _ = tx.send(FileMsg::Armonia(id, que, r));
            if let Some(c) = ctx {
                c.request_repaint();
            }
        });
    }

    pub(super) fn afinar_clip(&mut self, id: u32, tp: TuneP) {
        let Some(c) = self.p.audio_clip(id).cloned() else { return };
        let file = c.file().to_string();
        let Some(path) = media::resolve(&self.plat.data_dir, &file) else {
            self.msg = "No se encuentra el audio del clip.".into();
            return;
        };
        let speed = crate::fx::velocidad(&tp);
        let seguir = self.settings.tune_acordes.then(|| (self.p.clone(), c.start, c.off, self.p.step_secs() as f32));
        let dir = self.plat.data_dir.join("audio");
        let tx = self.file_tx.clone();
        let ctx = self.ctx.clone();
        self.msg = "Afinando la voz con NN Tune…".into();
        std::thread::spawn(move || {
            let r = (|| -> Result<(String, Buf), String> {
                let b = media::decode(&path)?;
                let mono = b.mono();
                let res = match &seguir {
                    Some((proj, start, off, ss)) => {
                        let sr = b.sr;
                        let mascara = |i: usize| -> u16 {
                            let t = (i as f32 / sr - off).max(0.0);
                            let paso = *start as u64 + (t / ss) as u64;
                            let a = proj.acorde_en(None, paso);
                            if a.count_ones() >= 3 { a } else { tp.mask }
                        };
                        crate::fx::TuneResult { out: crate::fx::autotune_con(&mono, sr, &tp, &mascara), detected: vec![], corrected: vec![], hop: 256 }
                    }
                    None => crate::fx::autotune(&mono, b.sr, &tp),
                };
                let _ = std::fs::create_dir_all(&dir);
                let stem = std::path::Path::new(&file).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "voz".into());
                let tipo = if speed < 0.05 { "robot" } else { "afinada" };
                let mut name = format!("{stem} ({tipo}).wav");
                let mut k = 2;
                while dir.join(&name).exists() {
                    name = format!("{stem} ({tipo} {k}).wav");
                    k += 1;
                }
                let out = Buf { ch: 1, sr: b.sr, data: res.out };
                media::write_wav(&dir.join(&name), &out).map_err(|e| e.to_string())?;
                Ok((name, out))
            })();
            let _ = tx.send(FileMsg::Afinado(id, r));
            if let Some(c) = ctx {
                c.request_repaint();
            }
        });
    }

    pub(super) fn edit_clip(&mut self, id: u32, f: impl FnOnce(&mut AudioClip)) {
        self.snap();
        if let Some(c) = self.p.audio_clips.iter_mut().find(|c| c.id == id) {
            f(c);
            c.active = c.active.min(c.takes.len().saturating_sub(1));
            c.len = c.len.max(1);
        }
        self.send();
    }

    /// Sube la ganancia del clip para que su pico quede justo debajo del máximo.
    pub(super) fn normalizar_clip(&mut self, id: u32) {
        let Some(c) = self.p.audio_clip(id).cloned() else { return };
        let Some(path) = media::resolve(&self.plat.data_dir, c.file()) else {
            self.msg = "No se encuentra el audio del clip.".into();
            return;
        };
        let b = match media::decode(&path) {
            Ok(b) => b,
            Err(e) => {
                self.msg = format!("No se pudo leer el audio: {e}");
                return;
            }
        };
        // solo el tramo que suena en la playlist
        let ratio = c.ratio(self.p.bpm);
        let ini = (c.off * b.sr as f32) as usize;
        let fin = (ini + (c.len as f32 * self.p.step_secs() as f32 / ratio * b.sr as f32) as usize).min(b.frames());
        let mut pico = 0f32;
        for i in ini.min(fin)..fin {
            let (l, r) = b.at(i);
            pico = pico.max(l.abs()).max(r.abs());
        }
        if pico < 1e-5 {
            self.msg = "El clip está en silencio.".into();
            return;
        }
        let g = (0.97 / pico).min(16.0);
        self.edit_clip(id, |x| x.gain = g);
        self.msg = format!("Clip normalizado ({:+.1} dB).", 20.0 * g.log10());
    }

    /// Quita el silencio del principio y del final del clip (umbral −45 dB).
    pub(super) fn recortar_silencio(&mut self, id: u32) {
        let Some(c) = self.p.audio_clip(id).cloned() else { return };
        let Some(path) = media::resolve(&self.plat.data_dir, c.file()) else {
            self.msg = "No se encuentra el audio del clip.".into();
            return;
        };
        let Ok(b) = media::decode(&path) else {
            self.msg = "No se pudo leer el audio.".into();
            return;
        };
        let ss = self.p.step_secs() as f32;
        let ratio = c.ratio(self.p.bpm);
        let sr = b.sr as f32;
        let ini = (c.off * sr) as usize;
        let fin = (ini + (c.len as f32 * ss / ratio * sr) as usize).min(b.frames());
        let umbral = 10f32.powf(-45.0 / 20.0);
        let suena = |i: usize| {
            let (l, r) = b.at(i);
            l.abs().max(r.abs()) > umbral
        };
        let Some(primero) = (ini..fin).find(|&i| suena(i)) else {
            self.msg = "El clip está en silencio.".into();
            return;
        };
        let ultimo = (ini..fin).rev().find(|&i| suena(i)).unwrap_or(fin);
        // pasos que se pueden quitar a cada lado (dejando un poquito de aire)
        let aire = (0.03 * sr) as usize;
        let quitar_ini = (primero.saturating_sub(aire).saturating_sub(ini)) as f32 / sr * ratio / ss;
        let quitar_fin = (fin.saturating_sub(ultimo + aire)) as f32 / sr * ratio / ss;
        let (qi, qf) = (quitar_ini.floor() as u32, quitar_fin.floor() as u32);
        if qi + qf == 0 || qi + qf >= c.len {
            self.msg = "No hay silencio que recortar.".into();
            return;
        }
        self.snap();
        if let Some(x) = self.p.audio_clips.iter_mut().find(|x| x.id == id) {
            x.start += qi;
            x.off += qi as f32 * ss / ratio;
            x.len -= qi + qf;
        }
        self.cambios = true;
        self.send();
        self.msg = "Silencio recortado.".into();
    }

    fn split_clip(&mut self, id: u32, step: u32) {
        let Some(c) = self.p.audio_clip(id).cloned() else { return };
        if step <= c.start || step >= c.start + c.len {
            return;
        }
        self.snap();
        let ss = self.p.step_secs() as f32;
        let ratio = c.ratio(self.p.bpm);
        let nid = self.p.new_id();
        let d = step - c.start;
        let right = AudioClip { id: nid, start: step, len: c.len - d, off: c.off + d as f32 * ss / ratio, fade_in: 0.0, ..c.clone() };
        if let Some(x) = self.p.audio_clips.iter_mut().find(|x| x.id == id) {
            x.len = d;
            x.fade_out = 0.0;
        }
        self.p.audio_clips.push(right);
        self.msg = "Clip cortado: cada parte puede usar una toma distinta (comping).".into();
        self.send();
    }

    // ============================ automatización ============================

    /// Crea un clip de automatización para `t` y abre su editor.
    pub(super) fn add_auto(&mut self, t: AutoTarget) {
        self.snap();
        let cur = match t {
            AutoTarget::Master => t.norm(self.p.master),
            AutoTarget::Param(..) => 0.5,
            _ => {
                let c = t.channel().and_then(|id| self.p.channel(id));
                c.map(|c| match t {
                    AutoTarget::Vol(_) => t.norm(c.vol),
                    AutoTarget::Pan(_) => t.norm(c.pan),
                    AutoTarget::Cut(_) => t.norm(c.cut),
                    AutoTarget::Rev(_) => t.norm(c.rev),
                    AutoTarget::Dly(_) => t.norm(c.dly),
                    AutoTarget::Space(_) => t.norm(c.space),
                    _ => 0.5,
                })
                .unwrap_or(0.5)
            }
        };
        let len = (self.p.song_bars().max(4) * STEPS_PER_BAR).min(64 * 16);
        let track = self.free_track(0, len);
        self.ensure_tracks(track + 1);
        let id = self.p.new_id();
        self.p.autos.push(AutoClip { id, track, start: 0, len, target: t, pts: vec![[0.0, cur], [1.0, cur]] });
        self.open_win(Win::Auto(id));
        self.msg = "Automatización creada: dibuja la curva (clic: punto · arrastrar: mover · clic derecho: borrar). Suena en modo SONG.".into();
        self.send();
    }

    pub(super) fn auto_menu(&mut self, ui: &mut Ui, ch: u32) {
        for t in [AutoTarget::Vol(ch), AutoTarget::Pan(ch), AutoTarget::Cut(ch), AutoTarget::Rev(ch), AutoTarget::Dly(ch), AutoTarget::Space(ch)] {
            if ui.button(t.label()).clicked() {
                ui.close();
                self.add_auto(t);
            }
        }
        let slots: Vec<PluginSlot> = self.p.channel(ch).map(|c| c.inserts.iter().chain(c.instrument.iter()).cloned().collect()).unwrap_or_default();
        self.plugin_auto_entries(ui, ch, &slots);
    }

    pub(super) fn auto_window(&mut self, ui: &mut Ui, id: u32) {
        let Some(a) = self.p.auto_clip(id).cloned() else {
            ui.label("La automatización ya no existe.");
            return;
        };
        let target_name = a.target.channel().and_then(|c| self.p.channel(c)).map(|c| c.name.clone()).unwrap_or_else(|| "Master".into());
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{target_name} · {}", self.auto_label(a.target))).strong());
            ui.label(RichText::new(format!("compases {} – {}", a.start / 16 + 1, (a.start + a.len) as f32 / 16.0 + 1.0)).small().color(DIM));
        });
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(560.0, 200.0), Sense::click_and_drag());
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 4, Color32::from_gray(19));
        let bars = (a.len as f32 / 16.0).max(0.25);
        let mut b = 0.0;
        while b <= bars {
            let x = rect.left() + b / bars * rect.width();
            p.vline(x, rect.y_range(), Stroke::new(1.0, Color32::from_gray(33)));
            b += if bars > 16.0 { 4.0 } else { 1.0 };
        }
        for k in 1..4 {
            p.hline(rect.x_range(), rect.top() + rect.height() * k as f32 / 4.0, Stroke::new(1.0, Color32::from_gray(30)));
        }
        let to = |pt: [f32; 2]| Pos2::new(rect.left() + pt[0] * rect.width(), rect.bottom() - pt[1] * rect.height());
        let mut line = vec![to([0.0, a.at(0.0)])];
        line.extend(a.pts.iter().map(|q| to(*q)));
        line.push(to([1.0, a.at(1.0)]));
        let fill: Vec<Pos2> = line.clone();
        p.add(egui::Shape::line(fill, Stroke::new(2.0, Color32::from_rgb(190, 170, 250))));
        for q in &a.pts {
            p.circle(to(*q), 5.0, Color32::from_rgb(230, 220, 255), Stroke::new(1.0, Color32::BLACK));
        }
        if self.song_mode && self.playing() {
            let pos = self.status.pos() as f32;
            if pos >= a.start as f32 && pos < (a.start + a.len) as f32 {
                let x = rect.left() + (pos - a.start as f32) / a.len as f32 * rect.width();
                p.vline(x, rect.y_range(), Stroke::new(2.0, ACCENT2));
            }
        }
        let from = |q: Pos2| [((q.x - rect.left()) / rect.width()).clamp(0.0, 1.0), ((rect.bottom() - q.y) / rect.height()).clamp(0.0, 1.0)];
        let near = |q: Pos2| a.pts.iter().position(|pt| to(*pt).distance(q) < 9.0);
        if let Some(h) = resp.hover_pos() {
            let v = from(h)[1];
            p.text(h + Vec2::new(10.0, -12.0), Align2::LEFT_BOTTOM, a.target.fmt(v), FontId::proportional(11.0), Color32::WHITE);
        }
        let mut pts = a.pts.clone();
        let mut changed = false;
        if resp.drag_started() {
            if let Some(q) = ui.input(|i| i.pointer.press_origin()) {
                self.snap();
                self.auto_drag = Some(match near(q) {
                    Some(i) => i,
                    None => {
                        pts.push(from(q));
                        changed = true;
                        pts.len() - 1
                    }
                });
            }
        }
        if let (Some(i), Some(q)) = (self.auto_drag, resp.interact_pointer_pos()) {
            if resp.dragged() && i < pts.len() {
                pts[i] = from(q);
                changed = true;
            }
        }
        if resp.drag_stopped() {
            self.auto_drag = None;
            self.send();
        }
        if resp.clicked() {
            if let Some(q) = resp.interact_pointer_pos() {
                if near(q).is_none() {
                    self.snap();
                    pts.push(from(q));
                    changed = true;
                }
            }
        }
        if resp.secondary_clicked() {
            if let Some(i) = resp.interact_pointer_pos().and_then(near) {
                if pts.len() > 1 {
                    self.snap();
                    pts.remove(i);
                    changed = true;
                }
            }
        }
        if changed {
            // mantener el orden por x (el punto arrastrado sigue siendo el mismo)
            let dragged = self.auto_drag.and_then(|i| pts.get(i).copied());
            pts.sort_by(|x, y| x[0].total_cmp(&y[0]));
            if let Some(d) = dragged {
                self.auto_drag = pts.iter().position(|q| *q == d);
            }
            if let Some(c) = self.p.autos.iter_mut().find(|c| c.id == id) {
                c.pts = pts;
            }
            self.cmd(Cmd::Project(Box::new(self.p.clone())));
            if !resp.dragged() {
                self.send();
            }
        }
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("FORMAS").small().color(DIM));
            let shapes: [(&str, Vec<[f32; 2]>); 6] = [
                ("Plana", vec![[0.0, 0.5], [1.0, 0.5]]),
                ("Sube", vec![[0.0, 0.0], [1.0, 1.0]]),
                ("Baja", vec![[0.0, 1.0], [1.0, 0.0]]),
                ("Sube y baja", vec![[0.0, 0.0], [0.5, 1.0], [1.0, 0.0]]),
                ("Barrido (filtro)", vec![[0.0, 0.2], [0.85, 0.75], [1.0, 1.0]]),
                ("LFO 4 ciclos", (0..=16).map(|i| [i as f32 / 16.0, if i % 4 == 0 { 0.2 } else if i % 4 == 2 { 0.8 } else { 0.5 }]).collect()),
            ];
            for (n, v) in shapes {
                if ui.small_button(n).clicked() {
                    self.snap();
                    if let Some(c) = self.p.autos.iter_mut().find(|c| c.id == id) {
                        c.pts = v;
                    }
                    self.send();
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label(RichText::new("INICIO").small().color(DIM));
            let mut s = a.start as f32 / 16.0 + 1.0;
            let r = ui.add(egui::DragValue::new(&mut s).range(1.0..=999.0).speed(0.05).max_decimals(2).prefix("compás "));
            self.track(&r);
            if r.changed() {
                if let Some(c) = self.p.autos.iter_mut().find(|c| c.id == id) {
                    c.start = (((s - 1.0) * 16.0).round().max(0.0)) as u32;
                }
                self.send();
            }
            ui.label(RichText::new("LARGO").small().color(DIM));
            let mut l = a.len as f32 / 16.0;
            let r = ui.add(egui::DragValue::new(&mut l).range(0.25..=256.0).speed(0.05).max_decimals(2).suffix(" compases"));
            self.track(&r);
            if r.changed() {
                if let Some(c) = self.p.autos.iter_mut().find(|c| c.id == id) {
                    c.len = ((l * 16.0).round() as u32).max(1);
                }
                self.send();
            }
            if ui.button("Borrar").clicked() {
                self.snap();
                self.p.autos.retain(|c| c.id != id);
                self.send();
            }
        });
    }

    // ============================ NN Sampler ============================

    pub(super) fn sampler_window(&mut self, ui: &mut Ui, id: u32) {
        let Some(c) = self.p.channel(id).cloned() else { return };
        let mut sp = c.smp.clone();
        let mut started = false;
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label(RichText::new(if sp.file.is_empty() { "Sin sample".to_string() } else { sp.file.clone() }).strong());
            if ui.button("Cargar…").clicked() {
                if let Some(path) = self.pick_audio() {
                    match media::import_file(&self.plat.data_dir, &path) {
                        Ok(n) => {
                            if self.load_now(&n).is_some() {
                                started = true;
                                changed = true;
                                sp.file = n;
                                sp.start = 0.0;
                                sp.end = 1.0;
                                sp.slices.clear();
                            }
                        }
                        Err(e) => self.msg = e,
                    }
                } else if self.plat.mobile {
                    self.left_tab = 1;
                    self.msg = "Elige el archivo en el navegador y arrástralo al rack.".into();
                }
            }
            if ui.button("Probar").clicked() {
                self.preview(id, sp.root, 0.9);
            }
        });
        let buf = self.files.get(&sp.file).cloned();
        let secs = buf.as_ref().map(|b| b.secs()).unwrap_or(0.0);
        // forma de onda con inicio/fin y cortes
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(560.0, 130.0), Sense::click_and_drag());
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 4, Color32::from_gray(17));
        if let Some((pk, _)) = self.wave_peaks.get(&sp.file) {
            let n = pk.len();
            for x in 0..rect.width() as usize {
                let i = (x as f32 / rect.width() * n as f32) as usize;
                let v = pk[i.min(n - 1)] * sp.gain.min(2.0);
                let cx = rect.left() + x as f32;
                let inside = (x as f32 / rect.width()) >= sp.start && (x as f32 / rect.width()) <= sp.end;
                let col = if inside { Color32::from_rgb(242, 170, 90) } else { Color32::from_gray(58) };
                p.vline(cx, (rect.center().y - v * 60.0)..=(rect.center().y + v * 60.0), Stroke::new(1.0, col));
            }
        } else {
            p.text(rect.center(), Align2::CENTER_CENTER, if sp.file.is_empty() { "Carga un sample o arrástralo desde el navegador" } else { "cargando…" }, FontId::proportional(12.0), DIM);
        }
        let xs = |f: f32| rect.left() + f.clamp(0.0, 1.0) * rect.width();
        if sp.slice && secs > 0.0 {
            for (i, s) in sp.slices.iter().enumerate() {
                let x = xs(s / secs);
                p.vline(x, rect.y_range(), Stroke::new(1.5, Color32::from_gray(225)));
                p.text(Pos2::new(x + 3.0, rect.top() + 2.0), Align2::LEFT_TOP, format!("{}", i + 1), FontId::proportional(10.0), Color32::from_gray(225));
            }
        } else {
            for f in [sp.start, sp.end] {
                p.vline(xs(f), rect.y_range(), Stroke::new(2.0, Color32::WHITE));
            }
        }
        if secs > 0.0 {
            let f = |q: Pos2| ((q.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
            if sp.slice {
                if resp.clicked() {
                    if let Some(q) = resp.interact_pointer_pos() {
                        started = true;
                        changed = true;
                        sp.slices.push(f(q) * secs);
                        sp.slices.sort_by(|a, b| a.total_cmp(b));
                    }
                }
                if resp.secondary_clicked() {
                    if let Some(q) = resp.interact_pointer_pos() {
                        let t = f(q) * secs;
                        if let Some(i) = sp.slices.iter().position(|s| ((s - t) / secs * rect.width()).abs() < 6.0) {
                            started = true;
                            changed = true;
                            sp.slices.remove(i);
                        }
                    }
                }
            } else if let Some(q) = resp.interact_pointer_pos() {
                if resp.drag_started() {
                    started = true;
                }
                if resp.dragged() || resp.clicked() {
                    let v = f(q);
                    if (v - sp.start).abs() < (v - sp.end).abs() {
                        sp.start = v.min(sp.end - 0.001);
                    } else {
                        sp.end = v.max(sp.start + 0.001);
                    }
                    changed = true;
                }
            }
        }
        ui.label(RichText::new(if sp.slice { "Modo slicer: clic agrega un corte · clic derecho lo quita · cada corte suena en una nota desde la nota base" } else { "Arrastra para mover el inicio y el fin" }).small().color(DIM));
        egui::Grid::new(("smpk", id)).spacing(Vec2::new(4.0, 2.0)).show(ui, |ui| {
            macro_rules! k {
                ($v:expr, $a:expr, $b:expr, $d:expr, $l:expr, $f:expr) => {{
                    let r = knob(ui, $v, $a, $b, $d, $l, $f);
                    started |= r.drag_started() || r.double_clicked();
                    changed |= r.changed();
                }};
            }
            let mut root = sp.root as f32;
            k!(&mut root, 24.0, 96.0, 60.0, "Nota base", |v| nname(v.round() as u8));
            sp.root = root.round() as u8;
            k!(&mut sp.gain, 0.0, 2.0, 1.0, "Ganancia", |v| format!("{:+.1} dB", 20.0 * v.max(1e-4).log10()));
            k!(&mut sp.attack, 0.0, 1.0, 0.0, "Ataque", |v| format!("{:.0} ms", v * 1000.0));
            k!(&mut sp.release, 0.003, 2.0, 0.05, "Relajación", |v| format!("{:.0} ms", v * 1000.0));
            k!(&mut sp.pitch, -12.0, 12.0, 0.0, "Tono", |v| format!("{:+.1} st", v));
            k!(&mut sp.stretch, 0.5, 2.0, 1.0, "Estirar", |v| format!("×{:.2}", v));
            ui.end_row();
        });
        ui.horizontal_wrapped(|ui| {
            let mut tog = |ui: &mut Ui, v: &mut bool, label: &str, tip: &str| {
                if ui.add(egui::Button::selectable(*v, label)).on_hover_text(tip).clicked() {
                    *v = !*v;
                    started = true;
                    changed = true;
                }
            };
            tog(ui, &mut sp.oneshot, "One-shot", "Suena completo aunque la nota sea corta");
            tog(ui, &mut sp.reverse, "Al revés", "Reproduce el sample invertido");
            tog(ui, &mut sp.slice, "Slicer", "Cada nota toca un trozo del loop");
            let fit = sp.bpm > 0.0;
            if ui.add(egui::Button::selectable(fit, "Ajustar al tempo")).on_hover_text("Estira el loop al tempo del proyecto sin cambiar el tono").clicked() {
                started = true;
                changed = true;
                sp.bpm = if fit { 0.0 } else { media::guess_bpm(&sp.file, secs).max(1.0) };
            }
            if fit {
                ui.label(RichText::new("tempo original").small().color(DIM));
                let r = ui.add(egui::DragValue::new(&mut sp.bpm).range(40.0..=220.0).speed(0.2).suffix(" bpm"));
                started |= r.drag_started();
                changed |= r.changed();
            }
        });
        if sp.slice {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("CORTES").small().color(DIM));
                if let Some(b) = &buf {
                    if ui.button("Detectar golpes").clicked() {
                        started = true;
                        changed = true;
                        sp.slices = media::onsets(b, self.sampler_sens, 32);
                    }
                    ui.spacing_mut().slider_width = 70.0;
                    ui.add(egui::Slider::new(&mut self.sampler_sens, 0.0..=1.0).show_value(false)).on_hover_text("Sensibilidad");
                    for n in [4usize, 8, 16] {
                        if ui.button(format!("{n} iguales")).clicked() {
                            started = true;
                            changed = true;
                            sp.slices = media::even_slices(b, n);
                        }
                    }
                }
                if ui.button("Crear patrón con los cortes").on_hover_text("Escribe una nota por corte en el patrón actual, en su posición original").clicked() && secs > 0.0 {
                    self.slices_to_pattern(id, &sp, secs);
                }
            });
        }
        if started {
            self.snap();
        }
        if changed {
            if let Some(cm) = self.p.channel_mut(id) {
                cm.smp = sp;
            }
            self.send();
        }
    }

    fn slices_to_pattern(&mut self, id: u32, sp: &SampleP, secs: f32) {
        self.snap();
        let ratio = sp.ratio(self.p.bpm);
        let ss = self.p.step_secs() as f32;
        let bars = (((secs * ratio) / ss / 16.0).ceil() as u32).clamp(1, 8);
        let pid = self.sel_pat;
        let notes: Vec<Note> = sp
            .slices
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let st = t * ratio / ss;
                let end = sp.slices.get(i + 1).map(|x| x * ratio / ss).unwrap_or(secs * ratio / ss);
                Note { s: st.floor() as u32, l: ((end - st).round() as u32).max(1), m: (sp.root as usize + i).min(127) as u8, v: 0.85, o: st.fract() }
            })
            .collect();
        if let Some(p) = self.p.pattern_mut(pid) {
            if p.bars < bars {
                p.set_bars(bars);
            }
            let ls = p.len_steps();
            p.notes.insert(id, notes.into_iter().filter(|n| n.s < ls).collect());
        }
        self.sel_ch = id;
        self.view = View::Roll;
        self.roll_center = true;
        self.msg = "Patrón creado con los cortes: muévelos en el piano roll para rehacer el loop.".into();
        self.send();
    }

    // ============================ propiedades de un clip de audio ============================

    pub(super) fn clip_window(&mut self, ui: &mut Ui, id: u32) {
        let Some(c) = self.p.audio_clip(id).cloned() else {
            ui.label("El clip ya no existe.");
            return;
        };
        let mut x = c.clone();
        let mut started = false;
        egui::Grid::new(("clipk", id)).spacing(Vec2::new(4.0, 2.0)).show(ui, |ui| {
            macro_rules! k {
                ($v:expr, $a:expr, $b:expr, $d:expr, $l:expr, $f:expr) => {{
                    let r = knob(ui, $v, $a, $b, $d, $l, $f);
                    started |= r.drag_started() || r.double_clicked();
                }};
            }
            k!(&mut x.gain, 0.0, 4.0, 1.0, "Ganancia", |v| format!("{:+.1} dB", 20.0 * v.max(1e-4).log10()));
            k!(&mut x.pitch, -12.0, 12.0, 0.0, "Tono", |v| format!("{:+.1} st", v));
            k!(&mut x.fade_in, 0.0, 4.0, 0.0, "Fundido ent.", |v| format!("{:.2} s", v));
            k!(&mut x.fade_out, 0.0, 4.0, 0.0, "Fundido sal.", |v| format!("{:.2} s", v));
            ui.end_row();
        });
        ui.horizontal(|ui| {
            ui.label("Tempo original");
            let r = ui.add(egui::DragValue::new(&mut x.bpm).range(0.0..=220.0).speed(0.2).suffix(" bpm"));
            started |= r.drag_started();
            ui.label(RichText::new("(0 = sin ajustar)").small().color(DIM));
        });
        ui.horizontal(|ui| {
            ui.label("Desplazamiento");
            let r = ui.add(egui::DragValue::new(&mut x.off).range(0.0..=600.0).speed(0.01).suffix(" s"));
            started |= r.drag_started();
            if ui.add(egui::Button::selectable(x.reverse, "Al revés")).clicked() {
                x.reverse = !x.reverse;
                started = true;
            }
        });
        if x.takes.len() > 1 {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("TOMAS").small().color(DIM));
                for i in 0..x.takes.len() {
                    if ui.add(egui::Button::selectable(i == x.active, format!("{}", i + 1))).clicked() {
                        x.active = i;
                        started = true;
                    }
                }
                if ui.button("Borrar toma activa").clicked() {
                    x.takes.remove(x.active);
                    x.active = x.active.min(x.takes.len().saturating_sub(1));
                    started = true;
                }
            });
        }
        if started {
            self.snap();
        }
        if x != c {
            if let Some(cm) = self.p.audio_clips.iter_mut().find(|c| c.id == id) {
                *cm = x;
            }
            self.send();
        }
    }

    // ============================ grabación por tomas ============================

    /// Graba audio en la playlist: con bucle cada vuelta es una toma; con punch solo se queda la región.
    pub(super) fn rec_track_start(&mut self) {
        if self.rec_track.is_some() || self.rec.is_some() {
            return;
        }
        if self.audio.is_none() {
            self.msg = "No hay salida de audio.".into();
            return;
        }
        let ch = match self.p.channel(self.sel_ch) {
            Some(c) if c.inst == Inst::Audio => c.id,
            Some(c) if c.inst == Inst::Vocal => {
                let id = c.id;
                self.rec_start(id);
                return;
            }
            _ => {
                self.snap();
                let id = self.p.new_id();
                let mut c = Channel::new(id, Inst::Audio);
                c.name = "Grabación".into();
                c.vol = 0.85;
                self.p.channels.push(c);
                self.sel_ch = id;
                self.send();
                id
            }
        };
        let region = (self.p.loop_b > self.p.loop_a && (self.p.loop_on || self.p.punch)).then(|| (self.p.loop_a * 16, self.p.loop_b * 16));
        let looped = self.p.loop_on && region.is_some();
        let from = match region {
            Some((a, _)) if looped => a,
            Some((a, _)) => a.saturating_sub(16),
            None => self.play_from,
        };
        self.mic_mon = None;
        match audio::record(self.status.clone(), &self.settings.entrada, true, self.settings.canal_entrada as usize) {
            Ok(r) => {
                if self.settings.metro_grabar {
                    self.cmd(Cmd::Metronome(true));
                }
                self.song_mode = true;
                // precuenta: compases de metrónomo antes de que arranque (no en bucle)
                let precuenta = if looped { 0 } else { self.settings.precuenta.min(2) as u32 * STEPS_PER_BAR };
                if precuenta > 0 {
                    self.cmd(Cmd::Precuenta(precuenta));
                }
                self.play(from);
                let precuenta_s = precuenta as f32 * self.p.step_secs() as f32;
                if precuenta > 0 {
                    self.msg = format!("Precuenta de {} compás(es)…", precuenta / STEPS_PER_BAR);
                }
                // carril libre desde donde empieza hasta el final de la canción
                let fin = (self.p.song_bars() * STEPS_PER_BAR).max(from + 64);
                let track = self.p.audio_clips.iter().find(|c| c.ch == ch && c.start >= from).map(|c| c.track).unwrap_or_else(|| self.free_track(from, fin - from));
                self.ensure_tracks(track + 1);
                self.rec_track = Some(TrackRec { r, ch, from, region, looped, started: Instant::now(), track, precuenta_s });
                // después de guardar la grabación: así la escucha usa este micrófono y no abre otro
                self.escucha_sync();
                self.msg = if looped { "Grabando en bucle: cada vuelta es una toma. Espacio o Detener para terminar.".into() } else if region.is_some() { "Grabando con punch: solo se guarda la región marcada.".into() } else { "Grabando en la pista de audio… Espacio o Detener para terminar.".into() };
            }
            Err(e) => self.msg = format!("No se pudo grabar: {e}"),
        }
    }

    pub(super) fn rec_track_stop(&mut self) {
        let Some(tr) = self.rec_track.take() else { return };
        self.cmd(Cmd::Stop);
        self.cmd(Cmd::Metronome(self.metro));
        self.escucha_sync();
        self.status.playing.store(false, Ordering::Relaxed);
        let sr = tr.r.sr as f32;
        let data = tr.r.buf.lock().map(|b| b.clone()).unwrap_or_default();
        drop(tr.r);
        let ss = self.p.step_secs() as f32;
        let lat = ((self.settings.latency_ms / 1000.0) * sr) as usize;
        // se quita la latencia y el tramo de la precuenta
        let data: Vec<f32> = data.into_iter().skip(lat + (tr.precuenta_s * sr) as usize).collect();
        let total_steps = data.len() as f32 / sr / ss;
        let mut pieces: Vec<(u32, u32, Vec<f32>)> = vec![]; // inicio, largo (pasos), audio
        match tr.region {
            Some((a, b)) if tr.looped => {
                let per = ((b - a) as f32 * ss * sr) as usize;
                let mut k = 0;
                while k * per < data.len() {
                    let seg = &data[k * per..((k + 1) * per).min(data.len())];
                    if seg.len() * 2 >= per || (k == 0 && seg.len() > (sr * 0.3) as usize) {
                        pieces.push((a, b - a, seg.to_vec()));
                    }
                    k += 1;
                }
            }
            Some((a, b)) => {
                let s0 = ((a - tr.from) as f32 * ss * sr) as usize;
                let s1 = (((b - tr.from) as f32 * ss * sr) as usize).min(data.len());
                if s1 > s0 + (sr * 0.2) as usize {
                    let real = ((s1 - s0) as f32 / sr / ss).ceil() as u32;
                    pieces.push((a, real.min(b - a).max(1), data[s0..s1].to_vec()));
                }
            }
            None => {
                if data.len() > (sr * 0.25) as usize {
                    pieces.push((tr.from, (total_steps.ceil() as u32).max(1), data));
                }
            }
        }
        if pieces.is_empty() {
            self.msg = "La grabación quedó vacía o demasiado corta.".into();
            return;
        }
        let dir = self.plat.data_dir.join("audio");
        let _ = std::fs::create_dir_all(&dir);
        self.snap();
        let ts = now_secs();
        let n = pieces.len();
        let nivelar = self.settings.nivelar_grabacion;
        for (k, (start, len, audio_data)) in pieces.into_iter().enumerate() {
            let name = format!("grabacion_{}_{}_{}.wav", tr.ch, ts, k + 1);
            // la toma suena tan fuerte y pareja como te escuchaste
            let audio_data = if nivelar { crate::fx::nivelar(&audio_data, sr) } else { audio_data };
            let b = Buf { ch: 1, sr, data: audio_data };
            if let Err(e) = media::write_wav(&dir.join(&name), &b) {
                self.msg = format!("No se pudo guardar la toma: {e}");
                return;
            }
            let b = Arc::new(b);
            self.wave_peaks.insert(name.clone(), (Arc::new(b.peaks(4000)), b.secs()));
            self.files.insert(name.clone(), b.clone());
            self.cmd(Cmd::File { key: name.clone(), buf: Some(b) });
            match self.p.audio_clips.iter_mut().find(|c| c.ch == tr.ch && c.start == start && c.len == len) {
                Some(c) => {
                    c.takes.push(name);
                    c.active = c.takes.len() - 1;
                }
                None => {
                    let track = tr.track;
                    self.ensure_tracks(track + 1);
                    let id = self.p.new_id();
                    self.p.audio_clips.push(AudioClip { id, ch: tr.ch, track, start, len, off: 0.0, takes: vec![name], active: 0, gain: 1.0, pitch: 0.0, bpm: 0.0, fade_in: 0.0, fade_out: 0.0, reverse: false });
                }
            }
        }
        if self.settings.tune_vivo {
            let ids: Vec<u32> = self.p.audio_clips.iter().filter(|c| c.ch == tr.ch && c.takes.last().is_some_and(|t| t.starts_with(&format!("grabacion_{}_{}_", tr.ch, ts)))).map(|c| c.id).collect();
            let tp = self.tune_params();
            for id in ids {
                self.afinar_clip(id, tp);
            }
        }
        self.msg = if n > 1 { format!("{n} tomas grabadas. Clic derecho en el clip para elegir la toma; «Cortar aquí» para combinarlas (comping).") } else { "Toma grabada en la playlist.".into() };
        let _ = tr.started;
        self.send();
    }

    // ============================ MIDI ============================

    pub(super) fn midi_connect(&mut self) {
        #[cfg(not(target_os = "android"))]
        {
            let ports = midi::input::ports();
            let idx = ports.iter().position(|p| *p == self.settings.midi_port).unwrap_or(0);
            let ctx = self.ctx.clone();
            match midi::input::connect(idx, self.midi_tx.clone(), move || {
                if let Some(c) = &ctx {
                    c.request_repaint();
                }
            }) {
                Ok(c) => {
                    self.msg = format!("Teclado MIDI conectado: {}", c.name);
                    self.settings.midi_port = c.name.clone();
                    self.midi_conn = Some(c);
                }
                Err(e) => {
                    self.midi_conn = None;
                    self.msg = format!("MIDI: {e}");
                }
            }
        }
    }

    pub(super) fn midi_poll(&mut self) {
        while let Ok(ev) = self.midi_rx.try_recv() {
            let Some(c) = self.p.channel(self.sel_ch).cloned() else { continue };
            match ev {
                midi::Live::On(m, v) => {
                    if c.inst.is_drum() {
                        self.preview(c.id, 60, v);
                    } else {
                        self.cmd(Cmd::NoteOn { ch: c.id, m, v });
                    }
                    if self.midi_rec && self.playing() && !self.song_mode {
                        if let Some(pos) = self.local_step() {
                            self.midi_held.insert(m, (c.id, pos, v));
                        }
                    }
                }
                midi::Live::Off(m) => {
                    self.cmd(Cmd::NoteOff { ch: c.id, m });
                    if let Some((ch, start, v)) = self.midi_held.remove(&m) {
                        self.midi_record_note(ch, m, start, v);
                    }
                }
                midi::Live::Cc(n, v) => {
                    // rueda de modulación → filtro del canal seleccionado
                    if n == 1 {
                        let id = c.id;
                        if let Some(cm) = self.p.channel_mut(id) {
                            cm.cut = AutoTarget::Cut(id).value(v);
                        }
                        self.send();
                    }
                }
            }
        }
    }

    fn midi_record_note(&mut self, ch: u32, m: u8, start: f64, v: f32) {
        let Some(c) = self.p.channel(ch).cloned() else { return };
        let len_steps = self.cur_pat().len_steps() as f64;
        let end = self.local_step().unwrap_or(start);
        let mut dur = end - start;
        if dur < 0.0 {
            dur += len_steps;
        }
        let q = self.settings.midi_quant;
        let pid = self.sel_pat;
        if self.hist.last().is_none_or(|h| h.pattern(pid).and_then(|p| p.notes.get(&ch)) == self.p.pattern(pid).and_then(|p| p.notes.get(&ch))) {
            self.snap();
        }
        if let Some(p) = self.p.pattern_mut(pid) {
            let n = p.len_steps();
            if c.inst.uses_steps() && !c.inst.melodic() || c.inst.is_drum() {
                let s = (start.round() as u32) % n;
                let a = p.steps.entry(ch).or_insert_with(|| vec![0.0; n as usize]);
                a.resize(n as usize, 0.0);
                a[s as usize] = v.max(0.1);
            } else {
                let (s, o) = if q { ((start.round() as u32) % n, 0.0) } else { (start.floor() as u32 % n, start.fract() as f32) };
                let l = (dur.round().max(1.0) as u32).min(n - s);
                p.notes.entry(ch).or_default().push(Note { s, l, m, v, o });
            }
        }
        self.cmd(Cmd::Project(Box::new(self.p.clone())));
        self.dirty = Some(Instant::now());
        self.files_dirty = true;
    }

    pub(super) fn import_midi_path(&mut self, path: &Path) {
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => {
                self.msg = format!("No se pudo leer el MIDI: {e}");
                return;
            }
        };
        match midi::read(&bytes) {
            Ok(mf) => {
                self.snap();
                let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "MIDI".into());
                match midi::import(&mut self.p, &mf, &name, Inst::Rhodes) {
                    Ok(pid) => {
                        self.select_pat(pid);
                        if let Some(id) = self.p.channels.last().map(|c| c.id) {
                            self.sel_ch = id;
                        }
                        self.msg = format!("MIDI importado como patrón «{name}»{}.", mf.bpm.map(|b| format!(" (el archivo está a {b:.0} bpm)")).unwrap_or_default());
                        self.send();
                    }
                    Err(e) => {
                        self.hist.pop();
                        self.msg = e;
                    }
                }
            }
            Err(e) => self.msg = e,
        }
    }

    pub(super) fn import_midi_dialog(&mut self) {
        #[cfg(not(target_os = "android"))]
        if let Some(p) = rfd::FileDialog::new().set_title("Importar MIDI").add_filter("MIDI", &["mid", "midi"]).set_directory(&self.plat.export_dir).pick_file() {
            self.import_midi_path(&p);
        }
        #[cfg(target_os = "android")]
        {
            self.left_tab = 1;
            self.msg = "Busca el .mid en el navegador y tócalo dos veces.".into();
        }
    }

    pub(super) fn export_midi(&mut self) {
        let bytes = midi::export_pattern(&self.p, self.cur_pat());
        let name = format!("{} - {}.mid", export::safe_name(&self.p.name), export::safe_name(&self.cur_pat().name));
        #[cfg(not(target_os = "android"))]
        let path = rfd::FileDialog::new().set_title("Exportar MIDI").add_filter("MIDI", &["mid"]).set_directory(&self.plat.export_dir).set_file_name(&name).save_file();
        #[cfg(target_os = "android")]
        let path = Some(self.plat.export_dir.join(&name));
        if let Some(p) = path {
            self.msg = match std::fs::write(&p, bytes) {
                Ok(_) => format!("MIDI exportado: {}", p.display()),
                Err(e) => format!("No se pudo escribir el MIDI: {e}"),
            };
        }
    }

    // ============================ herramientas del piano roll ============================

    pub(super) fn roll_tools(&mut self, ui: &mut Ui, ch: u32) {
        let pid = self.sel_pat;
        let mut edit = |this: &mut Self, f: &dyn Fn(&mut Vec<Note>, u32, &Project)| {
            this.snap();
            let proj = this.p.clone();
            if let Some(p) = this.p.pattern_mut(pid) {
                let n = p.len_steps();
                let ns = p.notes.entry(ch).or_default();
                f(ns, n, &proj);
                ns.retain(|x| x.s < n);
                for x in ns.iter_mut() {
                    x.l = x.l.clamp(1, n - x.s);
                }
            }
            this.send();
        };
        ui.menu_button("Herramientas", |ui| desplazable(ui, |ui| {
            ui.label(RichText::new("TRANSPONER").small().color(DIM));
            ui.horizontal(|ui| {
                for (n, d) in [("-12", -12i32), ("-1", -1), ("+1", 1), ("+12", 12)] {
                    if ui.button(n).clicked() {
                        edit(self, &|ns, _, _| {
                            for x in ns.iter_mut() {
                                x.m = (x.m as i32 + d).clamp(0, 127) as u8;
                            }
                        });
                    }
                }
            });
            if ui.button("Ajustar notas a la escala").clicked() {
                edit(self, &|ns, _, p| {
                    for x in ns.iter_mut() {
                        x.m = p.snap_scale(x.m as i32).clamp(0, 127) as u8;
                    }
                });
            }
            ui.separator();
            if ui.button("Cuantizar (quitar desplazamientos)").clicked() {
                edit(self, &|ns, n, _| {
                    for x in ns.iter_mut() {
                        if x.o >= 0.5 {
                            x.s = (x.s + 1).min(n - 1);
                        }
                        x.o = 0.0;
                    }
                });
            }
            if ui.button("Humanizar (tiempo y velocidad)").clicked() {
                let seed = now_secs() as u32;
                edit(self, &|ns, _, _| {
                    for (i, x) in ns.iter_mut().enumerate() {
                        let r1 = (crate::dsp::hash(seed, i as u32 * 3) >> 8) as f32 / (1u32 << 24) as f32;
                        let r2 = (crate::dsp::hash(seed, i as u32 * 3 + 1) >> 8) as f32 / (1u32 << 24) as f32;
                        x.o = (r1 * 0.14).min(0.99);
                        x.v = (x.v * (0.88 + r2 * 0.24)).clamp(0.05, 1.0);
                    }
                });
            }
            if ui.button("Legato (unir notas)").clicked() {
                edit(self, &|ns, n, _| {
                    let mut starts: Vec<u32> = ns.iter().map(|x| x.s).collect();
                    starts.sort();
                    starts.dedup();
                    for x in ns.iter_mut() {
                        let next = starts.iter().copied().find(|s| *s > x.s).unwrap_or(n);
                        x.l = next - x.s;
                    }
                });
            }
            if ui.button("Duplicar contenido (el doble de largo)").clicked() {
                self.snap();
                if let Some(p) = self.p.pattern_mut(pid) {
                    let n = p.len_steps();
                    if p.bars * 2 <= 16 {
                        p.set_bars(p.bars * 2);
                        if let Some(ns) = p.notes.get_mut(&ch) {
                            let copy: Vec<Note> = ns.iter().map(|x| Note { s: x.s + n, ..*x }).collect();
                            ns.extend(copy);
                        }
                    }
                }
                self.send();
            }
            ui.separator();
            ui.label(RichText::new("PROGRESIÓN DE ACORDES (un acorde por compás)").small().color(DIM));
            for (name, degs) in templates::PROGRESSIONS {
                if ui.button(name).clicked() {
                    ui.close();
                    let seventh = self.tool == 4 || self.tool == 1;
                    edit(self, &|ns, n, p| {
                        ns.clear();
                        let bars = n / 16;
                        for b in 0..bars {
                            let deg = degs[(b % 4) as usize];
                            for m in templates::diatonic_chord(p, deg, 48, seventh) {
                                ns.push(Note { s: b * 16, l: 16, m, v: 0.75, o: 0.0 });
                            }
                        }
                    });
                    self.msg = format!("Progresión «{name}» en {}.", self.p.key_label());
                }
            }
        }));
        // arpegiador del canal
        let Some(c) = self.p.channel(ch).cloned() else { return };
        let mut arp = c.arp;
        let before = arp;
        ui.menu_button(if arp.on { "Arpegiador (activo)" } else { "Arpegiador" }, |ui| desplazable(ui, |ui| {
            ui.checkbox(&mut arp.on, "Encendido");
            egui::ComboBox::from_id_salt("arpm").selected_text(ARP_MODES[arp.mode as usize % ARP_MODES.len()]).show_ui(ui, |ui| {
                for (i, n) in ARP_MODES.iter().enumerate() {
                    ui.selectable_value(&mut arp.mode, i as u8, *n);
                }
            });
            ui.horizontal(|ui| {
                ui.label("Velocidad");
                for (v, n) in [(1u32, "1/16"), (2, "1/8"), (4, "1/4")] {
                    ui.selectable_value(&mut arp.rate, v, n);
                }
            });
            ui.horizontal(|ui| {
                ui.label("Octavas");
                for o in 1..=4u8 {
                    ui.selectable_value(&mut arp.oct, o, o.to_string());
                }
            });
            ui.add(egui::Slider::new(&mut arp.gate, 0.1..=1.0).text("Duración"));
            ui.label(RichText::new("Escribe acordes largos: el arpegiador los recorre al tocar.").small().color(DIM));
        }));
        if arp != before {
            self.snap();
            if let Some(cm) = self.p.channel_mut(ch) {
                cm.arp = arp;
            }
            self.send();
        }
    }

    pub(super) fn scale_picker(&mut self, ui: &mut Ui) {
        ui.label(RichText::new("ESCALA").small().color(DIM));
        let (mut k, mut s) = (self.p.key, self.p.scale);
        egui::ComboBox::from_id_salt("key").width(56.0).selected_text(KEY_NAMES[(k % 12) as usize]).show_ui(ui, |ui| {
            for (i, n) in KEY_NAMES.iter().enumerate() {
                ui.selectable_value(&mut k, i as u8, *n);
            }
        });
        egui::ComboBox::from_id_salt("scale").width(120.0).selected_text(SCALES[(s as usize).min(SCALES.len() - 1)].0).show_ui(ui, |ui| {
            for (i, (n, _)) in SCALES.iter().enumerate() {
                ui.selectable_value(&mut s, i as u8, *n);
            }
        });
        if (k, s) != (self.p.key, self.p.scale) {
            self.snap();
            self.p.key = k;
            self.p.scale = s;
            self.send();
        }
        if ui.add(egui::Button::selectable(self.scale_snap, "Imán")).on_hover_text("Las notas nuevas caen siempre dentro de la escala").clicked() {
            self.scale_snap = !self.scale_snap;
        }
    }

    // ============================ mastering y sonoridad ============================

    pub(super) fn lufs_text(&self) -> String {
        let l = self.lufs_hold;
        if l[1] <= -69.0 { "— LUFS".into() } else { format!("{:.1} LUFS", l[1]) }
    }

    /// Osciloscopio (forma de onda) y vectorscopio (imagen estéreo) del master.
    fn visualizadores(&mut self, ui: &mut Ui) {
        let datos = match self.status.estereo.lock() {
            Ok(s) => s.clone(),
            Err(_) => return,
        };
        if self.playing() {
            ui.ctx().request_repaint();
        }
        let n = datos.len() / 2;
        let lado = 170.0;
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            // ---------- osciloscopio
            ui.vertical(|ui| {
                ui.label(RichText::new("Osciloscopio").small().color(DIM));
                let (r, _) = ui.allocate_exact_size(Vec2::new(lado * 1.3, lado), Sense::hover());
                let p = ui.painter_at(r);
                p.rect_filled(r, 8, Color32::from_gray(12));
                p.hline(r.x_range(), r.center().y, Stroke::new(1.0, Color32::from_gray(35)));
                // se busca un cruce por cero para que la onda quede quieta
                let ventana = 512.min(n);
                let mut ini = n - ventana;
                for i in (1..n - ventana).rev().take(800) {
                    let (a, b) = (datos[(i - 1) * 2] + datos[(i - 1) * 2 + 1], datos[i * 2] + datos[i * 2 + 1]);
                    if a < 0.0 && b >= 0.0 {
                        ini = i;
                        break;
                    }
                }
                let pts: Vec<Pos2> = (0..ventana)
                    .map(|k| {
                        let i = ini + k;
                        let v = ((datos[i * 2] + datos[i * 2 + 1]) * 0.5).clamp(-1.0, 1.0);
                        Pos2::new(r.left() + k as f32 / ventana as f32 * r.width(), r.center().y - v * r.height() * 0.48)
                    })
                    .collect();
                p.add(egui::Shape::line(pts, Stroke::new(1.3, ACCENT)));
            });
            // ---------- vectorscopio
            ui.vertical(|ui| {
                ui.label(RichText::new("Vectorscopio (estéreo)").small().color(DIM));
                let (r, _) = ui.allocate_exact_size(Vec2::splat(lado), Sense::hover());
                let p = ui.painter_at(r);
                p.rect_filled(r, 8, Color32::from_gray(12));
                let c = r.center();
                let s = lado * 0.45;
                p.line_segment([c - Vec2::new(0.0, s), c + Vec2::new(0.0, s)], Stroke::new(1.0, Color32::from_gray(35)));
                p.line_segment([c - Vec2::new(s * 0.7, s * 0.7), c + Vec2::new(s * 0.7, s * 0.7)], Stroke::new(1.0, Color32::from_gray(28)));
                p.line_segment([c - Vec2::new(s * 0.7, -s * 0.7), c + Vec2::new(s * 0.7, -s * 0.7)], Stroke::new(1.0, Color32::from_gray(28)));
                let mut corr = (0f32, 0f32, 0f32);
                let col = Color32::from_rgba_unmultiplied(230, 230, 235, 70);
                for i in (0..n).step_by(2) {
                    let (l, rr) = (datos[i * 2], datos[i * 2 + 1]);
                    corr.0 += l * rr;
                    corr.1 += l * l;
                    corr.2 += rr * rr;
                    // centro (mono) arriba-abajo, lados izquierda-derecha
                    let x = ((rr - l) * 0.707).clamp(-1.0, 1.0);
                    let y = ((l + rr) * 0.707).clamp(-1.0, 1.0);
                    p.circle_filled(c + Vec2::new(x * s, -y * s), 0.9, col);
                }
                let k = if corr.1 * corr.2 > 1e-9 { corr.0 / (corr.1 * corr.2).sqrt() } else { 1.0 };
                ui.label(RichText::new(format!("Correlación {k:+.2}")).small().color(if k < 0.0 { RED } else { DIM }))
                    .on_hover_text("+1 = mono, 0 = muy abierto, negativo = se cancela en mono (cuidado)");
            });
        });
    }

    pub(super) fn master_window(&mut self, ui: &mut Ui) {
        let l = self.lufs_hold;
        let tp = 20.0 * self.tpeak_hold.max(1e-6).log10();
        egui::Grid::new("lufsg").spacing(Vec2::new(14.0, 4.0)).show(ui, |ui| {
            for (n, v) in [("Momentáneo", l[0]), ("Corto plazo (3 s)", l[1]), ("Integrado", l[2])] {
                ui.label(RichText::new(n).small().color(DIM));
                ui.label(RichText::new(if v <= -69.0 { "—".into() } else { format!("{v:.1} LUFS") }).monospace().size(15.0).color(ACCENT2));
                ui.end_row();
            }
            ui.label(RichText::new("Pico").small().color(DIM));
            ui.label(RichText::new(format!("{tp:.1} dBFS")).monospace().size(15.0).color(if tp > -0.5 { RED } else { ACCENT2 }));
            ui.end_row();
        });
        // barra de sonoridad
        let (r, _) = ui.allocate_exact_size(Vec2::new(360.0, 16.0), Sense::hover());
        let p = ui.painter();
        p.rect_filled(r, 3, Color32::from_gray(12));
        let fx = |db: f32| r.left() + ((db + 36.0) / 36.0).clamp(0.0, 1.0) * r.width();
        p.rect_filled(Rect::from_min_max(r.min, Pos2::new(fx(l[1]), r.bottom())), 3, if l[1] > -9.0 { RED } else if l[1] > -16.0 { GREEN } else { Color32::from_gray(150) });
        let tgt = MASTER_TARGETS[self.master_target].1;
        p.vline(fx(tgt), r.y_range(), Stroke::new(2.0, Color32::WHITE));
        ui.label(RichText::new("Reproduce la canción en SONG para medir. La línea blanca es el objetivo.").small().color(DIM));
        self.visualizadores(ui);
        ui.separator();
        ui.horizontal(|ui| {
            if boton_metal(ui, "Abrir el masterizado", Vec2::new(200.0, 34.0)).on_hover_text("Estilos, intensidad, ecualizador y referencia (también desde la píldora de la barra de arriba)").clicked() {
                self.mast_panel = true;
            }
        });
    }

    pub(super) fn start_mastering(&mut self) {
        self.masterizar(false);
    }

    /// Masteriza la canción en segundo plano (estilo + volumen final). `auto`: lanzado solo al cambiar algo del panel.
    /// Si ya había uno en curso, se cancela y empieza de nuevo con los ajustes actuales.
    pub(super) fn masterizar(&mut self, auto: bool) {
        if self.p.song_bars() == 0 {
            if !auto {
                self.msg = "La playlist está vacía: no hay nada que masterizar.".into();
            }
            return;
        }
        if let Some(c) = &self.mast_cancel {
            c.store(true, Ordering::Relaxed);
        }
        let cancel = Arc::new(AtomicBool::new(false));
        self.mast_cancel = Some(cancel.clone());
        let (tx, rx) = mpsc::channel();
        let mut p = self.p.clone();
        let bank = self.bank();
        let eq3 = if self.master_eq_on { self.master_eq3 } else { [0.0; 3] };
        let (estilo, int, referencia) = (self.master_estilo, self.master_int, self.master_ref.clone());
        let target = referencia.as_ref().map(|r| r.1.clamp(-20.0, -7.0)).unwrap_or(int_valores(int).0);
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let pr = AtomicU32::new(0);
            let (mut eq, comp) = cadena_master(estilo, int, eq3);
            p.master_eq = eq;
            p.master_comp = comp;
            p.limiter = LimP { on: true, gain_db: 0.0, ceiling_db: -1.0, release_ms: 60.0 };
            let mut gain = 0.0f32;
            let mut lufs = -70.0;
            // con referencia: se mide la mezcla y se corrige su balance hacia el de la referencia
            if let Some((_, _, bal_ref)) = &referencia {
                let a = crate::engine::render_song(&p, &bank, 44_100.0, &pr, &cancel);
                if cancel.load(Ordering::Relaxed) {
                    return;
                }
                let bal = balance_3(&a, 44_100.0);
                let d: Vec<f32> = (0..3).map(|k| ((bal_ref[k] - bal[k]) * 0.7).clamp(-4.0, 4.0)).collect();
                eq = cadena_master(estilo, int, [eq3[0] + d[0], eq3[1] + d[1], eq3[2] + d[2]]).0;
                p.master_eq = eq;
            }
            for _ in 0..3 {
                p.limiter.gain_db = gain;
                let a = crate::engine::render_song(&p, &bank, 44_100.0, &pr, &cancel);
                if cancel.load(Ordering::Relaxed) {
                    return;
                }
                let (l, _) = crate::loudness::measure(&a, 44_100.0);
                lufs = l;
                if l <= -69.0 {
                    let _ = tx.send(Err("La canción está en silencio.".into()));
                    return;
                }
                let d = target - l;
                if d.abs() < 0.4 {
                    break;
                }
                gain = (gain + d).clamp(-12.0, 18.0);
            }
            let _ = tx.send(Ok((gain, lufs, p.master_eq, p.master_comp)));
            if let Some(c) = ctx {
                c.request_repaint();
            }
        });
        self.master_job = Some((rx, auto));
        if !auto {
            self.msg = "Masterizando la canción…".into();
        }
    }

    pub(super) fn poll_master(&mut self) {
        let Some((rx, auto)) = &self.master_job else { return };
        let auto = *auto;
        match rx.try_recv() {
            Ok(r) => {
                self.master_job = None;
                match r {
                    Ok((gain, lufs, eq, comp)) => {
                        // los cambios del panel ya dejaron su punto de deshacer
                        if !auto {
                            self.snap();
                        }
                        self.p.master_eq = eq;
                        self.p.master_comp = comp;
                        self.p.limiter = LimP { on: true, gain_db: gain, ceiling_db: -1.0, release_ms: 60.0 };
                        self.master_lufs = Some((lufs, gain));
                        self.cambios = true;
                        self.msg = format!("Masterizado «{}»: {:.1} LUFS ({:+.1} dB de volumen).", MASTER_ESTILOS[self.master_estilo].0, lufs, gain);
                        if self.sin_master {
                            self.poner_sin_master(false);
                        }
                        self.send();
                    }
                    Err(e) => self.msg = e,
                }
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(_) => {
                // cancelado porque empezó otro con ajustes nuevos: no se avisa nada
                self.master_job = None;
            }
        }
    }

    // ============================ ajustes, plantillas, presets y versiones ============================

    pub(super) fn load_settings(data: &Path) -> Settings {
        std::fs::read_to_string(data.join("ajustes.json")).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }
    pub(super) fn save_settings(&self) {
        if let Ok(s) = serde_json::to_string_pretty(&self.settings) {
            let _ = std::fs::write(self.plat.data_dir.join("ajustes.json"), s);
        }
    }

    pub(super) fn settings_window(&mut self, ui: &mut Ui) {
        let antes = format!("{:?}", self.settings);
        let secciones = ["General", "Audio", "Grabación", "MIDI", "Exportar", "Pantalla", "Plugins", "Metrónomo"];
        let mut reiniciar_audio = false;
        ui.horizontal_top(|ui| {
            // columna de secciones
            ui.vertical(|ui| {
                ui.set_width(130.0);
                for (i, n) in secciones.iter().enumerate() {
                    #[cfg(not(windows))]
                    if *n == "Plugins" {
                        continue;
                    }
                    if ui.add(egui::Button::selectable(self.ajuste_sec == i, RichText::new(*n).size(13.5)).min_size(Vec2::new(126.0, 30.0))).clicked() {
                        self.ajuste_sec = i;
                    }
                }
            });
            ui.add_space(10.0);
            ui.vertical(|ui| {
                ui.set_width(420.0);
                ui.set_min_height(380.0);
                let titulo = |ui: &mut Ui, t: &str| {
                    ui.label(RichText::new(t).strong().size(15.0).color(ACCENT2));
                    ui.add_space(4.0);
                };
                let nota = |ui: &mut Ui, t: &str| {
                    ui.label(RichText::new(t).small().color(DIM));
                    ui.add_space(6.0);
                };
                match self.ajuste_sec {
                    0 => {
                        titulo(ui, "Al abrir el programa");
                        ui.checkbox(&mut self.settings.abrir_ultimo, "Abrir el último proyecto");
                        nota(ui, "Si está apagado, siempre empiezas con un proyecto nuevo.");
                        ui.horizontal(|ui| {
                            ui.label("Proyectos nuevos con");
                            egui::ComboBox::from_id_salt("plant_nueva").width(160.0).selected_text(crate::templates::TEMPLATES[self.settings.plantilla_nueva.min(crate::templates::TEMPLATES.len() - 1)]).show_ui(ui, |ui| {
                                for (i, n) in crate::templates::TEMPLATES.iter().enumerate() {
                                    ui.selectable_value(&mut self.settings.plantilla_nueva, i, *n);
                                }
                            });
                        });
                        if self.settings.plantilla_nueva == 0 {
                            ui.add(egui::Slider::new(&mut self.settings.bpm_nuevo, 60.0..=200.0).step_by(1.0).suffix(" bpm").text("Tempo inicial"));
                        }
                        ui.add_space(10.0);
                        titulo(ui, "Guardar");
                        ui.horizontal(|ui| {
                            ui.label("Guardar solo");
                            egui::ComboBox::from_id_salt("autog").width(150.0).selected_text(match self.settings.autoguardar_min {
                                0 => "Nunca".to_string(),
                                m => format!("Cada {m} minutos"),
                            }).show_ui(ui, |ui| {
                                for m in [0u32, 1, 2, 5, 10, 15] {
                                    ui.selectable_value(&mut self.settings.autoguardar_min, m, if m == 0 { "Nunca".to_string() } else { format!("Cada {m} minutos") });
                                }
                            });
                        });
                        nota(ui, "Si el proyecto todavía no tiene nombre de archivo, se guarda como copia de recuperación.");
                        ui.checkbox(&mut self.settings.versiones, "Guardar copias de respaldo (Archivo > Versiones anteriores)");
                        if self.settings.versiones {
                            ui.add(egui::Slider::new(&mut self.settings.version_min, 1..=30).suffix(" min").text("Una copia cada"));
                        }
                        ui.checkbox(&mut self.settings.preguntar, "Preguntar antes de cerrar si hay cambios sin guardar");
                        ui.add_space(10.0);
                        titulo(ui, "Tutorial");
                        if ui.button("Ver el tutorial ahora").clicked() {
                            self.tut_mostrar(0);
                        }
                    }
                    1 => {
                        titulo(ui, "Salida (parlantes o audífonos)");
                        if self.entradas_t.is_none_or(|t| t.elapsed() > Duration::from_secs(3)) {
                            self.entradas = audio::inputs();
                            self.salidas = audio::outputs();
                            self.entradas_t = Some(Instant::now());
                        }
                        let salidas = self.salidas.clone();
                        let actual = if self.settings.salida.is_empty() { audio::PREDETERMINADO_SALIDA.to_string() } else { self.settings.salida.clone() };
                        egui::ComboBox::from_id_salt("salida").width(380.0).selected_text(actual).show_ui(ui, |ui| {
                            if ui.selectable_label(self.settings.salida.is_empty(), audio::PREDETERMINADO_SALIDA).clicked() {
                                self.settings.salida.clear();
                                reiniciar_audio = true;
                            }
                            for n in &salidas {
                                if ui.selectable_label(self.settings.salida == *n, n).clicked() {
                                    self.settings.salida = n.clone();
                                    reiniciar_audio = true;
                                }
                            }
                        });
                        if let Some(a) = &self.audio {
                            nota(ui, &format!("Sonando por: {} · {} Hz", a.device, a.sr));
                        } else if let Some(e) = &self.audio_err {
                            ui.label(RichText::new(format!("Sin audio: {e}")).color(RED));
                        }
                        titulo(ui, "Entrada (micrófono)");
                        let entradas = self.entradas.clone();
                        let actual = if self.settings.entrada.is_empty() { audio::PREDETERMINADO_ENTRADA.to_string() } else { self.settings.entrada.clone() };
                        let antes_e = self.settings.entrada.clone();
                        egui::ComboBox::from_id_salt("entrada").width(380.0).selected_text(actual).show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.settings.entrada, String::new(), audio::PREDETERMINADO_ENTRADA);
                            for n in &entradas {
                                ui.selectable_value(&mut self.settings.entrada, n.clone(), n);
                            }
                        });
                        if self.settings.entrada != antes_e {
                            // la escucha pasa al micrófono nuevo
                            self.mic_mon = None;
                            self.escucha_sync();
                        }
                        ui.add_space(10.0);
                        titulo(ui, "Latencia");
                        let antes_b = self.settings.buffer;
                        ui.horizontal(|ui| {
                            ui.label("Tamaño del bloque");
                            egui::ComboBox::from_id_salt("buffer").width(200.0).selected_text(match self.settings.buffer {
                                0 => "Automático".to_string(),
                                b => format!("{b} muestras"),
                            }).show_ui(ui, |ui| {
                                for b in [0u32, 128, 256, 512, 1024, 2048] {
                                    ui.selectable_value(&mut self.settings.buffer, b, if b == 0 { "Automático".to_string() } else { format!("{b} muestras") });
                                }
                            });
                        });
                        if self.settings.buffer != antes_b {
                            reiniciar_audio = true;
                        }
                        nota(ui, "Más chico: responde más rápido pero exige más al computador. Si se escuchan cortes, súbelo.");
                        if ui.button("Reiniciar el audio").clicked() {
                            reiniciar_audio = true;
                        }
                    }
                    7 => {
                        titulo(ui, "Metrónomo");
                        ui.horizontal(|ui| {
                            ui.label("Sonido");
                            for (k, n) in ["Clásico", "Madera", "Electrónico", "Suave"].iter().enumerate() {
                                if ui.add(egui::Button::selectable(self.settings.metro_sonido as usize == k, *n)).clicked() {
                                    self.settings.metro_sonido = k as u8;
                                }
                            }
                        });
                        ui.add(egui::Slider::new(&mut self.settings.metro_vol, 0.0..=1.5).custom_formatter(|v, _| format!("{:.0} %", v * 100.0)).text("Volumen"));
                        ui.horizontal(|ui| {
                            ui.label("Golpes");
                            for (k, n) in ["Cada tiempo (negras)", "Cada medio tiempo (corcheas)"].iter().enumerate() {
                                if ui.add(egui::Button::selectable(self.settings.metro_sub as usize == k, *n)).clicked() {
                                    self.settings.metro_sub = k as u8;
                                }
                            }
                        });
                        ui.checkbox(&mut self.settings.metro_acento, "Marcar más fuerte el primer tiempo de cada compás");
                        ui.checkbox(&mut self.settings.metro_grabar, "Prender el metrónomo mientras se graba");
                        ui.horizontal(|ui| {
                            ui.label("Precuenta antes de grabar");
                            for (k, n) in ["No", "1 compás", "2 compases"].iter().enumerate() {
                                if ui.add(egui::Button::selectable(self.settings.precuenta as usize == k, *n)).clicked() {
                                    self.settings.precuenta = k as u8;
                                }
                            }
                        });
                        nota(ui, "El metrónomo se prende y apaga con su botón en la barra de arriba (o la tecla M). La flechita junto a ese botón abre un menú rápido con tempo, sonido, volumen y precuenta.");
                    }
                    2 => {
                        titulo(ui, "Grabación");
                        ui.add(egui::Slider::new(&mut self.settings.latency_ms, 0.0..=250.0).suffix(" ms").text("Compensación de latencia"));
                        nota(ui, "Si lo grabado queda corrido (suena tarde respecto al ritmo), sube este valor.");
                        ui.checkbox(&mut self.settings.metro_grabar, "Metrónomo mientras se graba");
                        ui.add_space(6.0);
                        nota(ui, "Para escucharte y usar el autotune en vivo, abre el botón «Micrófono» de la barra de arriba.");
                        nota(ui, "Solo se escucha en los audífonos o parlantes; no queda en la grabación.");
                    }
                    3 => {
                        titulo(ui, "Teclado MIDI");
                        #[cfg(not(target_os = "android"))]
                        {
                            let ports = midi::input::ports();
                            if ports.is_empty() {
                                ui.label("No hay teclados MIDI conectados.");
                            }
                            let mut pick = false;
                            for p in ports.iter() {
                                let on = self.midi_conn.as_ref().is_some_and(|c| c.name == *p);
                                if ui.add(egui::Button::selectable(on, p)).clicked() {
                                    pick = true;
                                    self.settings.midi_port = p.clone();
                                }
                            }
                            if pick || ui.button("Buscar teclados").clicked() {
                                self.midi_connect();
                            }
                        }
                        #[cfg(target_os = "android")]
                        ui.label("En Android el teclado MIDI todavía no está disponible.");
                        ui.add_space(6.0);
                        ui.checkbox(&mut self.settings.midi_quant, "Acomodar al ritmo las notas que grabo (cuantizar)");
                    }
                    4 => {
                        titulo(ui, "Dónde se guarda");
                        let dir = self.plat.export_dir.display().to_string();
                        ui.label(RichText::new(dir).small());
                        ui.horizontal(|ui| {
                            #[cfg(not(target_os = "android"))]
                            if ui.button("Elegir carpeta…").clicked() {
                                if let Some(d) = rfd::FileDialog::new().set_directory(&self.plat.export_dir).pick_folder() {
                                    self.settings.export_dir = d.to_string_lossy().to_string();
                                    self.plat.export_dir = d;
                                }
                            }
                            if !self.settings.export_dir.is_empty() && ui.button("Volver al Escritorio").clicked() {
                                self.settings.export_dir.clear();
                                self.plat.export_dir = self.export_dir_inicial.clone();
                            }
                        });
                        ui.add_space(10.0);
                        titulo(ui, "Calidad");
                        ui.horizontal(|ui| {
                            ui.label("MP3");
                            for k in [128u32, 192, 256, 320] {
                                ui.selectable_value(&mut self.settings.mp3_kbps, k, format!("{k} kbps"));
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label("WAV");
                            ui.selectable_value(&mut self.settings.wav_bits, 16, "16 bits");
                            ui.selectable_value(&mut self.settings.wav_bits, 24, "24 bits");
                        });
                        ui.horizontal(|ui| {
                            ui.label("Frecuencia");
                            ui.selectable_value(&mut self.settings.sr_export, 44_100, "44.1 kHz");
                            ui.selectable_value(&mut self.settings.sr_export, 48_000, "48 kHz");
                        });
                        nota(ui, "44.1 kHz es lo normal para música; 48 kHz para video.");
                        titulo(ui, "Nivel");
                        ui.radio_value(&mut self.settings.normalizar, 0, "Automático (subir el volumen solo si el limitador está apagado)");
                        ui.radio_value(&mut self.settings.normalizar, 1, "Siempre subir al máximo sin distorsionar");
                        ui.radio_value(&mut self.settings.normalizar, 2, "Nunca tocar el volumen");
                    }
                    5 => {
                        titulo(ui, "Tamaño de la interfaz");
                        ui.horizontal(|ui| {
                            for (z, n) in [(0.85f32, "Chico"), (1.0, "Normal"), (1.15, "Grande"), (1.3, "Muy grande")] {
                                ui.selectable_value(&mut self.settings.zoom, z, n);
                            }
                        });
                        ui.add(egui::Slider::new(&mut self.settings.zoom, 0.7..=1.6).step_by(0.05).text("Escala"));
                        ui.add_space(10.0);
                        titulo(ui, "Ayudas");
                        ui.checkbox(&mut self.settings.ayudas, "Mostrar textos de ayuda junto a las herramientas");
                    }
                    _ => {
                        #[cfg(windows)]
                        {
                            titulo(ui, "Plugins CLAP");
                            nota(ui, "Se buscan en Archivos de programa > Common Files > CLAP y en estas carpetas:");
                            let mut rm = None;
                            for (i, d) in self.settings.plugin_dirs.iter().enumerate() {
                                ui.horizontal(|ui| {
                                    ui.label(d);
                                    if ui.small_button("Quitar").clicked() {
                                        rm = Some(i);
                                    }
                                });
                            }
                            if let Some(i) = rm {
                                self.settings.plugin_dirs.remove(i);
                            }
                            if ui.button("Agregar carpeta…").clicked() {
                                if let Some(d) = rfd::FileDialog::new().pick_folder() {
                                    self.settings.plugin_dirs.push(d.to_string_lossy().to_string());
                                }
                            }
                            if ui.button("Volver a buscar plugins").clicked() {
                                self.rescan_plugins();
                            }
                        }
                    }
                }
            });
        });
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("Restablecer todo").on_hover_text("Vuelve a los valores de fábrica").clicked() {
                let fav = std::mem::take(&mut self.settings.fav_dirs);
                let plug = std::mem::take(&mut self.settings.plugin_dirs);
                let tut = self.settings.tutorial_done;
                self.settings = Settings { fav_dirs: fav, plugin_dirs: plug, tutorial_done: tut, ..Settings::default() };
                self.plat.export_dir = self.export_dir_inicial.clone();
                reiniciar_audio = true;
            }
            ui.label(RichText::new("Los cambios se guardan solos.").small().color(DIM));
        });
        if reiniciar_audio {
            self.reiniciar_audio();
        }
        if antes != format!("{:?}", self.settings) {
            self.save_settings();
        }
    }

    /// Vuelve a abrir la salida de audio con los ajustes actuales y le reenvía todo al motor.
    pub(super) fn reiniciar_audio(&mut self) {
        self.audio = None;
        let abierto = match audio::start(self.p.clone(), self.status.clone(), &self.settings.salida, self.settings.buffer) {
            Ok(a) => {
                self.msg = format!("Audio: {}", a.device);
                Ok(a)
            }
            Err(e) => {
                // si el dispositivo elegido falla (se desconectó), se vuelve al predeterminado
                self.msg = format!("No se pudo abrir «{}» ({e}). Se usa la salida predeterminada.", self.settings.salida);
                audio::start(self.p.clone(), self.status.clone(), "", 0).map_err(|_| e)
            }
        };
        match abierto {
            Ok(mut a) => {
                self.trash_rx = a.trash.take();
                self.audio = Some(a);
                self.audio_err = None;
                // el motor nuevo empieza vacío: se le reenvían los audios, las tomas y el metrónomo
                let files: Vec<(String, Arc<Buf>)> = self.files.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                for (k, b) in files {
                    self.cmd(Cmd::File { key: k, buf: Some(b) });
                }
                let takes: Vec<((u32, u32), (Arc<Vec<f32>>, f32))> = self.samples.iter().map(|(k, v)| (*k, v.clone())).collect();
                for ((pat, ch), d) in takes {
                    self.cmd(Cmd::Sample { pat, ch, data: Some(d) });
                }
                self.cmd(Cmd::Metronome(self.metro));
                self.plugins_reload_all();
            }
            Err(e) => self.audio_err = Some(e),
        }
    }

    pub(super) fn new_from_template(&mut self, i: usize) {
        self.snap();
        self.p = templates::build(i);
        self.fix_selection();
        self.send();
        self.reload_takes();
        self.plugins_reload_all();
        self.proj_path = None;
        self.cambios = false;
        self.msg = format!("Proyecto nuevo: plantilla «{}» ({} · {:.0} bpm).", templates::TEMPLATES[i], self.p.key_label(), self.p.bpm);
    }

    fn user_dir(&self, sub: &str) -> PathBuf {
        let d = self.plat.data_dir.join(sub);
        let _ = std::fs::create_dir_all(&d);
        d
    }
    fn list_json(&self, sub: &str) -> Vec<PathBuf> {
        let mut v: Vec<PathBuf> = std::fs::read_dir(self.user_dir(sub)).map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "json")).collect()).unwrap_or_default();
        v.sort_by_key(|p| std::cmp::Reverse(std::fs::metadata(p).and_then(|m| m.modified()).ok()));
        v
    }

    pub(super) fn save_template(&mut self) {
        let path = self.user_dir("plantillas").join(format!("{}.json", export::safe_name(&self.p.name)));
        let mut p = self.p.clone();
        p.playlist.clear();
        p.audio_clips.clear();
        p.autos.clear();
        self.msg = match serde_json::to_string(&p).map_err(|e| e.to_string()).and_then(|s| std::fs::write(&path, s).map_err(|e| e.to_string())) {
            Ok(_) => format!("Plantilla guardada: «{}» (canales, patrones y mezcla, sin la playlist).", self.p.name),
            Err(e) => format!("No se pudo guardar la plantilla: {e}"),
        };
    }

    pub(super) fn file_menu_extra(&mut self, ui: &mut Ui) {
        ui.menu_button("Nuevo desde plantilla", |ui| desplazable(ui, |ui| {
            for (i, n) in templates::TEMPLATES.iter().enumerate() {
                if ui.button(*n).clicked() {
                    ui.close();
                    self.pedir(Pend::Nuevo(i));
                }
            }
            let mine = self.list_json("plantillas");
            if !mine.is_empty() {
                ui.separator();
                ui.label(RichText::new("MIS PLANTILLAS").small().color(DIM));
                for p in mine {
                    let n = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                    if ui.button(n).clicked() {
                        ui.close();
                        self.open_path(p);
                    }
                }
            }
        }));
        if ui.button("Guardar como plantilla").clicked() {
            ui.close();
            self.save_template();
        }
        ui.menu_button("Versiones anteriores", |ui| desplazable(ui, |ui| {
            let v = self.list_json("versiones");
            if v.is_empty() {
                ui.label("Todavía no hay versiones guardadas.");
            }
            let now = std::time::SystemTime::now();
            for p in v.into_iter().take(30) {
                let age = std::fs::metadata(&p).and_then(|m| m.modified()).ok().and_then(|t| now.duration_since(t).ok()).map(|d| d.as_secs()).unwrap_or(0);
                let n = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                let n = n.rsplit_once(" @").map(|x| x.0.to_string()).unwrap_or(n);
                if ui.button(format!("{n} — {}", ago(age))).clicked() {
                    ui.close();
                    self.open_path(p);
                }
            }
        }));
        ui.separator();
        if ui.button("Importar audio a la playlist…").clicked() {
            ui.close();
            // en el teléfono se abre el selector de archivos del teléfono; en la PC, el de Windows
            if self.plat.mobile {
                self.elegir_audio = Some(None);
            } else if let Some(p) = self.pick_audio() {
                let t = self.free_track(self.play_from, 16);
                self.import_to_playlist(&p, t, self.play_from);
            }
        }
        if ui.button("Importar sample al rack…").clicked() {
            ui.close();
            if let Some(p) = self.pick_audio() {
                self.import_to_rack(&p);
            } else if self.plat.mobile {
                self.left_tab = 1;
            }
        }
        if ui.button("Importar MIDI…").clicked() {
            ui.close();
            self.import_midi_dialog();
        }
        if ui.button("Exportar MIDI del patrón…").clicked() {
            ui.close();
            self.export_midi();
        }
        if ui.add_enabled(self.job.is_none(), egui::Button::new("Exportar stems (una pista WAV por canal)")).clicked() {
            ui.close();
            self.start_export(Format::Stems);
        }
        ui.menu_button("Aplicar kit de batería", |ui| desplazable(ui, |ui| {
            for (i, n) in templates::KITS.iter().enumerate() {
                if ui.button(*n).clicked() {
                    ui.close();
                    self.snap();
                    templates::apply_kit(&mut self.p, i);
                    self.msg = format!("Kit «{n}» aplicado a la batería.");
                    self.send();
                }
            }
        }));
        ui.separator();
        if ui.button("Ver tutorial").clicked() {
            ui.close();
            self.tut = Some(0);
        }
        if ui.button("Ajustes…").clicked() {
            ui.close();
            self.open_win(Win::Settings);
        }
    }

    /// Guarda una copia del proyecto cada cierto tiempo (historial de versiones).
    pub(super) fn version_tick(&mut self) {
        if !self.settings.versiones || !self.version_dirty || self.last_version.elapsed() < Duration::from_secs(self.settings.version_min.max(1) as u64 * 60) {
            return;
        }
        self.version_dirty = false;
        self.last_version = Instant::now();
        let dir = self.user_dir("versiones");
        let path = dir.join(format!("{} @{}.json", export::safe_name(&self.p.name), now_secs()));
        if let Ok(s) = serde_json::to_string(&self.p) {
            let _ = std::fs::write(path, s);
        }
        let mut v = self.list_json("versiones");
        while v.len() > 40 {
            if let Some(old) = v.pop() {
                let _ = std::fs::remove_file(old);
            }
        }
    }

    pub(super) fn channel_presets(&mut self, ui: &mut Ui, id: u32) {
        let Some(c) = self.p.channel(id).cloned() else { return };
        ui.menu_button("Presets", |ui| desplazable(ui, |ui| {
            if ui.button("Guardar preset del canal").clicked() {
                ui.close();
                let path = self.user_dir("presets").join(format!("{} - {}.json", c.inst.name(), export::safe_name(&c.name)));
                self.msg = match serde_json::to_string_pretty(&c).map_err(|e| e.to_string()).and_then(|s| std::fs::write(&path, s).map_err(|e| e.to_string())) {
                    Ok(_) => format!("Preset «{}» guardado.", c.name),
                    Err(e) => format!("No se pudo guardar: {e}"),
                };
            }
            if matches!(c.inst, Inst::Audio | Inst::Vocal) {
                ui.separator();
                ui.label(RichText::new("EFECTOS DE VOZ").small().color(DIM));
                for (i, (n, ayuda, ..)) in VOZ_FX.iter().enumerate() {
                    if ui.button(*n).on_hover_text(*ayuda).clicked() {
                        ui.close();
                        self.snap();
                        if let Some(cm) = self.p.channel_mut(id) {
                            aplicar_voz_fx(cm, i);
                        }
                        self.msg = format!("Efectos de voz: {n}");
                        self.send();
                    }
                }
            }
            if c.inst == Inst::Synth {
                ui.separator();
                ui.label(RichText::new("NN WAVE").small().color(DIM));
                for (i, n) in SYNTH_PRESETS.iter().enumerate() {
                    if ui.button(*n).clicked() {
                        ui.close();
                        self.snap();
                        if let Some(cm) = self.p.channel_mut(id) {
                            cm.synth = SynthP::preset(i);
                        }
                        self.send();
                        self.preview(id, 60, 0.85);
                    }
                }
            }
            let list: Vec<PathBuf> = self.list_json("presets").into_iter().filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with(c.inst.name()))).collect();
            if !list.is_empty() {
                ui.separator();
                ui.label(RichText::new("MIS PRESETS").small().color(DIM));
            }
            for p in list {
                let n = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                if ui.button(n.trim_start_matches(c.inst.name()).trim_start_matches(" - ")).clicked() {
                    ui.close();
                    if let Some(mut pc) = std::fs::read_to_string(&p).ok().and_then(|s| serde_json::from_str::<Channel>(&s).ok()) {
                        self.snap();
                        pc.id = id;
                        pc.mute = c.mute;
                        pc.solo = c.solo;
                        if let Some(cm) = self.p.channel_mut(id) {
                            *cm = pc;
                        }
                        self.send();
                        self.plugins_reload_channel(id);
                    }
                }
            }
        }));
    }

    /// Todo el audio que necesita una exportación o un análisis.
    pub(super) fn bank(&self) -> crate::engine::Bank {
        crate::engine::Bank { takes: self.samples.clone(), files: self.files.clone() }
    }
}

/// Lista de canales a los que puede apuntar un sidechain.
pub(super) fn sc_sources(p: &Project, me: u32) -> Vec<(u32, String)> {
    p.channels.iter().filter(|c| c.id != me).map(|c| (c.id, c.name.clone())).collect()
}

