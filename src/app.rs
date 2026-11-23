//! Interfaz de NoName Studio (egui nativo).
use crate::audio;
use crate::engine::{Cmd, Mode, Status};
use crate::export::{self, Format};
use crate::project::*;
use egui::{Align, Align2, Color32, CornerRadius, FontId, Frame, Key, Layout, Margin, Pos2, Rect, Response, RichText, Sense, Stroke, StrokeKind, Ui, Vec2};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

#[path = "plugins_ui.rs"]
mod plugins_ui;
use plugins_ui::*;
#[path = "studio_ui.rs"]
mod studio_ui;
use studio_ui::*;
#[path = "plugin_ui.rs"]
mod plugin_ui;
#[path = "tutorial.rs"]
mod tutorial;
#[path = "rec_view.rs"]
mod rec_view;
#[path = "masterizado_view.rs"]
mod masterizado_view;
#[path = "pads_view.rs"]
mod pads_view;
#[path = "efectos_view.rs"]
mod efectos_view;
#[path = "retune_view.rs"]
mod retune_view;
use retune_view::Retune;
use plugin_ui::*;
use std::path::Path;

pub struct Platform {
    pub data_dir: PathBuf,
    pub export_dir: PathBuf,
    pub mobile: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum View {
    Playlist,
    Rack,
    Roll,
    Mixer,
    Pads,
}

// Paleta monocroma «metal líquido»: negro profundo, grises neutros y plata.
const ACCENT: Color32 = Color32::from_gray(229);
const ACCENT2: Color32 = Color32::from_gray(193);
const BG: Color32 = Color32::from_gray(10);
const PANEL: Color32 = Color32::from_gray(17);
const PANEL2: Color32 = Color32::from_gray(27);
const LINE: Color32 = Color32::from_gray(40);
const DIM: Color32 = Color32::from_gray(125);
const GREEN: Color32 = Color32::from_rgb(58, 214, 140);
const RED: Color32 = Color32::from_rgb(255, 76, 80);

const CHORDS: [(&str, &[i32]); 10] = [
    ("Nota", &[0]),
    ("Acorde m9 abierto", &[0, 7, 15, 22, 26]),
    ("Acorde maj9 abierto", &[0, 7, 16, 23, 26]),
    ("Acorde m11 abierto", &[0, 7, 10, 15, 17]),
    ("Acorde m7 abierto (drop 2)", &[0, 10, 15, 19]),
    ("Acorde 7b13 abierto", &[0, 10, 16, 20]),
    ("Acorde sus4 abierto", &[0, 7, 10, 17]),
    ("Tríada menor abierta", &[0, 7, 15]),
    ("Tríada mayor abierta", &[0, 7, 16]),
    ("Octava", &[0, 12]),
];
const NOTE_NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
const LOW: u8 = 24;
const HIGH: u8 = 96;
const RH: f32 = 13.0;
const CW: f32 = 22.0;
const KW: f32 = 56.0;
const BW: f32 = 46.0;
const TH: f32 = 74.0;

fn nname(m: u8) -> String {
    format!("{}{}", NOTE_NAMES[(m % 12) as usize], m as i32 / 12 - 1)
}
fn c32(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}
fn is_black(m: u8) -> bool {
    matches!(m % 12, 1 | 3 | 6 | 8 | 10)
}

enum RollDrag {
    Move { idx: Vec<usize>, orig: Vec<(u32, u8)>, s0: i32, m0: i32 },
    Resize { idx: Vec<usize> },
}
struct ExportJob {
    progress: Arc<AtomicU32>,
    cancel: Arc<AtomicBool>,
    rx: mpsc::Receiver<Result<export::Done, String>>,
    fmt: Format,
    started: Instant,
}

pub struct StudioApp {
    p: Project,
    hist: Vec<Project>,
    status: Arc<Status>,
    audio: Option<audio::AudioOut>,
    audio_err: Option<String>,
    song_mode: bool,
    metro: bool,
    sel_pat: u32,
    sel_ch: u32,
    view: View,
    show_playlist: bool,
    tool: usize,
    note_len: u32,
    note_vel: f32,
    add_inst: Inst,
    open_set: HashSet<u32>,
    roll_drag: Option<RollDrag>,
    step_drag: Option<(u32, usize)>,
    roll_x: f32,
    roll_center: bool,
    msg: String,
    job: Option<ExportJob>,
    logo: Option<egui::TextureHandle>,
    plat: Platform,
    dirty: Option<Instant>,
    meters: Vec<f32>,
    mmeter: [f32; 2],
    open_list: Option<Vec<PathBuf>>,
    pub autotest: Option<AutoTest>,
    wins: Vec<Win>,
    eq_drag: Option<usize>,
    spec: Vec<f32>,
    samples: crate::engine::Samples,
    raws: std::collections::HashMap<(u32, u32), (Arc<Vec<f32>>, f32)>,
    takes: std::collections::HashMap<(u32, u32), TakeView>,
    rec: Option<RecState>,
    in_meter: f32,
    tune_tx: mpsc::Sender<TuneDone>,
    tune_rx: mpsc::Receiver<TuneDone>,
    tune_busy: usize,
    maximized: bool,
    gr_hold: [f32; 2],
    ctx: Option<egui::Context>,
    files: crate::media::Files,
    pending: HashSet<String>,
    bad_files: HashSet<String>,
    file_tx: mpsc::Sender<FileMsg>,
    file_rx: mpsc::Receiver<FileMsg>,
    files_dirty: bool,
    wave_peaks: std::collections::HashMap<String, (Arc<Vec<f32>>, f32)>,
    left_tab: u8,
    browser: Browser,
    snap_steps: u32,
    pl_drag: Option<PlDrag>,
    pl_menu: Option<PlMenu>,
    pl_org: Pos2,
    pl_rect: Option<Rect>,
    play_from: u32,
    #[cfg(not(target_os = "android"))]
    midi_conn: Option<crate::midi::input::Conn>,
    midi_tx: mpsc::Sender<crate::midi::Live>,
    midi_rx: mpsc::Receiver<crate::midi::Live>,
    midi_rec: bool,
    midi_held: std::collections::HashMap<u8, (u32, f64, f32)>,
    rec_track: Option<TrackRec>,
    settings: Settings,
    last_version: Instant,
    version_dirty: bool,
    /// masterizado en curso (y si se lanzó solo); su bandera para cancelarlo; cuándo relanzarlo; volumen final medido
    master_job: Option<(mpsc::Receiver<MasterRes>, bool)>,
    mast_cancel: Option<Arc<AtomicBool>>,
    mast_pendiente: Option<Instant>,
    /// (LUFS, ganancia del limitador) del último masterizado
    master_lufs: Option<(f32, f32)>,
    master_target: usize,
    /// mastering estilo BandLab: estilo, intensidad (0 ligero, 1 normal, 2 pesado), graves/medios/agudos
    /// y canción de referencia (nombre, sonoridad, balance de graves/medios/agudos)
    master_estilo: usize,
    master_int: f32,
    master_eq_on: bool,
    /// panel «Masterizando» abierto y escuchar sin master
    mast_panel: bool,
    sin_master: bool,
    /// en el teléfono: cuál de los dos paneles se abrió último (el otro se cierra)
    mic_antes_que_mast: bool,
    /// recortes del micrófono ya vistos y cuándo cambió por última vez (aviso de saturación)
    recortes_vistos: (u32, f64),
    master_eq3: [f32; 3],
    master_ref: Option<(String, f32, [f32; 3])>,
    scale_snap: bool,
    sampler_sens: f32,
    auto_drag: Option<usize>,
    lufs_hold: [f32; 3],
    tpeak_hold: f32,
    load_hold: f32,
    plug: PlugState,
    trash_rx: Option<mpsc::Receiver<Box<dyn crate::engine::Proc>>>,
    /// paso actual del tutorial de bienvenida
    tut: Option<usize>,
    tut_rects: std::collections::HashMap<&'static str, Rect>,
    tut_base: usize,
    tut_ok: bool,
    /// picos (cada 10 ms) de lo que se está grabando
    rec_peaks: Vec<f32>,
    rec_leido: usize,
    /// micrófono abierto solo para escucharse (sin grabar)
    mic_mon: Option<audio::Recorder>,
    /// panel del micrófono abierto abajo
    mic_panel: bool,
    /// historial reciente de la voz: (nota cantada, nota corregida) para el gráfico
    trazo: std::collections::VecDeque<(f32, f32)>,
    /// ancho de un compás en la playlist (zoom)
    pl_bw: f32,
    /// toques de «Tocar» para sacar el tempo
    toques: Vec<f64>,
    /// pads de batería y acordes
    pads: pads_view::Pads,
    /// permisos de Android ya pedidos; lista de micrófonos (y cuándo se leyó)
    permisos_pedidos: bool,
    /// cuándo se abrió la app
    inicio: Instant,
    /// última vez que se reconectó el audio; lista de salidas (se lee junto con las entradas)
    reconexion: Option<Instant>,
    salidas: Vec<String>,
    /// se pidió el permiso del micrófono y se espera la respuesta
    mic_esperando: Option<Instant>,
    entradas: Vec<String>,
    entradas_t: Option<Instant>,
    /// selector de audio del teléfono abierto (None dentro = lista de lugares)
    elegir_audio: Option<Option<PathBuf>>,
    elegir_cache: Option<(PathBuf, Vec<(String, PathBuf, bool)>)>,
    /// panel lateral de letras y notas
    letras_panel: bool,
    /// editor Retune abierto (y clip que se está analizando)
    retune: Option<Retune>,
    retune_cargando: Option<u32>,
    /// archivo del proyecto (None = todavía sin guardar)
    proj_path: Option<PathBuf>,
    /// hay cambios sin guardar
    cambios: bool,
    /// acción que espera la respuesta de «¿Guardar cambios?»
    pendiente: Option<Pend>,
    /// copia de rescate: hay una de un cierre inesperado (se ofrece recuperarla) y cuándo se escribió la última
    rescate: bool,
    ultimo_rescate: Instant,
    cerrar_ok: bool,
    titulo: String,
    last_auto: Instant,
    zoom_puesto: f32,
    ajuste_sec: usize,
    export_dir_inicial: PathBuf,
}

/// Acciones que pueden perder cambios sin guardar.
#[derive(Clone, Copy, PartialEq)]
enum Pend {
    Cerrar,
    Nuevo(usize),
    Abrir,
}

/// Prueba automática de la interfaz (se activa con --autotest).
pub struct AutoTest {
    pub frame: u32,
    pub log: Vec<String>,
}

impl StudioApp {
    pub fn new(cc: &eframe::CreationContext<'_>, plat: Platform) -> Self {
        setup_style(&cc.egui_ctx, plat.mobile);
        let _ = std::fs::create_dir_all(&plat.data_dir);
        let settings = StudioApp::load_settings(&plat.data_dir);
        // por defecto siempre empieza un proyecto nuevo; el último solo si así está en Ajustes
        let ultimo = if settings.abrir_ultimo {
            std::fs::read_to_string(plat.data_dir.join("autosave.json")).ok().and_then(|s| serde_json::from_str::<Project>(&s).ok()).filter(|p| p.version == 1 && !p.patterns.is_empty() && !p.channels.is_empty())
        } else {
            None
        };
        let p = ultimo.unwrap_or_else(|| StudioApp::proyecto_nuevo(&settings));
        let status = Arc::new(Status::new());
        let (mut audio, audio_err) = match audio::start(p.clone(), status.clone(), &settings.salida, settings.buffer).or_else(|_| audio::start(p.clone(), status.clone(), "", 0)) {
            Ok(a) => (Some(a), None),
            Err(e) => (None, Some(e)),
        };
        let trash_rx = audio.as_mut().and_then(|a| a.trash.take());
        let export_dir_inicial = plat.export_dir.clone();
        let mut plat = plat;
        if !settings.export_dir.is_empty() && std::path::Path::new(&settings.export_dir).is_dir() {
            plat.export_dir = PathBuf::from(&settings.export_dir);
        }
        let hay_rescate = plat.data_dir.join("recuperacion.json").exists();
        let (file_tx, file_rx) = mpsc::channel();
        let (midi_tx, midi_rx) = mpsc::channel();
        let logo = load_logo(&cc.egui_ctx);
        let sel_pat = p.patterns[0].id;
        let sel_ch = p.channels.iter().find(|c| c.inst.melodic()).unwrap_or(&p.channels[0]).id;
        let msg = match &audio_err {
            None if plat.mobile => "Listo.".to_string(),
            None => "Listo. Espacio: reproducir · Enter: PAT/SONG · F5/F6/F7/F9: ventanas.".to_string(),
            Some(e) => format!("Sin audio: {e}"),
        };
        let (tune_tx, tune_rx) = mpsc::channel();
        let mut app = StudioApp {
            p,
            hist: vec![],
            status,
            audio,
            audio_err,
            song_mode: false,
            metro: false,
            sel_pat,
            sel_ch,
            view: if plat.mobile { View::Playlist } else { View::Rack },
            show_playlist: true,
            tool: 0,
            note_len: 4,
            note_vel: 0.8,
            add_inst: Inst::Kick,
            open_set: HashSet::new(),
            roll_drag: None,
            step_drag: None,
            roll_x: 0.0,
            roll_center: true,
            msg,
            job: None,
            logo,
            plat,
            dirty: None,
            meters: vec![0.0; 64],
            mmeter: [0.0; 2],
            open_list: None,
            autotest: None,
            wins: vec![],
            eq_drag: None,
            spec: vec![],
            samples: Default::default(),
            raws: Default::default(),
            takes: Default::default(),
            rec: None,
            in_meter: 0.0,
            tune_tx,
            tune_rx,
            tune_busy: 0,
            maximized: false,
            gr_hold: [0.0; 2],
            ctx: Some(cc.egui_ctx.clone()),
            files: Default::default(),
            pending: HashSet::new(),
            bad_files: HashSet::new(),
            file_tx,
            file_rx,
            files_dirty: true,
            wave_peaks: Default::default(),
            left_tab: 0,
            browser: Browser::default(),
            snap_steps: 4,
            pl_drag: None,
            pl_menu: None,
            pl_org: Pos2::ZERO,
            pl_rect: None,
            play_from: 0,
            #[cfg(not(target_os = "android"))]
            midi_conn: None,
            midi_tx,
            midi_rx,
            midi_rec: false,
            midi_held: Default::default(),
            rec_track: None,
            settings,
            last_version: Instant::now(),
            version_dirty: false,
            master_job: None,
            mast_cancel: None,
            mast_pendiente: None,
            master_lufs: None,
            master_target: 0,
            master_estilo: 0,
            master_int: 1.0,
            master_eq_on: false,
            mast_panel: false,
            mic_antes_que_mast: false,
            recortes_vistos: (0, -100.0),
            sin_master: false,
            master_eq3: [0.0; 3],
            master_ref: None,
            scale_snap: false,
            sampler_sens: 0.5,
            auto_drag: None,
            lufs_hold: [-70.0; 3],
            tpeak_hold: 0.0,
            load_hold: 0.0,
            plug: PlugState::default(),
            trash_rx,
            tut: None,
            tut_rects: Default::default(),
            tut_base: 0,
            tut_ok: false,
            rec_peaks: Vec::new(),
            rec_leido: 0,
            mic_mon: None,
            mic_panel: false,
            trazo: std::collections::VecDeque::with_capacity(400),
            pl_bw: 72.0,
            permisos_pedidos: false,
            pads: Default::default(),
            toques: vec![],
            inicio: Instant::now(),
            reconexion: None,
            salidas: vec![],
            mic_esperando: None,
            entradas: vec![],
            entradas_t: None,
            elegir_audio: None,
            elegir_cache: None,
            letras_panel: false,
            retune: None,
            retune_cargando: None,
            proj_path: None,
            cambios: false,
            pendiente: None,
            rescate: hay_rescate,
            ultimo_rescate: Instant::now(),
            cerrar_ok: false,
            titulo: String::new(),
            last_auto: Instant::now(),
            zoom_puesto: -1.0,
            ajuste_sec: 0,
            export_dir_inicial,
        };
        if !app.settings.tutorial_done {
            app.tut_mostrar(0);
        }
        app.reload_takes();
        app.plugins_list_load();
        app.plugins_reload_all();
        #[cfg(not(target_os = "android"))]
        if !app.plat.mobile && !crate::midi::input::ports().is_empty() {
            app.midi_connect();
        }
        app
    }

    // ---------- estado y edición ----------
    fn cmd(&self, c: Cmd) {
        if let Some(a) = &self.audio {
            let _ = a.tx.send(c);
        }
    }
    fn send(&mut self) {
        self.cmd(Cmd::Project(Box::new(self.p.clone())));
        self.dirty = Some(Instant::now());
        self.files_dirty = true;
        self.version_dirty = true;
        self.cambios = true;
        if self.settings.escucharme {
            // por si cambió la tonalidad: el autotune en vivo la sigue
            self.escucha_sync();
        }
    }

    /// Proyecto nuevo según los ajustes (plantilla y tempo).
    fn proyecto_nuevo(s: &Settings) -> Project {
        let i = s.plantilla_nueva.min(crate::templates::TEMPLATES.len() - 1);
        let mut p = crate::templates::build(i);
        if i == 0 {
            p.bpm = s.bpm_nuevo.clamp(50.0, 200.0);
        }
        p
    }

    /// Hace una acción que podría perder cambios, preguntando antes si hace falta.
    fn pedir(&mut self, a: Pend) {
        if self.cambios && self.settings.preguntar && self.autotest.is_none() {
            self.pendiente = Some(a);
        } else {
            self.hacer(a);
        }
    }
    fn hacer(&mut self, a: Pend) {
        self.pendiente = None;
        match a {
            Pend::Cerrar => {
                self.cerrar_ok = true;
                if let Some(c) = &self.ctx {
                    c.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            Pend::Nuevo(i) => {
                self.stop();
                self.snap();
                self.p = if i == usize::MAX { StudioApp::proyecto_nuevo(&self.settings) } else { crate::templates::build(i) };
                self.fix_selection();
                self.send();
                self.reload_takes();
                self.plugins_reload_all();
                self.proj_path = None;
                self.cambios = false;
                self.msg = "Proyecto nuevo.".into();
            }
            Pend::Abrir => self.open_project_dialog(),
        }
    }

    /// Ventana «¿Guardar los cambios?».
    fn borrar_rescate(&mut self) {
        let _ = std::fs::remove_file(self.plat.data_dir.join("recuperacion.json"));
        let _ = std::fs::remove_file(self.plat.data_dir.join("recuperacion_ruta.txt"));
    }

    /// La última vez la app se cerró de golpe con cambios sin guardar: se ofrece recuperarlos.
    fn dialogo_rescate(&mut self, ctx: &egui::Context) {
        if !self.rescate || self.pendiente.is_some() {
            return;
        }
        let mut r: Option<bool> = None;
        egui::Area::new(egui::Id::new("resc_fondo")).order(egui::Order::Foreground).fixed_pos(ctx.content_rect().min).show(ctx, |ui| {
            let (rr, _) = ui.allocate_exact_size(ctx.content_rect().size(), Sense::click());
            ui.painter().rect_filled(rr, 0, Color32::from_black_alpha(150));
        });
        egui::Area::new(egui::Id::new("dlg_rescate")).order(egui::Order::Tooltip).anchor(Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
            Frame::new().fill(PANEL).stroke(Stroke::new(1.5, ACCENT)).corner_radius(10).inner_margin(Margin::same(18)).show(ui, |ui| {
                ui.set_width(420.0);
                ui.label(RichText::new("¿Recuperar tu trabajo?").strong().size(18.0).color(Color32::WHITE));
                ui.add_space(4.0);
                ui.label(RichText::new("La última vez NoName Studio se cerró de golpe y quedaron cambios sin guardar. Hay una copia de rescate.").size(13.5));
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new("Descartar").min_size(Vec2::new(110.0, 32.0))).clicked() {
                        r = Some(false);
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if boton_metal(ui, "Recuperar", Vec2::new(130.0, 36.0)).clicked() {
                            r = Some(true);
                        }
                    });
                });
            });
        });
        let Some(si) = r else { return };
        self.rescate = false;
        if si {
            let ruta = self.plat.data_dir.join("recuperacion.json");
            match std::fs::read_to_string(&ruta).ok().and_then(|s| serde_json::from_str::<Project>(&s).ok()) {
                Some(p) if !p.patterns.is_empty() && !p.channels.is_empty() => {
                    let archivo = std::fs::read_to_string(self.plat.data_dir.join("recuperacion_ruta.txt")).unwrap_or_default();
                    self.hist.clear();
                    self.p = p;
                    self.fix_selection();
                    self.send();
                    self.reload_takes();
                    self.plugins_reload_all();
                    self.proj_path = if archivo.trim().is_empty() { None } else { Some(PathBuf::from(archivo.trim())) };
                    // queda como cambios sin guardar: Guardar lo escribe en su archivo
                    self.cambios = true;
                    self.ultimo_rescate = Instant::now();
                    self.msg = format!("Trabajo recuperado: «{}». Guárdalo para no perderlo.", self.p.name);
                    return;
                }
                _ => self.msg = "La copia de rescate estaba dañada.".into(),
            }
        }
        self.borrar_rescate();
    }

    fn dialogo_guardar(&mut self, ctx: &egui::Context) {
        let Some(a) = self.pendiente else { return };
        let que = match a {
            Pend::Cerrar => "antes de cerrar",
            Pend::Nuevo(_) => "antes de empezar otro proyecto",
            Pend::Abrir => "antes de abrir otro proyecto",
        };
        let mut r: Option<u8> = None;
        egui::Area::new(egui::Id::new("dlg_fondo")).order(egui::Order::Foreground).fixed_pos(ctx.content_rect().min).show(ctx, |ui| {
            let (rr, _) = ui.allocate_exact_size(ctx.content_rect().size(), Sense::click());
            ui.painter().rect_filled(rr, 0, Color32::from_black_alpha(150));
        });
        egui::Area::new(egui::Id::new("dlg_guardar")).order(egui::Order::Tooltip).anchor(Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
            Frame::new().fill(PANEL).stroke(Stroke::new(1.5, ACCENT)).corner_radius(10).inner_margin(Margin::same(18)).show(ui, |ui| {
                ui.set_width(400.0);
                ui.label(RichText::new("¿Guardar los cambios?").strong().size(18.0).color(Color32::WHITE));
                ui.add_space(4.0);
                ui.label(RichText::new(format!("«{}» tiene cambios sin guardar. ¿Quieres guardarlos {que}?", self.p.name)).size(13.5));
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    if ui.button("Cancelar").clicked() {
                        r = Some(0);
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if boton_metal(ui, "Guardar", Vec2::new(130.0, 36.0)).clicked() {
                            r = Some(1);
                        }
                        if ui.add(egui::Button::new("No guardar").min_size(Vec2::new(110.0, 32.0))).clicked() {
                            r = Some(2);
                        }
                    });
                });
            });
        });
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            r = Some(0);
        }
        match r {
            Some(0) => self.pendiente = None,
            Some(1) => {
                if self.guardar(false) {
                    self.hacer(a);
                }
            }
            Some(2) => self.hacer(a),
            _ => {}
        }
    }
    fn snap(&mut self) {
        self.hist.push(self.p.clone());
        if self.hist.len() > 150 {
            self.hist.remove(0);
        }
    }
    fn track(&mut self, r: &Response) {
        if r.drag_started() || (r.changed() && !r.dragged()) {
            self.snap();
        }
    }
    fn undo(&mut self) {
        if let Some(p) = self.hist.pop() {
            self.p = p;
            self.fix_selection();
            self.send();
            self.reload_takes();
            self.plugins_reload_all();
            self.msg = "Deshecho.".into();
        } else {
            self.msg = "Nada que deshacer.".into();
        }
    }
    fn fix_selection(&mut self) {
        if self.p.pattern(self.sel_pat).is_none() {
            self.sel_pat = self.p.patterns[0].id;
        }
        if self.p.channel(self.sel_ch).is_none() {
            self.sel_ch = self.p.channels[0].id;
        }
    }
    fn playing(&self) -> bool {
        self.status.playing.load(Ordering::Relaxed)
    }
    fn mode(&self) -> Mode {
        if self.song_mode { Mode::Song } else { Mode::Pattern(self.sel_pat) }
    }
    fn play(&mut self, from: u32) {
        if self.audio.is_none() {
            self.msg = format!("No hay salida de audio: {}", self.audio_err.clone().unwrap_or_default());
            return;
        }
        self.cmd(Cmd::Play { mode: self.mode(), from_step: from });
        self.status.playing.store(true, Ordering::Relaxed);
        self.msg = if self.song_mode { "Reproduciendo canción…".into() } else { format!("Reproduciendo patrón «{}»…", self.cur_pat().name) };
    }
    fn stop(&mut self) {
        if self.rec_track.is_some() {
            self.rec_track_stop();
            return;
        }
        self.cmd(Cmd::Stop);
        self.status.playing.store(false, Ordering::Relaxed);
        self.msg = "Detenido.".into();
    }
    fn toggle_play(&mut self) {
        if self.rec_track.is_some() {
            self.rec_track_stop();
        } else if self.playing() {
            self.stop()
        } else {
            let from = if self.song_mode { self.play_from } else { 0 };
            self.play(from)
        }
    }
    fn set_song_mode(&mut self, song: bool) {
        self.song_mode = song;
        if self.playing() {
            self.play(0);
        }
    }
    fn select_pat(&mut self, id: u32) {
        if self.sel_pat != id {
            self.sel_pat = id;
            self.roll_center = true;
            if !self.song_mode {
                self.cmd(Cmd::SetMode(Mode::Pattern(id)));
                if self.playing() {
                    self.msg = format!("Reproduciendo patrón «{}»…", self.cur_pat().name);
                }
            }
        }
    }
    fn cur_pat(&self) -> &Pattern {
        self.p.pattern(self.sel_pat).unwrap_or(&self.p.patterns[0])
    }
    fn preview(&self, ch: u32, m: u8, v: f32) {
        self.cmd(Cmd::Preview { ch, m, v });
    }
    fn melodic(&self) -> Vec<(u32, String)> {
        self.p.channels.iter().filter(|c| c.inst.melodic()).map(|c| (c.id, c.name.clone())).collect()
    }
    fn roll_ch(&mut self) -> Option<u32> {
        match self.p.channel(self.sel_ch) {
            Some(c) if c.inst.melodic() => Some(c.id),
            _ => {
                let id = self.p.channels.iter().find(|c| c.inst.melodic())?.id;
                self.sel_ch = id;
                Some(id)
            }
        }
    }
    /// Paso local del patrón seleccionado bajo el cabezal (o None).
    fn local_step(&self) -> Option<f64> {
        if !self.playing() {
            return None;
        }
        let pos = self.status.pos();
        let p = self.cur_pat();
        let len = p.len_steps() as f64;
        if !self.song_mode {
            return Some(pos.rem_euclid(len));
        }
        self.p.playlist.iter().filter(|c| c.pat == p.id).map(|c| pos - (c.bar * STEPS_PER_BAR) as f64).find(|l| *l >= 0.0 && *l < len)
    }

    // ---------- archivos ----------
    /// Guardado automático: solo si está activado en Ajustes.
    fn autosave(&mut self) {
        let min = self.settings.autoguardar_min;
        let quieto = self.dirty.is_none_or(|t| t.elapsed() > Duration::from_millis(1500));
        // copia de rescate escondida (no toca tus archivos): solo sirve si la app se cierra de golpe
        if !self.rescate {
            let ruta = self.plat.data_dir.join("recuperacion.json");
            if self.cambios && quieto && self.ultimo_rescate.elapsed() > Duration::from_secs(40) {
                self.ultimo_rescate = Instant::now();
                if let Ok(s) = serde_json::to_string(&self.p) {
                    let _ = std::fs::write(&ruta, s);
                    let _ = std::fs::write(self.plat.data_dir.join("recuperacion_ruta.txt"), self.proj_path.as_ref().map(|p| p.to_string_lossy().to_string()).unwrap_or_default());
                }
            } else if !self.cambios && self.ultimo_rescate.elapsed() > Duration::from_secs(2) {
                // ya está guardado (o se descartó): la copia de rescate sobra (se revisa cada 2 s)
                self.ultimo_rescate = Instant::now();
                if ruta.exists() {
                    self.borrar_rescate();
                }
            }
        }
        if min == 0 || !self.cambios || !quieto || self.last_auto.elapsed() < Duration::from_secs(min as u64 * 60) {
            return;
        }
        self.last_auto = Instant::now();
        self.dirty = None;
        if self.proj_path.is_some() {
            self.guardar(false);
        } else {
            self.plugins_store_state();
            if let Ok(s) = serde_json::to_string(&self.p) {
                let _ = std::fs::write(self.plat.data_dir.join("recuperacion.json"), s);
            }
        }
    }
    fn save_project(&mut self) {
        self.guardar(false);
    }
    /// Guarda en el archivo del proyecto (o pide dónde si es nuevo o `como`). Devuelve si se guardó.
    fn guardar(&mut self, como: bool) -> bool {
        self.plugins_store_state();
        let json = match serde_json::to_string_pretty(&self.p) {
            Ok(s) => s,
            Err(e) => {
                self.msg = format!("Error al guardar: {e}");
                return false;
            }
        };
        if let (false, Some(path)) = (como, self.proj_path.clone()) {
            return match std::fs::write(&path, &json) {
                Ok(_) => {
                    self.cambios = false;
                    self.msg = format!("Guardado: {}", path.display());
                    true
                }
                Err(e) => {
                    self.msg = format!("No se pudo guardar: {e}");
                    false
                }
            };
        }
        #[cfg(not(target_os = "android"))]
        let path = rfd::FileDialog::new()
            .set_title("Guardar proyecto")
            .add_filter("Proyecto NoName Studio", &["nns.json", "json"])
            .set_directory(&self.plat.export_dir)
            .set_file_name(format!("{}.nns.json", export::safe_name(&self.p.name)))
            .save_file();
        #[cfg(target_os = "android")]
        let path = {
            let _ = std::fs::create_dir_all(&self.plat.export_dir);
            Some(self.plat.export_dir.join(format!("{}.nns.json", export::safe_name(&self.p.name))))
        };
        match path {
            Some(path) => match std::fs::write(&path, json) {
                Ok(_) => {
                    self.msg = format!("Proyecto guardado: {}", path.display());
                    self.proj_path = Some(path);
                    self.cambios = false;
                    true
                }
                Err(e) => {
                    self.msg = format!("No se pudo guardar: {e}");
                    false
                }
            },
            None => false,
        }
    }
    fn open_project_dialog(&mut self) {
        #[cfg(not(target_os = "android"))]
        {
            if let Some(path) = rfd::FileDialog::new().set_title("Abrir proyecto").add_filter("Proyecto NoName Studio", &["json"]).set_directory(&self.plat.export_dir).pick_file() {
                self.open_path(path);
            }
        }
        #[cfg(target_os = "android")]
        {
            let mut v = vec![];
            for d in [&self.plat.export_dir, &self.plat.data_dir] {
                if let Ok(rd) = std::fs::read_dir(d) {
                    for e in rd.flatten() {
                        let p = e.path();
                        if p.to_string_lossy().ends_with(".json") && !p.ends_with("autosave.json") {
                            v.push(p);
                        }
                    }
                }
            }
            self.open_list = Some(v);
        }
    }
    fn open_path(&mut self, path: PathBuf) {
        match std::fs::read_to_string(&path).ok().and_then(|s| serde_json::from_str::<Project>(&s).ok()) {
            Some(p) if !p.patterns.is_empty() && !p.channels.is_empty() => {
                // otro archivo: el historial del proyecto anterior no se puede deshacer sobre este
                // (si no, Guardar escribiría el proyecto viejo encima del archivo recién abierto)
                self.hist.clear();
                self.p = p;
                self.fix_selection();
                self.send();
                self.reload_takes();
                self.plugins_reload_all();
                self.proj_path = Some(path.clone());
                self.cambios = false;
                self.msg = format!("Proyecto «{}» abierto.", self.p.name);
            }
            _ => self.msg = "Ese archivo no es un proyecto de NoName Studio.".into(),
        }
    }
    fn start_export(&mut self, fmt: Format) {
        if self.job.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let progress = Arc::new(AtomicU32::new(0));
        let cancel = Arc::new(AtomicBool::new(false));
        self.plugins_store_state();
        let mut bank = self.bank();
        crate::media::ensure(&self.p, &self.plat.data_dir, &mut bank.files);
        let mut setup = self.export_setup();
        let (p, dir, pr, ca) = (self.p.clone(), self.plat.export_dir.clone(), progress.clone(), cancel.clone());
        let opts = export::Opts { sr: self.settings.sr_export, kbps: self.settings.mp3_kbps, bits: self.settings.wav_bits, norm: self.settings.normalizar };
        std::thread::spawn(move || {
            let _ = tx.send(export::export_full(&p, &bank, &dir, fmt, &opts, &pr, &ca, &mut setup));
        });
        self.msg = if fmt == Format::Stems { format!("Renderizando stems ({} canales)…", self.p.channels.len()) } else { format!("Renderizando canción ({} compases)…", self.p.song_bars()) };
        self.job = Some(ExportJob { progress, cancel, rx, fmt, started: Instant::now() });
    }
    fn poll_export(&mut self) {
        let Some(j) = &self.job else { return };
        match j.rx.try_recv() {
            Ok(r) => {
                let secs = j.started.elapsed().as_secs_f32();
                self.msg = match r {
                    Ok(d) if j.fmt == Format::Stems => format!("Stems exportados: {} archivos en {} · {:.1} s", d.files, d.path.display(), secs),
                    Ok(d) => format!("Exportado: {} · {:.1} s · {:.1} LUFS · pico {:.1} dBFS · {:.1} s", d.path.display(), d.secs, d.lufs, d.peak_db, secs),
                    Err(e) => format!("Error al exportar: {e}"),
                };
                if let Some(t) = &mut self.autotest {
                    t.log.push(self.msg.clone());
                }
                self.job = None;
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(_) => {
                self.msg = "La exportación terminó sin respuesta.".into();
                self.job = None;
            }
        }
    }

    // ---------- teclado ----------
    fn shortcuts(&mut self, ctx: &egui::Context) {
        let typing = ctx.memory(|m| m.focused().is_some());
        let rec = ctx.input(|i| i.modifiers.command && i.key_pressed(Key::R));
        if rec && !typing {
            if self.rec_track.is_some() { self.rec_track_stop() } else { self.rec_track_start() }
        }
        let (space, enter, undo, save, f5, f6, f7, f9, m) = ctx.input(|i| {
            (
                i.key_pressed(Key::Space),
                i.key_pressed(Key::Enter),
                i.modifiers.command && i.key_pressed(Key::Z),
                i.modifiers.command && i.key_pressed(Key::S),
                i.key_pressed(Key::F5),
                i.key_pressed(Key::F6),
                i.key_pressed(Key::F7),
                i.key_pressed(Key::F9),
                !i.modifiers.command && i.key_pressed(Key::M) && !i.modifiers.shift,
            )
        });
        if f5 {
            self.show_playlist = !self.show_playlist;
        }
        if f6 {
            self.view = View::Rack;
        }
        if f7 {
            self.view = View::Roll;
            self.roll_center = true;
        }
        if f9 {
            self.view = View::Mixer;
        }
        if ctx.input(|i| i.key_pressed(Key::F8)) {
            self.view = View::Pads;
        }
        if typing {
            return;
        }
        if space {
            self.toggle_play();
        }
        if enter {
            self.set_song_mode(!self.song_mode);
        }
        if undo {
            self.undo();
        }
        if save {
            self.save_project();
        }
        if m {
            self.metro = !self.metro;
            self.cmd(Cmd::Metronome(self.metro));
        }
    }

    // ---------- barra superior ----------
    fn top_bar(&mut self, ui: &mut Ui, compact: bool) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = if compact { 6.0 } else { 8.0 };
            let ancho = ui.available_width();
            // la derecha tiene su lugar fijo; la izquierda usa lo que queda y nunca se monta encima
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                self.barra_der(ui, !compact && ancho > 1800.0, compact);
                ui.add_space(if compact { 6.0 } else { 14.0 });
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    let caja = ui.max_rect();
                    ui.set_clip_rect(caja.intersect(ui.clip_rect()));
                    if compact {
                        // en el teléfono todo en una fila: se desliza con el dedo si no entra
                        egui::ScrollArea::horizontal()
                            .id_salt("barra_izq")
                            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                            .show(ui, |ui| {
                                ui.horizontal(|ui| self.barra_izq(ui, true, 0.0));
                            });
                    } else {
                        self.barra_izq(ui, false, ancho);
                    }
                });
            });
        });
    }

    /// Barra de arriba, izquierda: logo, transporte, micrófono, letras, PAT/SONG, tempo, swing y nombre.
    fn barra_izq(&mut self, ui: &mut Ui, compact: bool, ancho: f32) {
        // lugar que de verdad queda para esta parte (lo de la derecha ya está puesto)
        let _ = ancho;
        let libre = if compact { 0.0 } else { ui.available_width() };
        // lo básico (transporte, tempo, swing y nombre) ocupa unos 1040 px; el título y los rótulos solo si caben
        let extra = libre - 1040.0;
        // el nombre de la app siempre se ve en la PC (en el teléfono, solo el logo)
        let con_titulo = !compact;
        let con_rotulos = !compact && extra > 235.0;
        if let Some(t) = &self.logo {
            ui.add(egui::Image::new(t).fit_to_exact_size(Vec2::splat(30.0))).on_hover_text(concat!("NoName Studio ", env!("CARGO_PKG_VERSION")));
        }
        // en el teléfono el nombre del proyecto va al principio, siempre a la vista
        if compact {
            self.campo_nombre(ui, 120.0);
        }
        if con_titulo {
            ui.label(RichText::new("NoName Studio").strong().size(15.0).color(Color32::WHITE));
        }
        ui.add_space(6.0);
        let playing = self.playing();
        let r = icon_button(ui, if playing { Ic::Pause } else { Ic::Play }, Vec2::new(40.0, 28.0), false, if playing { Some(ACCENT) } else { None });
        if playing {
            let k = (ui.input(|i| i.time) as f32 * 3.0).sin() * 0.5 + 0.5;
            ui.painter().rect_stroke(r.rect.expand(2.0 + 2.0 * k), 11, Stroke::new(1.5, ACCENT.gamma_multiply(0.25 + 0.35 * (1.0 - k))), StrokeKind::Outside);
        }
        self.tut_mark("play", r.rect);
        let play_rect = r.rect;
        if r.on_hover_text("Reproducir / pausar (Espacio)").clicked() {
            self.toggle_play();
        }
        if icon_button(ui, Ic::Stop, Vec2::new(40.0, 28.0), false, None).on_hover_text("Detener").clicked() {
            self.stop();
        }
        let recording = self.rec_track.is_some() || self.rec.is_some();
        if icon_button(ui, Ic::Rec, Vec2::new(36.0, 28.0), recording, if recording { Some(RED) } else { None }).on_hover_text("Grabar audio en la pista seleccionada (Ctrl+R). Con Bucle: una toma por vuelta. Con Punch: solo la región.").clicked() {
            if self.rec_track.is_some() {
                self.rec_track_stop();
            } else if self.rec.is_some() {
                self.rec_stop();
            } else {
                self.rec_track_start();
            }
        }
        self.rec_indicator(ui);
        self.micro_boton(ui);
        if ui.add(egui::Button::selectable(self.letras_panel, RichText::new("Letras").size(12.5)).min_size(Vec2::new(0.0, 28.0))).on_hover_text("Escribe la letra y notas de la canción (se guardan en el proyecto)").clicked() {
            self.letras_panel = !self.letras_panel;
        }
        if ui.add(egui::Button::selectable(!self.song_mode, "PAT").min_size(Vec2::new(44.0, 28.0))).on_hover_text("Patrón en bucle").clicked() {
            self.set_song_mode(false);
        }
        let r = ui.add(egui::Button::selectable(self.song_mode, "SONG").min_size(Vec2::new(50.0, 28.0)));
        self.tut_mark("song", r.rect.union(play_rect));
        if r.on_hover_text("Canción completa (playlist)").clicked() {
            self.set_song_mode(true);
        }
        // metrónomo: el botón lo prende y apaga; la flechita abre sus ajustes
        ui.spacing_mut().item_spacing.x = 0.0;
        let rm = icon_button(ui, Ic::Metro, Vec2::new(34.0, 28.0), self.metro, None).on_hover_text("Metrónomo (M)");
        if rm.clicked() {
            self.metro = !self.metro;
            self.cmd(Cmd::Metronome(self.metro));
        }
        let (rr, rf) = ui.allocate_exact_size(Vec2::new(20.0, 28.0), Sense::click());
        let abierto = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&rf));
        let cf = if rf.hovered() || abierto { Color32::WHITE } else { DIM };
        let c = rr.center();
        let (a1, a2) = if abierto { (-3.0, 2.0) } else { (2.0, -3.0) };
        ui.painter().add(egui::Shape::line(vec![c + Vec2::new(-4.5, -a1 - 0.5), c + Vec2::new(0.0, -a2 - 0.5), c + Vec2::new(4.5, -a1 - 0.5)], Stroke::new(1.6, cf)));
        let rf = rf.on_hover_text("Ajustes del metrónomo");
        ui.spacing_mut().item_spacing.x = 6.0;
        egui::Popup::menu(&rf).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| self.menu_metronomo(ui));
        if rm.secondary_clicked() || rm.long_touched() {
            egui::Popup::open_id(ui.ctx(), egui::Popup::default_response_id(&rf));
        }
        // LCD
        let s = if playing { self.status.pos().max(0.0) as u64 } else { 0 };
        let lcd = format!("{:03}:{}:{:02}", s / 16 + 1, (s / 4) % 4 + 1, s % 4 + 1);
        Frame::new().fill(Color32::from_gray(12)).corner_radius(5).inner_margin(Margin::symmetric(10, 4)).show(ui, |ui| {
            ui.label(RichText::new(lcd).monospace().size(16.0).color(ACCENT2));
        });
        if con_rotulos {
            ui.label(RichText::new("TEMPO").small().color(DIM));
        }
        let mut bpm = self.p.bpm;
        let r = ui.add(egui::DragValue::new(&mut bpm).range(50.0..=200.0).speed(0.25).fixed_decimals(0).suffix(" bpm"));
        self.track(&r);
        if r.changed() {
            self.p.bpm = bpm.round();
            self.send();
        }
        if con_rotulos {
            ui.label(RichText::new("SWING").small().color(DIM));
        }
        let mut sw = self.p.swing;
        ui.spacing_mut().slider_width = 64.0;
        let r = ui.add(egui::Slider::new(&mut sw, 0.0..=0.5).custom_formatter(|v, _| format!("{:.0}%", v * 100.0))).on_hover_text("Swing: atrasa un poco los golpes pares para que suene con más groove");
        self.track(&r);
        if r.changed() {
            self.p.swing = sw;
            self.send();
        }
        if ui.add(egui::Button::selectable(self.p.swing8, if self.p.swing8 { "1/8" } else { "1/16" }).min_size(Vec2::new(34.0, 22.0))).on_hover_text("Swing sobre semicorcheas (1/16) o corcheas (1/8)").clicked() {
            self.snap();
            self.p.swing8 = !self.p.swing8;
            self.send();
        }
        if !compact {
            let w = (ui.available_width() - 6.0).min(200.0);
            if w >= 60.0 {
                self.campo_nombre(ui, w);
            }
        }
    }

    /// Cuadro con el nombre del proyecto.
    fn campo_nombre(&mut self, ui: &mut Ui, ancho: f32) {
        let mut name = self.p.name.clone();
        let r = ui.add(egui::TextEdit::singleline(&mut name).desired_width(ancho).hint_text("Nombre del proyecto"));
        if r.changed() {
            if r.gained_focus() || !self.hist.last().is_some_and(|h| h.name != self.p.name) {
                self.snap();
            }
            self.p.name = name;
            self.send();
        }
    }

    /// Barra de arriba, derecha (de derecha a izquierda): Exportar, sonoridad, CPU, ajustes, Archivo y deshacer.
    /// Menú del metrónomo (flechita junto a su botón): tempo con «Tocar», sonido, volumen y precuenta.
    fn menu_metronomo(&mut self, ui: &mut Ui) {
        ui.set_width(270.0);
        let mut guardar = false;
        let titulo = |ui: &mut Ui, t: &str| {
            ui.add_space(4.0);
            ui.label(RichText::new(t).strong().size(15.0));
            ui.add_space(2.0);
        };
        // ---------- tempo
        titulo(ui, "Tempo");
        Frame::new().fill(PANEL2).corner_radius(10).inner_margin(Margin::symmetric(8, 6)).show(ui, |ui| {
            ui.horizontal(|ui| {
                let ancho = ui.available_width();
                let lado = Vec2::new(44.0, 44.0);
                if ui.add(egui::Button::new(RichText::new("−").size(18.0)).frame(false).min_size(lado)).clicked() {
                    self.snap();
                    self.p.bpm = (self.p.bpm.round() - 1.0).max(50.0);
                    self.send();
                }
                // centro: el número y «Tocar» (toca al ritmo para sacar el tempo)
                let (r, resp) = ui.allocate_exact_size(Vec2::new(ancho - lado.x * 2.0 - 16.0, 44.0), Sense::click());
                let t = ui.input(|i| i.time);
                if resp.clicked() {
                    if self.toques.last().is_some_and(|u| t - u > 2.0) {
                        self.toques.clear();
                    }
                    self.toques.push(t);
                    if self.toques.len() > 6 {
                        self.toques.remove(0);
                    }
                    if self.toques.len() >= 3 {
                        let n = self.toques.len();
                        let prom = (self.toques[n - 1] - self.toques[0]) / (n - 1) as f64;
                        let bpm = (60.0 / prom).round().clamp(50.0, 200.0) as f32;
                        if bpm != self.p.bpm {
                            self.p.bpm = bpm;
                            self.cambios = true;
                            self.send();
                        }
                    }
                }
                let brillo = ui.ctx().animate_bool_with_time(resp.id.with("toque"), resp.is_pointer_button_down_on(), 0.1);
                let p = ui.painter();
                if resp.hovered() || brillo > 0.0 {
                    p.rect_filled(r, 8, Color32::from_white_alpha((10.0 + 30.0 * brillo) as u8));
                }
                p.text(r.center() - Vec2::new(0.0, 7.0), Align2::CENTER_CENTER, format!("{:.0}", self.p.bpm), FontId::proportional(20.0), Color32::WHITE);
                let pista = if self.toques.len() == 1 || self.toques.len() == 2 { "sigue tocando…" } else { "Tocar" };
                p.text(r.center() + Vec2::new(0.0, 13.0), Align2::CENTER_CENTER, pista, FontId::proportional(11.0), DIM);
                resp.on_hover_text("Toca aquí al ritmo de la canción (3 veces o más) y el tempo se ajusta solo");
                if ui.add(egui::Button::new(RichText::new("+").size(18.0)).frame(false).min_size(lado)).clicked() {
                    self.snap();
                    self.p.bpm = (self.p.bpm.round() + 1.0).min(200.0);
                    self.send();
                }
            });
        });
        // ---------- sonido
        titulo(ui, "Sonido del metrónomo");
        ui.horizontal(|ui| {
            const SONIDOS: [&str; 4] = ["Clásico", "Madera", "Electrónico", "Suave"];
            let actual = SONIDOS[(self.settings.metro_sonido as usize).min(3)];
            egui::ComboBox::from_id_salt("metro_sonido").width(200.0).selected_text(actual).show_ui(ui, |ui| {
                for (k, n) in SONIDOS.iter().enumerate() {
                    if ui.selectable_label(self.settings.metro_sonido as usize == k, *n).clicked() {
                        self.settings.metro_sonido = k as u8;
                        self.status.metro_sonido.store(k as u32, Ordering::Relaxed);
                        self.cmd(Cmd::ClicPrueba);
                        guardar = true;
                    }
                }
            });
            if icon_button(ui, Ic::Play, Vec2::new(30.0, 30.0), false, None).on_hover_text("Escuchar").clicked() {
                self.cmd(Cmd::ClicPrueba);
            }
        });
        // ---------- volumen (en dB)
        titulo(ui, "Volumen del metrónomo");
        ui.horizontal(|ui| {
            ui.spacing_mut().slider_width = 170.0;
            let mut v = self.settings.metro_vol;
            let r = ui.add(egui::Slider::new(&mut v, 0.0..=1.5).show_value(false));
            // doble clic: vuelve a 0 dB
            if r.double_clicked() {
                v = 1.0;
            }
            if v != self.settings.metro_vol {
                self.settings.metro_vol = v;
                guardar = true;
            }
            if r.drag_stopped() {
                self.cmd(Cmd::ClicPrueba);
            }
            let db = 20.0 * self.settings.metro_vol.max(1e-4).log10();
            ui.label(RichText::new(if self.settings.metro_vol < 0.01 { "−∞".to_string() } else { format!("{db:+.1} dB") }).monospace().small());
        });
        // ---------- precuenta
        titulo(ui, "Precuenta al grabar");
        Frame::new().fill(PANEL2).corner_radius(99).inner_margin(Margin::same(3)).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                let w = (ui.available_width() - 6.0) / 3.0;
                for (k, n) in ["No", "1 compás", "2 compases"].iter().enumerate() {
                    let sel = self.settings.precuenta as usize == k;
                    let b = egui::Button::new(RichText::new(*n).color(if sel { Color32::from_gray(15) } else { DIM }))
                        .fill(if sel { ACCENT } else { Color32::TRANSPARENT })
                        .corner_radius(99)
                        .min_size(Vec2::new(w, 30.0));
                    if ui.add(b).clicked() {
                        self.settings.precuenta = k as u8;
                        guardar = true;
                    }
                }
            });
        });
        // ---------- más
        ui.add_space(6.0);
        ui.collapsing("Más opciones", |ui| {
            ui.horizontal(|ui| {
                for (k, n) in ["Negras", "Corcheas"].iter().enumerate() {
                    if ui.add(egui::Button::selectable(self.settings.metro_sub as usize == k, *n)).clicked() {
                        self.settings.metro_sub = k as u8;
                        guardar = true;
                    }
                }
            });
            guardar |= ui.checkbox(&mut self.settings.metro_acento, "Primer tiempo más fuerte").changed();
            guardar |= ui.checkbox(&mut self.settings.metro_grabar, "Prender al grabar").changed();
        });
        if guardar {
            self.save_settings();
        }
    }

    fn barra_der(&mut self, ui: &mut Ui, cpu: bool, compact: bool) {
        let exporting = self.job.as_ref().map(|j| (j.progress.load(Ordering::Relaxed), j.fmt));
        match exporting {
            Some((pr, f)) => {
                let label = format!("Exportando {}%{}", pr / 10, match f { Format::Wav => " (WAV)", Format::Stems => " (pistas)", _ => "" });
                let b = egui::Button::new(RichText::new(label).strong().color(Color32::from_gray(21))).fill(ACCENT).min_size(Vec2::new(124.0, 30.0));
                let r = ui.add_enabled(false, b);
                self.tut_mark("export", r.rect);
            }
            None => {
                // botón de metal líquido; el clic abre las opciones de exportar
                let r = boton_metal(ui, "Exportar", if compact { Vec2::new(96.0, 32.0) } else { Vec2::new(124.0, 34.0) });
                egui::Popup::menu(&r).show(|ui| desplazable(ui, |ui| self.export_menu(ui)));
                self.tut_mark("export", r.rect);
            }
        }
        // en el teléfono la sonoridad y el mastering están en el menú Archivo
        self.pildora_master(ui, compact);
        if !compact {
            let lufs = self.lufs_text();
            if ui.add(egui::Button::new(RichText::new(lufs).monospace().size(12.0).color(ACCENT2)).min_size(Vec2::new(84.0, 28.0))).on_hover_text("Sonoridad (corto plazo). Clic: NN Master y mastering con un clic").clicked() {
                self.alternar_win(Win::Master);
            }
        }
        if icon_button(ui, Ic::Gear, Vec2::new(34.0, 28.0), false, None).on_hover_text("Configuración").clicked() {
            self.alternar_win(Win::Settings);
        }
        ui.menu_button("Archivo", |ui| desplazable(ui, |ui| {
            if ui.button("Nuevo proyecto").clicked() {
                ui.close();
                self.pedir(Pend::Nuevo(usize::MAX));
            }
            if ui.button("Abrir proyecto…").clicked() {
                ui.close();
                self.pedir(Pend::Abrir);
            }
            if ui.button("Guardar  (Ctrl+S)").clicked() {
                ui.close();
                self.guardar(false);
            }
            if ui.button("Guardar como…").clicked() {
                ui.close();
                self.guardar(true);
            }
            ui.separator();
            if ui.add_enabled(self.job.is_none(), egui::Button::new(format!("Exportar WAV ({} bits)", self.settings.wav_bits))).clicked() {
                ui.close();
                self.start_export(Format::Wav);
            }
            if ui.button("Sonoridad y mastering…").clicked() {
                ui.close();
                self.open_win(Win::Master);
            }
            if ui.button("Cargar beat de ejemplo").clicked() {
                ui.close();
                self.snap();
                self.p = Project::demo();
                self.fix_selection();
                self.send();
                self.reload_takes();
                self.plugins_reload_all();
                self.msg = "Beat de ejemplo cargado (Deshacer para volver).".into();
            }
            self.file_menu_extra(ui);
            ui.separator();
            ui.label(RichText::new(format!("Exporta en: {}", self.plat.export_dir.display())).small().color(DIM));
        }));
        if icon_button(ui, Ic::Undo, Vec2::new(36.0, 28.0), false, None).on_hover_text("Deshacer (Ctrl+Z)").clicked() {
            self.undo();
        }
    }

    /// Opciones del botón «Exportar».
    fn export_menu(&mut self, ui: &mut Ui) {
        ui.set_min_width(290.0);
        ui.set_max_width(290.0);
        let mut opcion = |ui: &mut Ui, titulo: &str, detalle: &str| -> bool {
            let r = ui.add(egui::Button::new(RichText::new(titulo).strong()).min_size(Vec2::new(ui.available_width(), 26.0)));
            ui.label(RichText::new(detalle).small().color(DIM));
            ui.add_space(4.0);
            r.clicked()
        };
        ui.label(RichText::new("GUARDAR LA CANCIÓN").small().color(DIM));
        let mp3 = format!("Archivo liviano para compartir o subir ({} kbps).", self.settings.mp3_kbps);
        if opcion(ui, "Canción en MP3", &mp3) {
            ui.close();
            self.start_export(Format::Mp3);
        }
        if opcion(ui, "Canción en WAV", "Calidad completa, sin compresión.") {
            ui.close();
            self.start_export(Format::Wav);
        }
        if opcion(ui, "Pistas separadas", "Un WAV por cada canal, para mezclar en otro programa.") {
            ui.close();
            self.start_export(Format::Stems);
        }
        if opcion(ui, "Patrón actual en MIDI", "Solo las notas, para usarlas en otro programa.") {
            ui.close();
            self.export_midi();
        }
        ui.separator();
        if opcion(ui, "Masterizar antes de exportar…", "Deja la canción al volumen de las plataformas.") {
            ui.close();
            self.open_win(Win::Master);
        }
        let carpeta = self.plat.export_dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let carpeta = if carpeta.eq_ignore_ascii_case("Desktop") { "tu Escritorio".to_string() } else if carpeta.eq_ignore_ascii_case("Music") { "tu carpeta Música".to_string() } else { format!("la carpeta «{carpeta}»") };
        ui.label(RichText::new(format!("Se guarda en {carpeta}.")).small().color(DIM)).on_hover_text(self.plat.export_dir.display().to_string());
    }

    // ---------- lista de patrones ----------
    fn pattern_list(&mut self, ui: &mut Ui) {
        ui.label(RichText::new("PATRONES").small().color(DIM));
        let mut clicked = None;
        for p in &self.p.patterns {
            let sel = p.id == self.sel_pat;
            let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 26.0), Sense::click());
            let painter = ui.painter();
            if sel {
                painter.rect(r, 5, Color32::from_gray(30), Stroke::new(1.0, ACCENT), StrokeKind::Inside);
            } else if resp.hovered() {
                painter.rect_filled(r, 5, PANEL2);
            }
            painter.rect_filled(Rect::from_center_size(Pos2::new(r.left() + 12.0, r.center().y), Vec2::splat(10.0)), 2, c32(p.color));
            painter.text(Pos2::new(r.left() + 24.0, r.center().y), Align2::LEFT_CENTER, &p.name, FontId::proportional(13.0), Color32::WHITE);
            painter.text(Pos2::new(r.right() - 6.0, r.center().y), Align2::RIGHT_CENTER, format!("{}c", p.bars), FontId::proportional(11.0), DIM);
            if resp.clicked() {
                clicked = Some(p.id);
            }
        }
        if let Some(id) = clicked {
            self.select_pat(id);
        }
        ui.add_space(4.0);
        let mut name = self.cur_pat().name.clone();
        let r = ui.add(egui::TextEdit::singleline(&mut name).desired_width(f32::INFINITY));
        if r.changed() {
            if r.gained_focus() || self.hist.last().and_then(|h| h.pattern(self.sel_pat)).is_none_or(|hp| hp.name == self.cur_pat().name) {
                self.snap();
            }
            let id = self.sel_pat;
            if let Some(p) = self.p.pattern_mut(id) {
                p.name = name;
            }
            self.send();
        }
        ui.horizontal_wrapped(|ui| {
            if ui.button("+ Nuevo").clicked() {
                self.snap();
                let id = self.p.new_id();
                let color = PALETTE[self.p.patterns.len() % PALETTE.len()];
                let n = self.p.patterns.len() + 1;
                self.p.patterns.push(Pattern { id, name: format!("Patrón {n}"), bars: 1, color, steps: Default::default(), notes: Default::default(), audio: Default::default() });
                self.select_pat(id);
                self.send();
            }
            if ui.button("Clonar").clicked() {
                self.snap();
                let mut c = self.cur_pat().clone();
                c.id = self.p.new_id();
                c.name = format!("{} 2", c.name);
                c.color = PALETTE[self.p.patterns.len() % PALETTE.len()];
                let id = c.id;
                self.p.patterns.push(c);
                self.select_pat(id);
                self.send();
            }
            if ui.button("Borrar").clicked() {
                if self.p.patterns.len() < 2 {
                    self.msg = "Debe quedar al menos un patrón.".into();
                } else {
                    self.snap();
                    let id = self.sel_pat;
                    self.p.patterns.retain(|p| p.id != id);
                    self.p.playlist.retain(|c| c.pat != id);
                    let first = self.p.patterns[0].id;
                    self.select_pat(first);
                    self.send();
                }
            }
        });
    }

    // ---------- playlist ----------
    fn playlist(&mut self, ui: &mut Ui) {
        egui::Panel::left("patlist").resizable(true).default_size(if self.plat.mobile { 132.0 } else { 190.0 }).min_size(if self.plat.mobile { 110.0 } else { 150.0 }).frame(Frame::new().fill(Color32::from_gray(26)).inner_margin(6)).show(ui, |ui| {
            ui.horizontal(|ui| {
                for (i, n) in [(0u8, "Patrones"), (1, "Navegador")] {
                    let r = ui.add(egui::Button::selectable(self.left_tab == i, n));
                    if i == 1 {
                        self.tut_mark("browser", r.rect);
                    }
                    if r.clicked() {
                        self.left_tab = i;
                        // el navegador busca sonidos en el teléfono: ahí se pide el permiso de los archivos
                        #[cfg(target_os = "android")]
                        if i == 1 {
                            audio::pedir_permisos(&["android.permission.READ_MEDIA_AUDIO", "android.permission.READ_EXTERNAL_STORAGE"]);
                        }
                    }
                }
            });
            ui.separator();
            egui::ScrollArea::vertical().id_salt("patscroll").auto_shrink([false, false]).show(ui, |ui| {
                if self.left_tab == 1 {
                    self.browser_panel(ui);
                } else {
                    self.pattern_list(ui);
                }
            });
        });
        egui::CentralPanel::default().frame(Frame::new().fill(PANEL)).show(ui, |ui| {
            self.playlist_toolbar(ui);
            egui::ScrollArea::both().id_salt("pl").auto_shrink([false, false]).show(ui, |ui| self.playlist_canvas(ui));
        });
    }

    // ---------- channel rack ----------
    fn rack(&mut self, ui: &mut Ui) {
        Frame::new().fill(PANEL2).inner_margin(Margin::symmetric(10, 6)).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(format!("Channel Rack — {}", self.cur_pat().name)).strong());
                ui.label(RichText::new("COMPASES").small().color(DIM));
                let mut bars = self.cur_pat().bars;
                egui::ComboBox::from_id_salt("bars").width(50.0).selected_text(bars.to_string()).show_ui(ui, |ui| {
                    for b in [1, 2, 4, 8] {
                        ui.selectable_value(&mut bars, b, b.to_string());
                    }
                });
                if bars != self.cur_pat().bars {
                    self.snap();
                    let id = self.sel_pat;
                    if let Some(p) = self.p.pattern_mut(id) {
                        p.set_bars(bars);
                    }
                    self.send();
                }
                ui.label(RichText::new("AGREGAR CANAL").small().color(DIM));
                egui::ComboBox::from_id_salt("addinst").width(110.0).selected_text(self.add_inst.name()).show_ui(ui, |ui| {
                    for i in Inst::ALL {
                        ui.selectable_value(&mut self.add_inst, i, i.name());
                    }
                    #[cfg(windows)]
                    ui.selectable_value(&mut self.add_inst, Inst::Plugin, "Instrumento plugin (CLAP)");
                });
                if ui.button("+ Canal").clicked() && self.p.channels.len() < crate::engine::MAX_CH {
                    self.snap();
                    let id = self.p.new_id();
                    self.p.channels.push(Channel::new(id, self.add_inst));
                    self.sel_ch = id;
                    self.msg = format!("Canal «{}» agregado.", self.add_inst.name());
                    self.send();
                    match self.add_inst {
                        Inst::Sampler => self.open_win(Win::Sampler(id)),
                        Inst::Plugin => self.open_win(Win::Inserts(id)),
                        _ => {}
                    }
                }
                if self.settings.ayudas {
                    ui.label(RichText::new("Clic: paso · clic derecho: golpe suave · arrastrar arriba/abajo: velocidad · nombre: piano roll").small().color(DIM));
                }
            });
        });
        let cur_step = self.local_step().map(|s| s.floor() as usize);
        let out = egui::ScrollArea::both().id_salt("rack").auto_shrink([false, false]).show(ui, |ui| {
            ui.add_space(6.0);
            let ids: Vec<u32> = self.p.channels.iter().map(|c| c.id).collect();
            for id in ids {
                self.rack_row(ui, id, cur_step);
            }
            ui.add_space(30.0);
            if self.settings.ayudas {
                ui.label(RichText::new("   Arrastra aquí un audio desde el navegador para crear un canal NN Sampler.").small().color(DIM));
            }
        });
        let resp = ui.interact(out.inner_rect, egui::Id::new("rackdrop"), Sense::hover());
        if let Some(path) = resp.dnd_release_payload::<PathBuf>() {
            self.stop_preview();
            self.import_to_rack(&path);
        }
    }

    fn rack_row(&mut self, ui: &mut Ui, id: u32, cur_step: Option<usize>) {
        let Some(c) = self.p.channel(id).cloned() else { return };
        let pat = self.cur_pat().clone();
        let n = pat.len_steps() as usize;
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            ui.spacing_mut().slider_width = 52.0;
            let (r, led) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::click());
            ui.painter().circle(r.center(), 6.5, if c.mute { Color32::from_gray(43) } else { GREEN }, Stroke::new(1.0, Color32::BLACK));
            if led.on_hover_text("Silenciar").clicked() {
                self.snap();
                if let Some(cm) = self.p.channel_mut(id) {
                    cm.mute = !cm.mute;
                }
                self.send();
            }
            let nb = egui::Button::selectable(self.sel_ch == id, RichText::new(&c.name)).min_size(Vec2::new(110.0, 28.0));
            let r = ui.add(nb);
            if self.tut_melo_id() == Some(id) {
                self.tut_mark("melo", r.rect.intersect(ui.clip_rect()));
            }
            if r.on_hover_text(c.inst.name()).clicked() {
                self.sel_ch = id;
                if c.inst == Inst::Vocal {
                    self.alternar_win(Win::Tune(id));
                } else if c.inst == Inst::Sampler {
                    self.alternar_win(Win::Sampler(id));
                    self.preview(id, c.smp.root, 0.85);
                } else if c.inst == Inst::Audio {
                    self.show_playlist = true;
                } else if c.inst == Inst::Plugin {
                    self.alternar_win(Win::Inserts(id));
                } else if c.inst.is_drum() {
                    self.preview(id, 60, 0.85);
                } else {
                    self.view = View::Roll;
                    self.roll_center = true;
                }
            }
            let mut vol = c.vol;
            let r = ui.add(egui::Slider::new(&mut vol, 0.0..=1.2).show_value(false)).on_hover_text("Volumen");
            self.track(&r);
            if r.changed() {
                if let Some(cm) = self.p.channel_mut(id) {
                    cm.vol = vol;
                }
                self.send();
            }
            let mut pan = c.pan;
            let r = ui.add(egui::Slider::new(&mut pan, -1.0..=1.0).show_value(false)).on_hover_text("Paneo");
            self.track(&r);
            if r.changed() {
                if let Some(cm) = self.p.channel_mut(id) {
                    cm.pan = pan;
                }
                self.send();
            }
            let step_x = |i: usize| i as f32 * 24.0 + (i / 16) as f32 * 6.0;
            let width = step_x(n - 1) + 22.0;
            if c.inst == Inst::Vocal {
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 28.0), Sense::click());
                let painter = ui.painter_at(rect);
                painter.rect_filled(rect, 3, Color32::from_gray(32));
                let recording = self.rec.as_ref().is_some_and(|r| r.ch == id);
                match self.takes.get(&(pat.id, id)) {
                    Some(v) if !recording => {
                        let n = v.peaks.len().max(1);
                        let total = pat.len_steps() as f64 * self.p.step_secs();
                        let wfrac = (v.dur as f64 / total).min(1.0) as f32;
                        for (i, pk) in v.peaks.iter().enumerate() {
                            let x = rect.left() + i as f32 / n as f32 * rect.width() * wfrac;
                            let h = pk * 13.0;
                            painter.vline(x, (rect.center().y - h)..=(rect.center().y + h), Stroke::new(1.0, Color32::from_rgb(224, 122, 184)));
                        }
                    }
                    _ if recording => {
                        // forma de onda en vivo sobre el largo del patrón
                        painter.rect_filled(rect, 3, Color32::from_gray(28));
                        let total = (pat.len_steps() as f64 * self.p.step_secs()) as f32;
                        self.rec_draw(&painter, rect, rect.width() / total.max(0.1), 0.0, total);
                        painter.text(rect.left_top() + Vec2::new(6.0, 2.0), Align2::LEFT_TOP, "GRABANDO", FontId::proportional(10.0), Color32::WHITE);
                    }
                    _ => {
                        painter.text(rect.left_center() + Vec2::new(8.0, 0.0), Align2::LEFT_CENTER, "Sin toma — toca para abrir NN Tune y grabar con el micrófono", FontId::proportional(11.0), DIM);
                    }
                }
                if resp.clicked() {
                    self.alternar_win(Win::Tune(id));
                }
            } else if c.inst == Inst::Audio {
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 28.0), Sense::click());
                let painter = ui.painter_at(rect);
                painter.rect_filled(rect, 3, Color32::from_gray(35));
                let n = self.p.audio_clips.iter().filter(|a| a.ch == id).count();
                let t = if n == 0 { "Pista de audio — importa un audio o graba con el botón rojo para crear clips en la playlist".to_string() } else { format!("Pista de audio — {n} clips en la playlist") };
                painter.text(rect.left_center() + Vec2::new(8.0, 0.0), Align2::LEFT_CENTER, t, FontId::proportional(11.0), Color32::from_gray(205));
                if resp.clicked() {
                    self.sel_ch = id;
                    self.show_playlist = true;
                }
            } else if c.inst.uses_steps() && !pat.notes.get(&id).is_some_and(|v| !v.is_empty()) {
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 28.0), Sense::click_and_drag());
                if self.tut_kick_id() == Some(id) {
                    self.tut_mark("kick", rect.intersect(ui.clip_rect()));
                }
                let painter = ui.painter_at(rect);
                let steps = pat.steps.get(&id).cloned().unwrap_or_default();
                for i in 0..n {
                    let r = Rect::from_min_size(Pos2::new(rect.left() + step_x(i), rect.top()), Vec2::new(22.0, 28.0));
                    painter.rect_filled(r, 3, if (i / 4) % 2 == 1 { Color32::from_gray(55) } else { Color32::from_gray(46) });
                    let v = steps.get(i).copied().unwrap_or(0.0);
                    let suena = cur_step == Some(i);
                    if v > 0.0 {
                        let h = 28.0 * v.clamp(0.08, 1.0);
                        let col = if suena { ACCENT2 } else { ACCENT };
                        if suena {
                            painter.rect_filled(r.expand(2.0), 5, ACCENT.gamma_multiply(0.35));
                        }
                        painter.rect_filled(Rect::from_min_max(Pos2::new(r.left(), r.bottom() - h), r.right_bottom()), 4, col);
                    }
                    if suena {
                        painter.rect_stroke(r, 4, Stroke::new(1.5, Color32::from_white_alpha(150)), StrokeKind::Inside);
                    }
                }
                let hit = |x: f32| (0..n).find(|&i| {
                    let l = rect.left() + step_x(i);
                    x >= l && x < l + 23.0
                });
                if resp.clicked() || resp.secondary_clicked() {
                    if let Some(i) = resp.interact_pointer_pos().and_then(|p| hit(p.x)) {
                        self.snap();
                        let soft = resp.secondary_clicked();
                        let v = {
                            let pid = self.sel_pat;
                            let p = self.p.pattern_mut(pid).unwrap();
                            let a = p.steps.entry(id).or_insert_with(|| vec![0.0; n]);
                            a.resize(n, 0.0);
                            a[i] = if a[i] > 0.0 { 0.0 } else if soft { 0.35 } else { 0.85 };
                            a[i]
                        };
                        if v > 0.0 {
                            self.preview(id, if c.inst == Inst::Sampler { c.smp.root } else { 60 }, v);
                        }
                        self.send();
                    }
                }
                if resp.drag_started() {
                    if let Some(i) = ui.input(|inp| inp.pointer.press_origin()).and_then(|p| hit(p.x)) {
                        if steps.get(i).copied().unwrap_or(0.0) > 0.0 {
                            self.snap();
                            self.step_drag = Some((id, i));
                        }
                    }
                }
                if let Some((sid, i)) = self.step_drag {
                    if sid == id {
                        if let Some(p) = resp.interact_pointer_pos() {
                            let v = (1.0 - (p.y - rect.top()) / 28.0).clamp(0.05, 1.0);
                            let pid = self.sel_pat;
                            if let Some(a) = self.p.pattern_mut(pid).and_then(|pp| pp.steps.get_mut(&id)) {
                                a[i] = (v * 100.0).round() / 100.0;
                            }
                            self.msg = format!("{} paso {}: velocidad {:.0}%", c.name, i + 1, v * 100.0);
                            self.cmd(Cmd::Project(Box::new(self.p.clone())));
                        }
                        if resp.drag_stopped() {
                            self.step_drag = None;
                            self.send();
                        }
                    }
                }
            } else {
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 28.0), Sense::click());
                let painter = ui.painter_at(rect);
                painter.rect_filled(rect, 3, Color32::from_gray(39));
                let ns = pat.notes.get(&id).cloned().unwrap_or_default();
                if ns.is_empty() {
                    painter.text(rect.left_center() + Vec2::new(8.0, 0.0), Align2::LEFT_CENTER, "Vacío — toca para escribir notas en el piano roll", FontId::proportional(11.0), DIM);
                } else {
                    let lo = ns.iter().map(|x| x.m).min().unwrap() as f32;
                    let hi = ns.iter().map(|x| x.m).max().unwrap() as f32;
                    let rg = (hi - lo).max(8.0);
                    for x in &ns {
                        let px = rect.left() + step_x(x.s as usize);
                        let w = (x.l as f32 * 24.0 - 3.0).max(3.0);
                        let y = rect.bottom() - 5.0 - (x.m as f32 - lo) / rg * 19.0;
                        painter.rect_filled(Rect::from_min_size(Pos2::new(px, y), Vec2::new(w, 3.0)), 1, c32(pat.color));
                    }
                }
                if resp.clicked() {
                    self.sel_ch = id;
                    self.view = View::Roll;
                    self.roll_center = true;
                }
            }
            if c.inst == Inst::Synth && ui.button("NN Wave").on_hover_text("Editar el sintetizador").clicked() {
                self.alternar_win(Win::Wave(id));
            }
            if c.inst == Inst::Sampler && ui.button("NN Sampler").on_hover_text("Sample, cortes, tono y tempo").clicked() {
                self.alternar_win(Win::Sampler(id));
            }
            if c.inst == Inst::Plugin && ui.button("Plugin").on_hover_text("Elegir y abrir el instrumento CLAP").clicked() {
                self.alternar_win(Win::Inserts(id));
            }
            if c.inst == Inst::Audio {
                let recording = self.rec_track.as_ref().is_some_and(|r| r.ch == id);
                if recording {
                    if icon_button(ui, Ic::Stop, Vec2::new(28.0, 26.0), false, Some(RED)).on_hover_text("Detener grabación").clicked() {
                        self.rec_track_stop();
                    }
                } else if icon_button(ui, Ic::Rec, Vec2::new(28.0, 26.0), false, None).on_hover_text("Grabar en esta pista (desde el marcador de la regla)").clicked() {
                    self.sel_ch = id;
                    self.rec_track_start();
                }
            }
            if c.inst == Inst::Vocal {
                let recording = self.rec.as_ref().is_some_and(|r| r.ch == id);
                if recording {
                    if icon_button(ui, Ic::Stop, Vec2::new(28.0, 26.0), false, Some(RED)).on_hover_text("Detener grabación").clicked() {
                        self.rec_stop();
                    }
                } else if icon_button(ui, Ic::Rec, Vec2::new(28.0, 26.0), false, None).on_hover_text("Grabar voz en este patrón").clicked() {
                    self.rec_start(id);
                }
                if ui.button("NN Tune").clicked() {
                    self.alternar_win(Win::Tune(id));
                }
            }
            let open = self.open_set.contains(&id);
            if icon_button(ui, Ic::Gear, Vec2::new(28.0, 26.0), open, None).on_hover_text("Ajustes del canal").clicked() {
                if open { self.open_set.remove(&id); } else { self.open_set.insert(id); }
            }
            if icon_button(ui, Ic::Close, Vec2::new(28.0, 26.0), false, None).on_hover_text("Quitar canal").clicked() && self.p.channels.len() > 1 {
                self.snap();
                self.p.channels.retain(|x| x.id != id);
                for p in &mut self.p.patterns {
                    p.steps.remove(&id);
                    p.notes.remove(&id);
                }
                self.p.audio_clips.retain(|a| a.ch != id);
                self.p.autos.retain(|a| a.target.channel() != Some(id));
                self.fix_selection();
                self.send();
                self.plugins_reload_all();
            }
        });
        if self.open_set.contains(&id) {
            ui.horizontal_wrapped(|ui| {
                ui.add_space(40.0);
                ui.spacing_mut().slider_width = 110.0;
                ui.label(RichText::new("Afinación").small().color(DIM));
                let mut tune = c.tune;
                let r = ui.add(egui::Slider::new(&mut tune, -12.0..=12.0).step_by(1.0).suffix(" st"));
                self.track(&r);
                if r.changed() {
                    if let Some(cm) = self.p.channel_mut(id) {
                        cm.tune = tune;
                    }
                    self.send();
                }
                if r.drag_stopped() || (r.changed() && !r.dragged()) {
                    self.preview(id, 60, 0.85);
                }
                if c.inst.is_drum() {
                    ui.label(RichText::new("Decay").small().color(DIM));
                    let mut d = c.decay;
                    let r = ui.add(egui::Slider::new(&mut d, 0.3..=2.5).prefix("×"));
                    self.track(&r);
                    if r.changed() {
                        if let Some(cm) = self.p.channel_mut(id) {
                            cm.decay = d;
                        }
                        self.send();
                    }
                    if r.drag_stopped() {
                        self.preview(id, 60, 0.85);
                    }
                } else if c.inst.melodic() {
                    ui.label(RichText::new("Strum (rasgueo)").small().color(DIM));
                    let mut s = c.strum * 1000.0;
                    let r = ui.add(egui::Slider::new(&mut s, 0.0..=60.0).suffix(" ms"));
                    self.track(&r);
                    if r.changed() {
                        if let Some(cm) = self.p.channel_mut(id) {
                            cm.strum = s / 1000.0;
                        }
                        self.send();
                    }
                }
                self.channel_presets(ui, id);
                if !c.inst.is_drum() {
                    if let Some(k) = self.p.channels.iter().find(|x| x.inst == Inst::Kick).map(|x| x.id) {
                        let on = c.comp.on && c.comp.sc == k;
                        if ui.add(egui::Button::selectable(on, "Sidechain al kick")).on_hover_text("El canal se agacha en cada golpe del kick (bombeo)").clicked() {
                            self.snap();
                            if let Some(cm) = self.p.channel_mut(id) {
                                cm.comp = if on { CompP::channel() } else { CompP::ducking(k) };
                            }
                            self.send();
                        }
                    }
                }
                ui.label(RichText::new("Nombre").small().color(DIM));
                let mut name = c.name.clone();
                let r = ui.add(egui::TextEdit::singleline(&mut name).desired_width(120.0));
                if r.gained_focus() {
                    self.snap();
                }
                if r.changed() {
                    if let Some(cm) = self.p.channel_mut(id) {
                        cm.name = name;
                    }
                    self.send();
                }
            });
        }
        ui.add_space(3.0);
    }

    // ---------- piano roll ----------
    fn roll(&mut self, ui: &mut Ui) {
        let melodic = self.melodic();
        let rc = self.roll_ch();
        Frame::new().fill(PANEL2).inner_margin(Margin::symmetric(10, 6)).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                let chn = rc.and_then(|id| self.p.channel(id)).map(|c| c.name.clone()).unwrap_or_default();
                ui.label(RichText::new(format!("Piano Roll — {} · {}", self.cur_pat().name, chn)).strong());
                ui.label(RichText::new("CANAL").small().color(DIM));
                let mut sel = self.sel_ch;
                egui::ComboBox::from_id_salt("rollch").width(110.0).selected_text(chn.clone()).show_ui(ui, |ui| {
                    for (id, n) in &melodic {
                        ui.selectable_value(&mut sel, *id, n);
                    }
                });
                if sel != self.sel_ch {
                    self.sel_ch = sel;
                    self.roll_center = true;
                }
                ui.label(RichText::new("HERRAMIENTA").small().color(DIM));
                egui::ComboBox::from_id_salt("tool").width(190.0).selected_text(CHORDS[self.tool].0).show_ui(ui, |ui| {
                    for (i, (n, _)) in CHORDS.iter().enumerate() {
                        ui.selectable_value(&mut self.tool, i, *n);
                    }
                });
                ui.label(RichText::new("DURACIÓN").small().color(DIM));
                let lens = [(1, "1/16"), (2, "1/8"), (4, "1/4"), (8, "1/2"), (16, "1 compás")];
                let cur = lens.iter().find(|l| l.0 == self.note_len).map(|l| l.1).unwrap_or("…");
                egui::ComboBox::from_id_salt("len").width(80.0).selected_text(cur).show_ui(ui, |ui| {
                    for (v, n) in lens {
                        ui.selectable_value(&mut self.note_len, v, n);
                    }
                });
                ui.label(RichText::new("VEL").small().color(DIM));
                ui.spacing_mut().slider_width = 70.0;
                ui.add(egui::Slider::new(&mut self.note_vel, 0.1..=1.0).show_value(false));
                self.scale_picker(ui);
                if let Some(id) = rc {
                    self.roll_tools(ui, id);
                }
                let mr = self.midi_rec;
                if ui.add(egui::Button::selectable(mr, if mr { "Grabando MIDI" } else { "Grabar MIDI" })).on_hover_text("Con el patrón sonando, lo que toques en el teclado MIDI se escribe aquí").clicked() {
                    self.midi_rec = !mr;
                }
                if ui.button("Vaciar").clicked() {
                    if let Some(id) = rc {
                        self.snap();
                        let pid = self.sel_pat;
                        if let Some(p) = self.p.pattern_mut(pid) {
                            p.notes.insert(id, vec![]);
                        }
                        self.send();
                    }
                }
            });
        });
        let Some(ch) = rc else {
            ui.centered_and_justified(|ui| ui.label("Agrega un canal melódico (Rhodes, Pad, Sub Bass, Campana, NN Wave, NN Sampler o un plugin) en el Channel Rack."));
            return;
        };
        egui::Panel::bottom("vel").exact_size(74.0).frame(Frame::new().fill(BG)).show(ui, |ui| self.velocity_lane(ui, ch));
        egui::CentralPanel::default().frame(Frame::new().fill(PANEL)).show(ui, |ui| {
            self.tut_mark("roll", ui.max_rect());
            let pat = self.cur_pat().clone();
            let steps = pat.len_steps();
            let rows = (HIGH - LOW + 1) as f32;
            let mut sa = egui::ScrollArea::both().id_salt(("roll", pat.id)).auto_shrink([false, false]);
            if self.roll_center {
                let ns = pat.notes.get(&ch).cloned().unwrap_or_default();
                let mid = if ns.is_empty() { 60.0 } else { ns.iter().map(|n| n.m as f32).sum::<f32>() / ns.len() as f32 };
                sa = sa.vertical_scroll_offset(((HIGH as f32 - mid) * RH - ui.available_height() / 2.0).max(0.0));
                self.roll_center = false;
            }
            let out = sa.show_viewport(ui, |ui, vp| {
                let size = Vec2::new(KW + steps as f32 * CW, rows * RH);
                let (rect, resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
                let painter = ui.painter_at(rect);
                let gx = rect.left() + KW;
                let row_y = |m: u8| rect.top() + (HIGH - m) as f32 * RH;
                let key = self.p.key;
                for m in LOW..=HIGH {
                    let y = row_y(m);
                    let pc = (m % 12) as usize;
                    let col = if self.p.in_scale(m as i32) {
                        if pc == key as usize % 12 { Color32::from_gray(62) } else { Color32::from_gray(44) }
                    } else if is_black(m) {
                        Color32::from_gray(25)
                    } else {
                        Color32::from_gray(30)
                    };
                    painter.rect_filled(Rect::from_min_size(Pos2::new(gx, y), Vec2::new(steps as f32 * CW, RH)), 0, col);
                    painter.hline(gx..=rect.right(), y + RH - 0.5, Stroke::new(1.0, Color32::from_gray(24)));
                }
                for s in 0..=steps {
                    let x = gx + s as f32 * CW;
                    let (w, c) = if s % 16 == 0 { (2.0, Color32::from_gray(12)) } else if s % 4 == 0 { (1.0, Color32::from_gray(22)) } else { (1.0, Color32::from_gray(29)) };
                    painter.vline(x, rect.top()..=rect.bottom(), Stroke::new(w, c));
                }
                // notas fantasma de otros canales
                for (oid, ns) in &pat.notes {
                    if *oid == ch {
                        continue;
                    }
                    for n in ns.iter().filter(|n| n.m >= LOW && n.m <= HIGH) {
                        painter.rect_filled(Rect::from_min_size(Pos2::new(gx + n.s as f32 * CW + 1.0, row_y(n.m) + 1.0), Vec2::new(n.l as f32 * CW - 2.0, RH - 2.0)), 2, Color32::from_white_alpha(16));
                    }
                }
                let ns = pat.notes.get(&ch).cloned().unwrap_or_default();
                for n in ns.iter().filter(|n| n.m >= LOW && n.m <= HIGH) {
                    let r = Rect::from_min_size(Pos2::new(gx + n.s as f32 * CW + 1.0, row_y(n.m) + 1.0), Vec2::new(n.l as f32 * CW - 2.0, RH - 2.0));
                    let a = (0.45 + 0.55 * n.v).clamp(0.0, 1.0);
                    painter.rect_filled(r, 2, c32(pat.color).gamma_multiply(a));
                    painter.rect_filled(Rect::from_min_max(Pos2::new(r.right() - 4.0, r.top() + 1.0), Pos2::new(r.right() - 1.0, r.bottom() - 1.0)), 1, Color32::from_black_alpha(130));
                    if r.width() > 34.0 {
                        painter.text(r.left_center() + Vec2::new(3.0, 0.0), Align2::LEFT_CENTER, nname(n.m), FontId::proportional(9.0), Color32::from_gray(13));
                    }
                }
                if let Some(ls) = self.local_step() {
                    painter.vline(gx + ls as f32 * CW, rect.top()..=rect.bottom(), Stroke::new(2.0, ACCENT2));
                }
                // teclado fijo a la izquierda
                let kx = rect.left() + vp.min.x;
                for m in LOW..=HIGH {
                    let y = row_y(m);
                    let r = Rect::from_min_size(Pos2::new(kx, y), Vec2::new(KW, RH));
                    painter.rect_filled(r, 0, if is_black(m) { Color32::from_gray(22) } else { Color32::from_rgb(232, 232, 232) });
                    painter.hline(kx..=kx + KW, y + RH - 0.5, Stroke::new(1.0, Color32::from_black_alpha(80)));
                    if self.p.in_scale(m as i32) {
                        painter.rect_filled(Rect::from_min_size(Pos2::new(kx + KW - 5.0, y), Vec2::new(5.0, RH)), 0, if m % 12 == key % 12 { ACCENT } else { ACCENT.gamma_multiply(0.4) });
                    }
                    if m % 12 == 0 || m % 12 == key % 12 {
                        painter.text(Pos2::new(kx + 4.0, y + RH / 2.0), Align2::LEFT_CENTER, nname(m), FontId::proportional(9.5), if is_black(m) { Color32::LIGHT_GRAY } else { Color32::from_gray(29) });
                    }
                }
                self.roll_input(ui, &resp, rect, kx, ch, steps);
            });
            self.roll_x = out.state.offset().x;
        });
    }

    fn roll_input(&mut self, ui: &Ui, resp: &Response, rect: Rect, kx: f32, ch: u32, steps: u32) {
        let gx = rect.left() + KW;
        let at = |p: Pos2| -> (f32, u8) { ((p.x - gx) / CW, (HIGH as i32 - ((p.y - rect.top()) / RH).floor() as i32).clamp(LOW as i32, HIGH as i32) as u8) };
        let pid = self.sel_pat;
        let hit = |ns: &[Note], fx: f32, m: u8| ns.iter().rposition(|n| n.m == m && fx >= n.s as f32 && fx < (n.s + n.l) as f32);
        let on_keys = |p: Pos2| p.x < kx + KW;
        // teclado: escuchar
        if resp.clicked() {
            if let Some(p) = resp.interact_pointer_pos() {
                if on_keys(p) {
                    self.preview(ch, at(p).1, 0.8);
                    return;
                }
            }
        }
        if resp.secondary_clicked() {
            if let Some(p) = resp.interact_pointer_pos() {
                let (fx, m) = at(p);
                let ns = self.p.pattern(pid).and_then(|pp| pp.notes.get(&ch)).cloned().unwrap_or_default();
                if let Some(i) = hit(&ns, fx, m) {
                    self.snap();
                    if let Some(v) = self.p.pattern_mut(pid).and_then(|pp| pp.notes.get_mut(&ch)) {
                        v.remove(i);
                    }
                    self.send();
                }
            }
            return;
        }
        let begin = |this: &mut Self, p: Pos2, dragging: bool| {
            let (fx, m) = at(p);
            if fx < 0.0 || fx >= steps as f32 {
                return;
            }
            this.snap();
            let tool = this.tool;
            let (len, vel) = (this.note_len, this.note_vel);
            let snap_sc = this.scale_snap;
            let proj = if snap_sc { Some(this.p.clone()) } else { None };
            let Some(pp) = this.p.pattern_mut(pid) else { return };
            let ns = pp.notes.entry(ch).or_default();
            if let Some(i) = hit(ns, fx, m) {
                let n = ns[i];
                if dragging {
                    let near_end = fx > (n.s + n.l) as f32 - 0.35;
                    this.roll_drag = Some(if near_end { RollDrag::Resize { idx: vec![i] } } else { RollDrag::Move { idx: vec![i], orig: vec![(n.s, n.m)], s0: fx.floor() as i32, m0: m as i32 } });
                }
                this.preview(ch, n.m, n.v);
            } else {
                let s = fx.floor() as u32;
                let start = ns.len();
                let base = proj.as_ref().map(|p| p.snap_scale(m as i32)).unwrap_or(m as i32);
                for d in CHORDS[tool].1 {
                    let mm = proj.as_ref().map(|p| p.snap_scale(base + d)).unwrap_or(base + d);
                    if (LOW as i32..=HIGH as i32).contains(&mm) {
                        ns.push(Note { s, l: len.min(steps - s).max(1), m: mm as u8, v: vel, o: 0.0 });
                    }
                }
                let new: Vec<usize> = (start..ns.len()).collect();
                let ms: Vec<u8> = new.iter().map(|&i| ns[i].m).collect();
                for mm in ms {
                    this.preview(ch, mm, vel * 0.8);
                }
                if dragging {
                    this.roll_drag = Some(RollDrag::Resize { idx: new });
                }
            }
            this.cmd(Cmd::Project(Box::new(this.p.clone())));
        };
        if resp.clicked() {
            if let Some(p) = resp.interact_pointer_pos() {
                begin(self, p, false);
                self.send();
            }
        }
        if resp.drag_started() {
            if let Some(p) = ui.input(|i| i.pointer.press_origin()) {
                if !on_keys(p) {
                    begin(self, p, true);
                }
            }
        }
        if let (Some(d), Some(p)) = (&self.roll_drag, resp.interact_pointer_pos()) {
            let (fx, m) = at(p);
            if let Some(ns) = self.p.pattern_mut(pid).and_then(|pp| pp.notes.get_mut(&ch)) {
                match d {
                    RollDrag::Move { idx, orig, s0, m0 } => {
                        let ds = fx.floor() as i32 - s0;
                        let dm = m as i32 - m0;
                        for (k, &i) in idx.iter().enumerate() {
                            if let Some(n) = ns.get_mut(i) {
                                n.s = (orig[k].0 as i32 + ds).clamp(0, (steps - n.l) as i32) as u32;
                                n.m = (orig[k].1 as i32 + dm).clamp(LOW as i32, HIGH as i32) as u8;
                            }
                        }
                    }
                    RollDrag::Resize { idx } => {
                        for &i in idx {
                            if let Some(n) = ns.get_mut(i) {
                                n.l = ((fx - n.s as f32).round().max(1.0) as u32).min(steps - n.s);
                            }
                        }
                    }
                }
            }
            if resp.drag_stopped() {
                if let Some(RollDrag::Resize { idx }) = &self.roll_drag {
                    if let Some(n) = idx.first().and_then(|&i| self.p.pattern(pid)?.notes.get(&ch)?.get(i)) {
                        self.note_len = n.l;
                    }
                }
                self.roll_drag = None;
                self.send();
            } else {
                self.cmd(Cmd::Project(Box::new(self.p.clone())));
            }
        } else if resp.drag_stopped() {
            self.roll_drag = None;
        }
    }

    fn velocity_lane(&mut self, ui: &mut Ui, ch: u32) {
        let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0, BG);
        painter.hline(rect.left()..=rect.right(), rect.top() + 0.5, Stroke::new(2.0, Color32::from_gray(13)));
        let pat = self.cur_pat().clone();
        let gx = rect.left() + KW - self.roll_x;
        let h = rect.height() - 10.0;
        for s in (0..=pat.len_steps()).step_by(4) {
            let x = gx + s as f32 * CW;
            if x > rect.left() + KW {
                painter.vline(x, rect.top()..=rect.bottom(), Stroke::new(1.0, if s % 16 == 0 { Color32::from_gray(12) } else { Color32::from_gray(31) }));
            }
        }
        if let Some(ns) = pat.notes.get(&ch) {
            for n in ns {
                let x = gx + n.s as f32 * CW + CW / 2.0;
                if x < rect.left() + KW {
                    continue;
                }
                let top = rect.bottom() - n.v * h;
                painter.vline(x, top..=rect.bottom(), Stroke::new(3.0, c32(pat.color)));
                painter.circle_filled(Pos2::new(x, top), 3.5, c32(pat.color));
            }
        }
        painter.rect_filled(Rect::from_min_size(rect.min, Vec2::new(KW, rect.height())), 0, Color32::from_gray(25));
        painter.text(rect.left_top() + Vec2::new(6.0, 8.0), Align2::LEFT_TOP, "VEL.", FontId::proportional(10.0), DIM);
        if resp.drag_started() || resp.clicked() {
            self.snap();
        }
        if resp.dragged() || resp.clicked() {
            if let Some(p) = resp.interact_pointer_pos() {
                if p.x > rect.left() + KW {
                    let s = ((p.x - gx) / CW).floor() as i64;
                    let v = ((rect.bottom() - p.y) / h).clamp(0.05, 1.0);
                    let pid = self.sel_pat;
                    let mut changed = false;
                    if let Some(ns) = self.p.pattern_mut(pid).and_then(|pp| pp.notes.get_mut(&ch)) {
                        for n in ns.iter_mut().filter(|n| n.s as i64 == s) {
                            n.v = (v * 100.0).round() / 100.0;
                            changed = true;
                        }
                    }
                    if changed {
                        self.msg = format!("Velocidad paso {}: {:.0}%", s + 1, v * 100.0);
                        self.cmd(Cmd::Project(Box::new(self.p.clone())));
                    }
                }
            }
        }
        if resp.drag_stopped() || resp.clicked() {
            self.send();
        }
    }

    // ---------- mixer ----------
    fn mixer(&mut self, ui: &mut Ui) {
        egui::ScrollArea::both().id_salt("mix").auto_shrink([false, false]).show(ui, |ui| {
            ui.add_space(6.0);
            ui.horizontal_top(|ui| {
                ui.add_space(6.0);
                let ids: Vec<u32> = self.p.channels.iter().map(|c| c.id).collect();
                for (i, id) in ids.into_iter().enumerate() {
                    self.strip(ui, i, id);
                }
                self.master_strip(ui);
            });
        });
    }

    fn strip(&mut self, ui: &mut Ui, idx: usize, id: u32) {
        let Some(mut c) = self.p.channel(id).cloned() else { return };
        let before = c.clone();
        let mut touched = false;
        let mut started = false;
        let (mut open_eq, mut open_comp, mut open_fx) = (false, false, false);
        let mut open_nn = false;
        Frame::new().fill(Color32::from_gray(25)).stroke(Stroke::new(1.0, Color32::from_gray(13))).corner_radius(6).inner_margin(6).show(ui, |ui| {
            ui.set_width(96.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(&c.name).strong().size(12.0));
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 3.0;
                    if ui.add(egui::Button::selectable(!eq_is_flat(&c.eq), RichText::new("EQ").size(10.5)).min_size(Vec2::new(40.0, 18.0))).on_hover_text("NN EQ del canal").clicked() {
                        open_eq = true;
                    }
                    if ui.add(egui::Button::selectable(c.comp.on, RichText::new(if c.comp.sc != 0 { "SC" } else { "Comp" }).size(10.5)).min_size(Vec2::new(44.0, 18.0))).on_hover_text("NN Dynamics del canal (y sidechain)").clicked() {
                        open_comp = true;
                    }
                });
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 3.0;
                    let n = c.inserts.len();
                    if ui.add(egui::Button::selectable(n > 0, RichText::new(if n > 0 { format!("FX {n}") } else { "FX".into() }).size(10.5)).min_size(Vec2::new(40.0, 18.0))).on_hover_text("Plugins CLAP del canal").clicked() {
                        open_fx = true;
                    }
                    let has_auto = self.p.autos.iter().any(|a| a.target.channel() == Some(id));
                    ui.menu_button(RichText::new(if has_auto { "Auto*" } else { "Auto" }).size(10.5), |ui| desplazable(ui, |ui| {
                        ui.label(RichText::new("CREAR AUTOMATIZACIÓN").small().color(DIM));
                        self.auto_menu(ui, id);
                    }));
                });
                let ne = c.efectos.len();
                if ui.add(egui::Button::selectable(ne > 0, RichText::new(if ne > 0 { format!("Efectos ({ne})") } else { "Efectos".into() }).size(10.5)).min_size(Vec2::new(87.0, 18.0))).on_hover_text("Chorus, Phaser, Tape Delay, Bitcrusher, Spreader…").clicked() {
                    open_nn = true;
                }
                let mut cutn = ((c.cut / 200.0).ln() / 100f32.ln()).clamp(0.0, 1.0);
                egui::Grid::new(("knobs", id)).spacing(Vec2::new(4.0, 2.0)).show(ui, |ui| {
                    let r = knob(ui, &mut c.hi, -12.0, 12.0, 0.0, "Agudos", |v| format!("{:+.1}", v));
                    started |= r.drag_started();
                    touched |= r.changed();
                    let r = knob(ui, &mut c.lo, -12.0, 12.0, 0.0, "Graves", |v| format!("{:+.1}", v));
                    started |= r.drag_started();
                    touched |= r.changed();
                    ui.end_row();
                    let r = knob(ui, &mut cutn, 0.0, 1.0, 1.0, "Filtro", |v| {
                        let hz = 200.0 * 100f32.powf(v);
                        if hz >= 19_900.0 { "abierto".to_string() } else if hz >= 1000.0 { format!("{:.1}k", hz / 1000.0) } else { format!("{:.0}", hz) }
                    });
                    if r.changed() {
                        c.cut = (200.0 * 100f32.powf(cutn)).round();
                    }
                    started |= r.drag_started();
                    touched |= r.changed();
                    let r = knob(ui, &mut c.rev, 0.0, 1.0, 0.0, "Reverb", |v| format!("{:.0}%", v * 100.0));
                    started |= r.drag_started();
                    touched |= r.changed();
                    ui.end_row();
                    let r = knob(ui, &mut c.dly, 0.0, 0.8, 0.0, "Delay", |v| format!("{:.0}%", v * 100.0));
                    started |= r.drag_started();
                    touched |= r.changed();
                    let r = knob(ui, &mut c.space, 0.0, 1.0, 0.0, "Space", |v| format!("{:.0}%", v * 100.0));
                    started |= r.drag_started();
                    touched |= r.changed();
                    ui.end_row();
                    let r = knob(ui, &mut c.pan, -1.0, 1.0, 0.0, "Paneo", pan_txt);
                    started |= r.drag_started();
                    touched |= r.changed();
                    ui.end_row();
                });
                ui.horizontal(|ui| {
                    ui.add_space(14.0);
                    let r = ui.add_sized([24.0, 130.0], egui::Slider::new(&mut c.vol, 0.0..=1.2).vertical().show_value(false));
                    started |= r.drag_started();
                    touched |= r.changed();
                    meter(ui, Vec2::new(10.0, 130.0), self.meters.get(idx).copied().unwrap_or(0.0));
                });
                let db = if c.vol > 0.0 { format!("{:.1} dB", 20.0 * c.vol.log10()) } else { "-∞".into() };
                ui.label(RichText::new(db).monospace().size(10.0).color(DIM));
                ui.horizontal(|ui| {
                    ui.add_space(14.0);
                    let m = egui::Button::new(RichText::new("M").size(11.0)).fill(if c.mute { RED } else { PANEL2 }).min_size(Vec2::new(28.0, 20.0));
                    if ui.add(m).on_hover_text("Silenciar").clicked() {
                        c.mute = !c.mute;
                        started = true;
                        touched = true;
                    }
                    let s = egui::Button::new(RichText::new("S").size(11.0).color(if c.solo { Color32::BLACK } else { Color32::WHITE })).fill(if c.solo { Color32::from_rgb(224, 201, 74) } else { PANEL2 }).min_size(Vec2::new(28.0, 20.0));
                    if ui.add(s).on_hover_text("Solo").clicked() {
                        c.solo = !c.solo;
                        started = true;
                        touched = true;
                    }
                });
            });
        });
        if open_nn {
            self.alternar_win(Win::Efectos(id));
        }
        if open_eq {
            self.alternar_win(Win::Eq(Some(id)));
        }
        if open_comp {
            self.alternar_win(Win::Comp(id));
        }
        if open_fx {
            self.alternar_win(Win::Inserts(id));
        }
        if started {
            self.snap();
        }
        if touched {
            let _ = before;
            if let Some(cm) = self.p.channel_mut(id) {
                *cm = c;
            }
            self.send();
        }
    }

    fn master_strip(&mut self, ui: &mut Ui) {
        Frame::new().fill(Color32::from_gray(24)).stroke(Stroke::new(1.0, Color32::from_gray(70))).corner_radius(6).inner_margin(6).show(ui, |ui| {
            ui.set_width(104.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("Master").strong().size(12.0));
                egui::Grid::new("mbtn").spacing(Vec2::new(3.0, 3.0)).show(ui, |ui| {
                    for (i, (n, w)) in [("EQ", Win::Eq(None)), ("Dinám.", Win::Dyn), ("Verb", Win::Verb), ("Space", Win::Space), ("Master", Win::Master), ("FX", Win::Inserts(0)), ("Efectos", Win::Efectos(0))].into_iter().enumerate() {
                        if ui.add(egui::Button::new(RichText::new(n).size(10.5)).min_size(Vec2::new(46.0, 18.0))).clicked() {
                            self.alternar_win(w);
                        }
                        if i % 2 == 1 {
                            ui.end_row();
                        }
                    }
                });
                let mut vin = self.p.vinyl;
                let r = knob(ui, &mut vin, 0.0, 1.0, 0.0, "Vinilo", |v| format!("{:.0}%", v * 100.0));
                if r.drag_started() {
                    self.snap();
                }
                if r.changed() {
                    self.p.vinyl = vin;
                    self.send();
                }
                ui.horizontal(|ui| {
                    ui.add_space(10.0);
                    let mut m = self.p.master;
                    let r = ui.add_sized([24.0, 130.0], egui::Slider::new(&mut m, 0.0..=1.2).vertical().show_value(false));
                    if r.drag_started() {
                        self.snap();
                    }
                    if r.changed() {
                        self.p.master = m;
                        self.send();
                    }
                    meter(ui, Vec2::new(10.0, 130.0), self.mmeter[0]);
                    meter(ui, Vec2::new(10.0, 130.0), self.mmeter[1]);
                });
                ui.label(RichText::new(format!("{:.1} dB", 20.0 * self.p.master.max(1e-4).log10())).monospace().size(10.0).color(DIM));
                ui.label(RichText::new(format!("Comp -{:.1} · Lim -{:.1} dB", self.gr_hold[0], self.gr_hold[1])).small().color(DIM));
                ui.label(RichText::new(self.lufs_text()).monospace().size(11.0).color(ACCENT2));
                if ui.small_button("Auto volumen").clicked() {
                    self.add_auto(AutoTarget::Master);
                }
            });
        });
    }

    // ---------- vistas ----------
    fn view_tabs(&mut self, ui: &mut Ui, compact: bool) {
        ui.horizontal(|ui| {
            ui.add_space(6.0);
            let tabs: &[(View, &str)] = if compact {
                &[(View::Playlist, "Playlist"), (View::Rack, "Channel Rack"), (View::Roll, "Piano Roll"), (View::Mixer, "Mixer"), (View::Pads, "Pads")]
            } else {
                &[(View::Rack, "Channel Rack  F6"), (View::Roll, "Piano Roll  F7"), (View::Mixer, "Mixer  F9"), (View::Pads, "Pads  F8")]
            };
            for &(v, n) in tabs {
                let sel = self.view == v || (!compact && v == View::Rack && self.view == View::Playlist);
                let r = pestana_metal(ui, sel, n, if compact { 0.0 } else { 130.0 });
                if v == View::Mixer {
                    self.tut_mark("mixer", r.rect);
                }
                if r.clicked() {
                    self.view = v;
                    if v == View::Roll {
                        self.roll_center = true;
                    }
                }
            }
            if !compact {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if pestana_metal(ui, self.show_playlist, "Playlist  F5", 120.0).clicked() {
                        self.show_playlist = !self.show_playlist;
                    }
                });
            }
        });
    }

    fn show_view(&mut self, ui: &mut Ui, v: View) {
        match v {
            View::Playlist => self.playlist(ui),
            View::Rack => self.rack(ui),
            View::Roll => self.roll(ui),
            View::Mixer => self.mixer(ui),
            View::Pads => self.pads(ui),
        }
    }

    fn status_bar(&mut self, ui: &mut Ui) {
        let chica = self.plat.mobile || ui.available_width() < 1100.0;
        ui.horizontal(|ui| {
            ui.add_space(6.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(6.0);
                // la marca siempre a la vista, en la esquina
                ui.label(RichText::new(concat!("NoName Studio ", env!("CARGO_PKG_VERSION"))).size(11.5).strong().color(Color32::from_gray(207)));
                if let Some(t) = &self.logo {
                    ui.add(egui::Image::new(t).fit_to_exact_size(Vec2::splat(15.0)));
                }
                ui.add_space(10.0);
                // en pantallas chicas solo lo esencial
                if !chica {
                    if let Some(a) = &self.audio {
                        ui.label(RichText::new(format!("Audio: {} · {} Hz", a.device, a.sr)).size(11.0).color(DIM));
                    }
                }
                let bars = self.p.song_bars();
                let secs = bars as f64 * 16.0 * self.p.step_secs();
                let info = if chica { format!("{}:{:02} · {}", secs as u32 / 60, secs as u32 % 60, self.p.key_label()) } else { format!("Patrón: {} · Canción: {} compases ({}:{:02}) · {}", self.cur_pat().name, bars, secs as u32 / 60, secs as u32 % 60, self.p.key_label()) };
                ui.label(RichText::new(info).size(12.0).color(ACCENT2));
                #[cfg(not(target_os = "android"))]
                if !chica {
                    if let Some(m) = &self.midi_conn {
                        ui.label(RichText::new(format!("MIDI: {}", m.name)).size(11.0).color(DIM));
                    }
                }
                // el mensaje usa lo que queda y se corta con «…» si no entra
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    ui.add(egui::Label::new(RichText::new(&self.msg).size(12.0).color(DIM)).truncate());
                });
            });
        });
    }

    fn run_autotest(&mut self, ctx: &egui::Context) {
        let Some(t) = &mut self.autotest else { return };
        t.frame += 1;
        let f = t.frame;
        if f == 1 {
            self.tut = None;
            if self.p.song_bars() == 0 {
                self.p = Project::demo();
                self.fix_selection();
                self.send();
            }
        }
        match f {
            5 => self.play(0),
            40 => self.view = View::Roll,
            70 => self.view = View::Mixer,
            100 => {
                self.stop();
                self.start_export(Format::Mp3);
            }
            _ => {}
        }
        if f > 100 && self.job.is_none() {
            let log = self.autotest.as_ref().map(|t| t.log.join("\n")).unwrap_or_default();
            let _ = std::fs::write(self.plat.data_dir.join("autotest.log"), format!("ok frames={f}\naudio={}\n{log}\n", self.audio.as_ref().map(|a| a.device.clone()).unwrap_or_else(|| format!("ERROR {:?}", self.audio_err))));
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        ctx.request_repaint();
    }
}

impl eframe::App for StudioApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if !self.maximized && !self.plat.mobile {
            self.maximized = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(true));
        }
        self.poll_export();
        self.poll_tune();
        self.autosave();
        self.files_poll();
        self.files_sync();
        self.midi_poll();
        self.poll_master();
        self.version_tick();
        self.handle_dropped(&ctx);
        if self.plugins_poll() || self.master_job.is_some() || !self.pending.is_empty() || self.rec_track.is_some() {
            ctx.request_repaint_after(Duration::from_millis(60));
        }
        let lu = self.status.lufs();
        for k in 0..3 {
            if self.playing() {
                self.lufs_hold[k] = lu[k];
            }
        }
        if self.playing() {
            self.tpeak_hold = f32::from_bits(self.status.tpeak.load(Ordering::Relaxed));
        }
        self.load_hold = f32::from_bits(self.status.load.load(Ordering::Relaxed));
        // reconectar solo si el audio o el micrófono se cortaron (como mucho una vez por segundo)
        if self.reconexion.is_none_or(|t| t.elapsed() > Duration::from_secs(1)) {
            if self.status.audio_caido.swap(false, Ordering::Relaxed) {
                self.reconexion = Some(Instant::now());
                self.reiniciar_audio();
                self.msg = "Se reconectó el audio.".into();
            }
            if self.status.mic_caido.swap(false, Ordering::Relaxed) {
                self.reconexion = Some(Instant::now());
                self.mic_mon = None;
                self.escucha_sync();
            }
        }
        if self.reconexion.is_some_and(|t| t.elapsed() < Duration::from_secs(3)) {
            ctx.request_repaint_after(Duration::from_millis(300));
        }
        self.status.metro_vol.store(self.settings.metro_vol.to_bits(), Ordering::Relaxed);
        self.status.metro_sonido.store(self.settings.metro_sonido as u32, Ordering::Relaxed);
        self.status.metro_acento.store(self.settings.metro_acento, Ordering::Relaxed);
        self.status.metro_sub.store(self.settings.metro_sub as u32, Ordering::Relaxed);
        // Android: pedir los permisos (micrófono y archivos de audio) al abrir la app
        // (cuando la app está en pantalla: si se pide con el teléfono bloqueado, el aviso se pierde)
        #[cfg(target_os = "android")]
        if !self.permisos_pedidos && self.inicio.elapsed() < Duration::from_millis(1500) {
            // la app solo se redibuja cuando pasa algo: se pide un redibujo para llegar al momento de pedir permisos
            ctx.request_repaint_after(Duration::from_millis(300));
        }
        #[cfg(target_os = "android")]
        if !self.permisos_pedidos && self.inicio.elapsed() >= Duration::from_millis(1500) {
            self.permisos_pedidos = true;
            // al abrir solo el micrófono; el de los archivos se pide al buscar un sonido en el teléfono
            let r = audio::pedir_permisos(&["android.permission.RECORD_AUDIO"]);
            if r.starts_with("error") {
                self.msg = format!("No se pudieron pedir los permisos: {r}");
            } else {
                self.mic_esperando = Some(Instant::now());
            }
        }
        // sin el permiso del micrófono la app no sigue
        #[cfg(target_os = "android")]
        if let Some(t0) = self.mic_esperando {
            ctx.request_repaint_after(Duration::from_millis(400));
            if audio::permiso_concedido("android.permission.RECORD_AUDIO") {
                self.mic_esperando = None;
            } else if t0.elapsed() > Duration::from_millis(1200) && audio::ventana_con_foco() {
                // el aviso ya se cerró y no se concedió
                audio::cerrar_app();
            }
        }
        let inp = f32::from_bits(self.status.input.swap(0, Ordering::Relaxed));
        self.in_meter = inp.max(self.in_meter * 0.9);
        self.rec_update();
        if let Some(r) = &self.rec {
            if r.started.elapsed().as_secs_f32() >= r.max_s + 0.1 {
                self.rec_stop();
            }
        }
        if self.rec.is_some() || self.tune_busy > 0 {
            ctx.request_repaint_after(Duration::from_millis(33));
        }
        self.shortcuts(&ctx);
        self.run_autotest(&ctx);
        // vúmetros con caída suave
        for i in 0..self.p.channels.len().min(self.meters.len()) {
            let pk = self.status.meter(i);
            self.meters[i] = pk.max(self.meters[i] * 0.88);
        }
        for k in 0..2 {
            let g = f32::from_bits(self.status.gr[k].load(Ordering::Relaxed)).max(0.0);
            self.gr_hold[k] = g.max(self.gr_hold[k] * 0.93);
        }
        let mp = self.status.master();
        for k in 0..2 {
            self.mmeter[k] = mp[k].max(self.mmeter[k] * 0.88);
        }
        // mientras suena: 30 cuadros por segundo (el cabezal y los vúmetros se ven fluidos y gasta la mitad que a 60)
        if self.playing() || self.job.is_some() || self.meters.iter().any(|m| *m > 0.001) {
            ctx.request_repaint_after(Duration::from_millis(33));
        }
        let compact = self.plat.mobile || ui.available_width() < 900.0;

        egui::Panel::top("top").frame(Frame::new().fill(BG).inner_margin(Margin::symmetric(10, 7))).show(ui, |ui| {
            // barra de arriba en forma de píldora, gris muy oscuro con borde sutil
            Frame::new().fill(Color32::from_gray(20)).stroke(Stroke::new(1.0, Color32::from_gray(38))).corner_radius(99).inner_margin(Margin::symmetric(14, 4)).show(ui, |ui| self.top_bar(ui, compact));
        });
        egui::Panel::bottom("status").frame(Frame::new().fill(Color32::from_gray(15)).inner_margin(Margin::symmetric(4, 3))).show(ui, |ui| self.status_bar(ui));
        // en el teléfono el micrófono usa toda la pantalla y el masterizado va a la derecha: uno a la vez
        // (si quedan los dos abiertos, se cierra el que ya estaba y queda el que acabas de abrir)
        if compact && self.mic_panel && self.mast_panel {
            if self.mic_antes_que_mast {
                self.mic_panel = false;
            } else {
                self.mast_panel = false;
            }
        }
        self.mic_antes_que_mast = self.mic_panel;
        // el masterizado va a la derecha y primero (todo el alto); en la PC el micrófono queda abajo, a su lado
        {
            let mut ver_mast = self.mast_panel;
            let (def, min) = if compact { (300.0, 260.0) } else { (330.0, 290.0) };
            egui::Panel::right("masterizado").default_size(def).min_size(min).frame(Frame::new().fill(PANEL).inner_margin(Margin::same(if compact { 10 } else { 12 })).stroke(Stroke::new(1.0, LINE))).show_collapsible(ui, &mut ver_mast, |ui| self.panel_masterizado(ui));
        }
        if !compact {
            let mut abierto = self.mic_panel;
            egui::Panel::bottom("micro").frame(Frame::new().fill(PANEL).inner_margin(Margin::symmetric(14, 10)).stroke(Stroke::new(1.0, LINE))).show_collapsible(ui, &mut abierto, |ui| {
                // si no entra a lo ancho, se desliza de lado en vez de montarse
                egui::ScrollArea::horizontal().id_salt("micro_pc").auto_shrink([false, true]).show(ui, |ui| self.micro_panel(ui));
            });
            self.mic_panel = abierto && self.mic_panel;
        }
        let mut ver_letras = self.letras_panel;
        egui::Panel::right("letras").default_size(300.0).frame(Frame::new().fill(PANEL).inner_margin(Margin::same(12)).stroke(Stroke::new(1.0, LINE))).show_collapsible(ui, &mut ver_letras, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Letras y notas").strong().size(14.0));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if icon_button(ui, Ic::Close, Vec2::new(26.0, 24.0), false, None).on_hover_text("Cerrar").clicked() {
                        self.letras_panel = false;
                    }
                });
            });
            ui.add_space(6.0);
            let antes = self.p.letras.len();
            egui::ScrollArea::vertical().show(ui, |ui| {
                let r = ui.add_sized([ui.available_width(), (ui.available_height()).max(200.0)], egui::TextEdit::multiline(&mut self.p.letras).hint_text("Escribe aquí la letra, ideas o notas…").font(FontId::proportional(14.5)).frame(Frame::NONE));
                if r.gained_focus() {
                    // punto de deshacer antes de escribir: así deshacer otro cambio no borra la letra nueva
                    self.snap();
                }
                if r.changed() || self.p.letras.len() != antes {
                    self.cambios = true;
                }
            });
        });
        self.letras_panel = ver_letras && self.letras_panel;
        if compact && self.mic_panel {
            // en el teléfono el micrófono y NN Tune usan toda la pantalla (se desplaza con el dedo)
            egui::CentralPanel::default().frame(Frame::new().fill(PANEL).inner_margin(Margin::symmetric(10, 8))).show(ui, |ui| {
                egui::ScrollArea::both().id_salt("micro_movil").auto_shrink([false, false]).show(ui, |ui| self.micro_panel(ui));
            });
        } else if compact {
            egui::Panel::top("tabs").frame(Frame::new().fill(Color32::from_gray(18)).inner_margin(Margin::symmetric(0, 4))).show(ui, |ui| self.view_tabs(ui, true));
            let v = self.view;
            egui::CentralPanel::default().frame(Frame::new().fill(PANEL)).show(ui, |ui| self.show_view(ui, v));
        } else {
            if self.show_playlist {
                egui::Panel::top("playlist").resizable(true).default_size(ui.available_height() * 0.4).min_size(120.0).frame(Frame::new().fill(PANEL)).show(ui, |ui| self.playlist(ui));
            }
            egui::Panel::top("docktabs").frame(Frame::new().fill(Color32::from_gray(18)).inner_margin(Margin::symmetric(0, 4))).show(ui, |ui| self.view_tabs(ui, false));
            let v = if self.view == View::Playlist { View::Rack } else { self.view };
            egui::CentralPanel::default().frame(Frame::new().fill(PANEL)).show(ui, |ui| self.show_view(ui, v));
        }

        self.plugin_windows(&ctx);
        self.playlist_menu(&ctx);
        self.dialogo_guardar(&ctx);
        self.dialogo_rescate(&ctx);
        self.selector_audio(&ctx);
        if ctx.input(|i| i.viewport().close_requested()) && !self.cerrar_ok && self.cambios && self.settings.preguntar && self.autotest.is_none() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.pendiente = Some(Pend::Cerrar);
        }
        let nombre = self.proj_path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().trim_end_matches(".json").trim_end_matches(".nns").to_string()).unwrap_or_else(|| self.p.name.clone());
        let titulo = format!("{}{} - NoName Studio", nombre, if self.cambios { " *" } else { "" });
        if titulo != self.titulo {
            self.titulo = titulo.clone();
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(titulo));
        }
        // tamaño de la interfaz: se revisa en cada cuadro porque Android lo reinicia al volver a la app
        // en el teléfono la escala nativa (≈2,75) deja todo enorme: se usa el 60 %
        let base = if self.plat.mobile { 0.6 } else { 1.0 };
        let deseado = (self.settings.zoom * base).clamp(0.5, 2.0);
        if (ctx.zoom_factor() - deseado).abs() > 0.001 {
            self.zoom_puesto = self.settings.zoom;
            ctx.set_zoom_factor(deseado);
        }
        // primero se contesta si se recupera el trabajo; el tutorial espera
        if !self.rescate {
            self.tutorial(&ctx);
        }

        // abrir proyecto (Android)
        if let Some(list) = self.open_list.clone() {
            let mut open = true;
            let mut pick = None;
            egui::Window::new("Abrir proyecto").open(&mut open).collapsible(false).anchor(Align2::CENTER_CENTER, Vec2::ZERO).show(&ctx, |ui| {
                if list.is_empty() {
                    ui.label(format!("No hay proyectos guardados en\n{}", self.plat.export_dir.display()));
                }
                for p in &list {
                    if ui.button(p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()).clicked() {
                        pick = Some(p.clone());
                    }
                }
            });
            if let Some(p) = pick {
                self.open_path(p);
                open = false;
            }
            if !open {
                self.open_list = None;
            }
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // no perder una toma si se cierra mientras se graba
        if self.rec_track.is_some() {
            self.rec_track_stop();
        }
        if self.rec.is_some() {
            self.rec_stop();
        }
        self.plugins_store_state();
        self.save_settings();
        // cierre normal: la copia de rescate ya no hace falta (si no se contestó la pregunta, se guarda para la próxima)
        if !self.rescate {
            self.borrar_rescate();
        }
        if !self.settings.abrir_ultimo {
            if let Some(j) = &self.job {
                j.cancel.store(true, Ordering::Relaxed);
            }
            return;
        }
        if let Ok(s) = serde_json::to_string(&self.p) {
            let _ = std::fs::write(self.plat.data_dir.join("autosave.json"), s);
        }
        if let Some(j) = &self.job {
            j.cancel.store(true, Ordering::Relaxed);
        }
    }
}

#[derive(Clone, Copy)]
enum Ic {
    Play,
    Pause,
    Stop,
    Metro,
    Gear,
    Close,
    Undo,
    Rec,
}

/// Botón con icono vectorial (no depende de fuentes; se ve igual en Windows y Android).
fn icon_button(ui: &mut Ui, ic: Ic, size: Vec2, selected: bool, fill: Option<Color32>) -> Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let t = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.12);
    let base = fill.unwrap_or(if selected { ACCENT } else { PANEL2 });
    let hover = if fill.is_some() || selected { base.gamma_multiply(1.15) } else { Color32::from_gray(56) };
    let bg = lerp_col(base, hover, t);
    let p = ui.painter();
    let r = rect.expand(t * 0.8);
    p.rect_filled(r, size.y * 0.5, bg);
    // sobre fondo claro (plateado) el ícono va oscuro; sobre fondo oscuro, blanco
    let claro = (bg.r() as u32 * 299 + bg.g() as u32 * 587 + bg.b() as u32 * 114) / 1000 > 150;
    let col = if claro { Color32::from_gray(20) } else if selected || fill.is_some() { Color32::WHITE } else { lerp_col(Color32::from_gray(220), Color32::WHITE, t) };
    draw_icon(p, ic, rect.center(), rect.height().min(rect.width()) * 0.3, col);
    resp
}

/// Mezcla de dos colores (t = 0 → a, t = 1 → b).
fn lerp_col(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_unmultiplied(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()), m(a.a(), b.a()))
}

fn draw_icon(p: &egui::Painter, ic: Ic, c: Pos2, s: f32, col: Color32) {
    let st = Stroke::new(1.8, col);
    match ic {
        Ic::Play => {
            p.add(egui::Shape::convex_polygon(vec![c + Vec2::new(-s * 0.7, -s), c + Vec2::new(s, 0.0), c + Vec2::new(-s * 0.7, s)], col, Stroke::NONE));
        }
        Ic::Pause => {
            p.rect_filled(Rect::from_center_size(c - Vec2::new(s * 0.45, 0.0), Vec2::new(s * 0.55, s * 1.9)), 1, col);
            p.rect_filled(Rect::from_center_size(c + Vec2::new(s * 0.45, 0.0), Vec2::new(s * 0.55, s * 1.9)), 1, col);
        }
        Ic::Stop => {
            p.rect_filled(Rect::from_center_size(c, Vec2::splat(s * 1.7)), 2, col);
        }
        Ic::Metro => {
            let pts = vec![c + Vec2::new(-s * 0.45, -s), c + Vec2::new(s * 0.45, -s), c + Vec2::new(s * 0.85, s), c + Vec2::new(-s * 0.85, s), c + Vec2::new(-s * 0.45, -s)];
            p.add(egui::Shape::line(pts, st));
            p.line_segment([c + Vec2::new(0.0, s * 0.6), c + Vec2::new(s * 0.55, -s * 0.9)], st);
        }
        Ic::Gear => {
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::TAU / 8.0;
                let d = Vec2::new(a.cos(), a.sin());
                p.line_segment([c + d * s * 0.7, c + d * s * 1.05], Stroke::new(2.2, col));
            }
            p.circle_stroke(c, s * 0.62, st);
            p.circle_filled(c, s * 0.22, col);
        }
        Ic::Rec => {
            p.circle_filled(c, s * 0.75, Color32::from_rgb(220, 60, 60));
        }
        Ic::Close => {
            p.line_segment([c + Vec2::new(-s * 0.7, -s * 0.7), c + Vec2::new(s * 0.7, s * 0.7)], st);
            p.line_segment([c + Vec2::new(-s * 0.7, s * 0.7), c + Vec2::new(s * 0.7, -s * 0.7)], st);
        }
        Ic::Undo => {
            let pts: Vec<Pos2> = (0..=20)
                .map(|i| {
                    let a = std::f32::consts::PI * (1.0 + 1.25 * i as f32 / 20.0);
                    c + Vec2::new(a.cos(), a.sin()) * s * 0.85 + Vec2::new(0.0, s * 0.2)
                })
                .collect();
            let tip = pts[0];
            p.add(egui::Shape::line(pts, st));
            p.add(egui::Shape::convex_polygon(vec![tip + Vec2::new(-s * 0.45, -s * 0.1), tip + Vec2::new(s * 0.45, -s * 0.1), tip + Vec2::new(0.0, s * 0.55)], col, Stroke::NONE));
        }
    }
}

fn pan_txt(p: f32) -> String {
    if p.abs() < 0.01 { "C".into() } else if p < 0.0 { format!("I{:.0}", -p * 100.0) } else { format!("D{:.0}", p * 100.0) }
}

/// Perilla giratoria: arrastrar arriba/abajo, doble clic para valor por defecto.
fn knob(ui: &mut Ui, val: &mut f32, min: f32, max: f32, def: f32, label: &str, fmt: impl Fn(f32) -> String) -> Response {
    ui.allocate_ui_with_layout(Vec2::new(46.0, 66.0), Layout::top_down(Align::Center), |ui| {
        ui.set_width(46.0);
        ui.label(RichText::new(label).size(9.5).color(DIM));
        let (rect, mut resp) = ui.allocate_exact_size(Vec2::splat(30.0), Sense::click_and_drag());
        if resp.dragged() {
            let d = resp.drag_delta();
            let nv = (*val + (d.x - d.y) * (max - min) / 160.0).clamp(min, max);
            if nv != *val {
                *val = nv;
                resp.mark_changed();
            }
        }
        if resp.double_clicked() {
            *val = def;
            resp.mark_changed();
        }
        let p = ui.painter();
        let c = rect.center();
        let r = 12.0;
        p.circle_filled(c, r, Color32::from_gray(19));
        let a0 = std::f32::consts::PI * 0.75;
        let span = std::f32::consts::PI * 1.5;
        let frac = ((*val - min) / (max - min)).clamp(0.0, 1.0);
        let arc = |from: f32, to: f32, col: Color32| {
            let n = 24;
            let pts: Vec<Pos2> = (0..=n).map(|i| {
                let a = from + (to - from) * i as f32 / n as f32;
                c + Vec2::new(a.cos(), a.sin()) * (r - 1.5)
            }).collect();
            p.add(egui::Shape::line(pts, Stroke::new(3.0, col)));
        };
        arc(a0, a0 + span, Color32::from_gray(43));
        // los controles bipolares se pintan desde el centro
        let zero = if min < 0.0 && max > 0.0 { (0.0 - min) / (max - min) } else { 0.0 };
        let (fa, fb) = if frac >= zero { (zero, frac) } else { (frac, zero) };
        if fb - fa > 0.002 {
            arc(a0 + span * fa, a0 + span * fb, ACCENT);
        }
        let a = a0 + span * frac;
        p.line_segment([c, c + Vec2::new(a.cos(), a.sin()) * (r - 4.0)], Stroke::new(2.0, Color32::WHITE));
        ui.label(RichText::new(fmt(*val)).size(9.5).color(ACCENT2));
        resp.on_hover_text(format!("{label} — arrastra para ajustar, doble clic para restablecer"))
    })
    .inner
}

fn meter(ui: &mut Ui, size: Vec2, peak: f32) {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 3, Color32::from_gray(10));
    let db = if peak > 0.0 { 20.0 * peak.log10() } else { -99.0 };
    let f = ((db + 48.0) / 48.0).clamp(0.0, 1.0);
    if f > 0.0 {
        let top = rect.bottom() - rect.height() * f;
        // degradado verde → amarillo → rojo por tramos
        let segs = [(0.0, 0.62, Color32::from_rgb(95, 200, 95)), (0.62, 0.82, Color32::from_rgb(200, 210, 74)), (0.82, 1.0, Color32::from_rgb(224, 90, 74))];
        for (a, b, col) in segs {
            let y0 = rect.bottom() - rect.height() * a;
            let y1 = (rect.bottom() - rect.height() * b).max(top);
            if y1 < y0 {
                p.rect_filled(Rect::from_min_max(Pos2::new(rect.left(), y1), Pos2::new(rect.right(), y0)), 1, col);
            }
        }
    }
}

fn draw_pattern_preview(p: &egui::Painter, r: Rect, pat: &Pattern) {
    let steps = pat.len_steps() as f32;
    let sw = r.width() / steps;
    let col = lerp_col(c32(pat.color), Color32::WHITE, 0.25);
    let all: Vec<&Note> = pat.notes.values().flatten().collect();
    if !all.is_empty() {
        let lo = all.iter().map(|n| n.m).min().unwrap() as f32;
        let hi = all.iter().map(|n| n.m).max().unwrap() as f32;
        let rg = (hi - lo).max(6.0);
        for n in all {
            let y = r.bottom() - 3.0 - (n.m as f32 - lo) / rg * (r.height() - 6.0);
            p.rect_filled(Rect::from_min_size(Pos2::new(r.left() + n.s as f32 * sw, y - 1.0), Vec2::new((n.l as f32 * sw - 1.0).max(2.0), 3.0)), 1, col);
        }
    } else {
        let filas = pat.steps.len().max(1) as f32;
        let alto = ((r.height() - 4.0) / filas).clamp(2.0, 6.0);
        for (row, a) in pat.steps.values().enumerate() {
            for (i, v) in a.iter().enumerate() {
                if *v > 0.0 {
                    p.rect_filled(Rect::from_min_size(Pos2::new(r.left() + i as f32 * sw, r.top() + 2.0 + row as f32 * (alto + 1.0)), Vec2::new((sw - 1.0).max(2.0), alto)), 1, col.gamma_multiply(0.5 + 0.5 * v));
                }
            }
        }
    }
}

fn load_logo(ctx: &egui::Context) -> Option<egui::TextureHandle> {
    // el diamante de NoName Studio, con su fondo transparente, tal cual
    let img = image::load_from_memory(crate::ICON_PNG).ok()?.to_rgba8();
    let img = image::imageops::resize(&img, 128, 128, image::imageops::FilterType::Lanczos3);
    let ci = egui::ColorImage::from_rgba_unmultiplied([128, 128], &img.into_raw());
    Some(ctx.load_texture("logo", ci, egui::TextureOptions::LINEAR))
}

fn setup_style(ctx: &egui::Context, mobile: bool) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = PANEL;
    v.faint_bg_color = PANEL;
    v.extreme_bg_color = Color32::from_gray(13);
    // lo elegido: píldora blanca con letra oscura
    v.selection.bg_fill = Color32::from_gray(244);
    v.selection.stroke = Stroke::new(1.0, Color32::from_gray(21));
    v.hyperlink_color = ACCENT2;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, Color32::from_gray(213));
    v.widgets.inactive.bg_fill = PANEL2;
    v.widgets.inactive.weak_bg_fill = PANEL2;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, Color32::from_gray(37));
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, Color32::from_gray(213));
    v.widgets.hovered.weak_bg_fill = Color32::from_gray(40);
    v.widgets.hovered.bg_fill = Color32::from_gray(40);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_gray(85));
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    v.widgets.hovered.expansion = 0.5;
    v.widgets.active.weak_bg_fill = Color32::from_gray(65);
    v.widgets.active.bg_fill = Color32::from_gray(65);
    v.widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    v.widgets.open.weak_bg_fill = Color32::from_gray(40);
    // botones, menús y campos en forma de píldora
    for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
        w.corner_radius = CornerRadius::same(99);
    }
    v.widgets.noninteractive.corner_radius = CornerRadius::same(10);
    v.window_corner_radius = CornerRadius::same(16);
    v.menu_corner_radius = CornerRadius::same(14);
    v.window_stroke = Stroke::new(1.0, LINE);
    v.window_shadow = egui::Shadow { offset: [0, 10], blur: 28, spread: 0, color: Color32::from_black_alpha(150) };
    v.popup_shadow = egui::Shadow { offset: [0, 6], blur: 18, spread: 0, color: Color32::from_black_alpha(140) };
    v.slider_trailing_fill = true;
    ctx.set_visuals(v);
    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = Vec2::new(7.0, 5.0);
        s.spacing.button_padding = Vec2::new(11.0, 5.0);
        s.spacing.slider_rail_height = 5.0;
        // transiciones suaves (paneles, hover, menús)
        s.animation_time = 0.16;
        if mobile {
            s.spacing.interact_size.y = 30.0;
        }
    });
}

// ============================ metal líquido ============================

/// Contorno de una píldora (rectángulo con extremos redondos) como puntos, en orden.
fn contorno_pildora(r: Rect, n: usize) -> Vec<Pos2> {
    let rad = (r.height() * 0.5).min(r.width() * 0.5);
    let (cd, ci) = (Pos2::new(r.right() - rad, r.center().y), Pos2::new(r.left() + rad, r.center().y));
    let mut v = Vec::with_capacity(2 * n + 2);
    for i in 0..=n {
        let a = -std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * i as f32 / n as f32;
        v.push(cd + Vec2::new(a.cos(), a.sin()) * rad);
    }
    for i in 0..=n {
        let a = std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * i as f32 / n as f32;
        v.push(ci + Vec2::new(a.cos(), a.sin()) * rad);
    }
    v
}

/// Píldora rellena con un color por punto (abanico desde el centro).
fn pildora_mesh(p: &egui::Painter, r: Rect, centro: Color32, color: impl Fn(Pos2) -> Color32) {
    let pts = contorno_pildora(r, 20);
    let mut m = egui::Mesh::default();
    m.colored_vertex(r.center(), centro);
    for q in &pts {
        m.colored_vertex(*q, color(*q));
    }
    let n = pts.len() as u32;
    for i in 0..n {
        m.add_triangle(0, 1 + i, 1 + (i + 1) % n);
    }
    p.add(egui::Shape::mesh(m));
}

/// Brillo del metal líquido en un punto: bandas diagonales que se mueven con el tiempo.
fn brillo_metal(q: Pos2, r: Rect, t: f32) -> Color32 {
    let u = ((q.x - r.left()) + (q.y - r.top()) * 1.6) / r.width().max(1.0);
    let b = 0.5 + 0.5 * (u * std::f32::consts::TAU * 1.6 - t * 1.7).sin();
    let b2 = 0.5 + 0.5 * (u * std::f32::consts::TAU * 3.3 + t * 1.1 + 1.3).sin();
    let v = (b * 0.7 + b2 * 0.3).powf(1.6);
    let g = 70.0 + 185.0 * v;
    // un toque de rojo y azul en los brillos, como el reflejo del metal
    let rojo = if v > 0.65 { 14.0 * (v - 0.65) / 0.35 } else { 0.0 };
    let azul = if v < 0.35 { 12.0 * (0.35 - v) / 0.35 } else { 0.0 };
    Color32::from_rgb((g + rojo).min(255.0) as u8, g as u8, (g + azul).min(255.0) as u8)
}

/// Botón principal de metal líquido: borde cromado animado, interior negro y onda al hacer clic.
fn boton_metal(ui: &mut Ui, texto: &str, tam: Vec2) -> Response {
    let (r, resp) = ui.allocate_exact_size(tam, Sense::click());
    if !ui.is_rect_visible(r) {
        return resp;
    }
    let ctx = ui.ctx().clone();
    let ahora = ui.input(|i| i.time);
    let hover = ctx.animate_bool_with_time(resp.id.with("h"), resp.hovered(), 0.25);
    // el metal corre más rápido al pasar el ratón y da un empujón al hacer clic
    let id_clic = resp.id.with("clic");
    if resp.clicked() {
        let pos = resp.interact_pointer_pos().unwrap_or(r.center());
        ctx.data_mut(|d| d.insert_temp(id_clic, (ahora, pos)));
    }
    let clic: Option<(f64, Pos2)> = ctx.data(|d| d.get_temp(id_clic));
    let empuje = clic.map(|(t0, _)| (1.0 - ((ahora - t0) / 0.3) as f32).clamp(0.0, 1.0)).unwrap_or(0.0);
    let id_fase = resp.id.with("fase");
    let (fase_ant, t_ant): (f32, f64) = ctx.data(|d| d.get_temp(id_fase)).unwrap_or((0.0, ahora));
    let vel = 1.0 * hover + 2.4 * empuje;
    let fase = fase_ant + ((ahora - t_ant) as f32).min(0.1) * vel;
    ctx.data_mut(|d| d.insert_temp(id_fase, (fase, ahora)));
    let apretado = resp.is_pointer_button_down_on();
    let r = if apretado { r.shrink(1.0).translate(Vec2::new(0.0, 1.0)) } else { r };
    let p = ui.painter();
    // sombra suave
    p.rect_filled(r.translate(Vec2::new(0.0, 3.0)).expand(1.0), 99, Color32::from_black_alpha(if apretado { 40 } else { 90 }));
    // borde de metal y el interior negro con degradado
    pildora_mesh(p, r, Color32::from_gray(150), |q| brillo_metal(q, r, fase));
    let dentro = r.shrink(2.5);
    let (arriba, alto) = (dentro.top(), dentro.height().max(1.0));
    pildora_mesh(p, dentro, Color32::from_gray(14), |q| lerp_col(Color32::from_gray(34), Color32::from_gray(0), ((q.y - arriba) / alto).clamp(0.0, 1.0)));
    // onda del clic
    if let Some((t0, pos)) = clic {
        let k = ((ahora - t0) / 0.6) as f32;
        if k < 1.0 {
            let pc = p.with_clip_rect(dentro);
            pc.circle_filled(pos, 10.0 + 50.0 * k, Color32::from_white_alpha((70.0 * (1.0 - k)) as u8));
        }
    }
    let tinta = lerp_col(Color32::from_gray(176), Color32::WHITE, hover);
    p.text(r.center() + Vec2::new(0.0, 0.5), Align2::CENTER_CENTER, texto, FontId::proportional(14.0), tinta);
    let activo = hover > 0.0 || empuje > 0.0 || apretado || clic.is_some_and(|(t0, _)| ahora - t0 < 0.7);
    if activo {
        ctx.request_repaint_after(Duration::from_millis(16));
    }
    // en reposo el metal queda quieto: redibujar toda la ventana solo para moverlo gastaría procesador
    resp
}

/// Pestaña con el estilo metálico: la elegida en degradado plateado, las otras gris oscuro.
fn pestana_metal(ui: &mut Ui, sel: bool, texto: &str, ancho_min: f32) -> Response {
    let galley = ui.painter().layout_no_wrap(texto.to_string(), FontId::proportional(13.5), Color32::WHITE);
    let tam = Vec2::new((galley.size().x + 28.0).max(ancho_min), 30.0);
    let (r, resp) = ui.allocate_exact_size(tam, Sense::click());
    let t = ui.ctx().animate_bool_with_time(resp.id.with("sel"), sel, 0.3);
    let p = ui.painter();
    let base = if resp.hovered() { Color32::from_gray(40) } else { Color32::from_gray(30) };
    p.rect_filled(r, 99, base);
    if t > 0.01 {
        // degradado plateado en diagonal (claro arriba a la izquierda, oscuro abajo a la derecha)
        let w = r.width() + r.height();
        pildora_mesh(p, r, Color32::from_gray(130).gamma_multiply(t), |q| {
            let u = (((q.x - r.left()) + (q.y - r.top())) / w).clamp(0.0, 1.0);
            let g = if u < 0.5 { 180.0 - 120.0 * u } else { 120.0 - 80.0 * (u - 0.5) };
            Color32::from_gray(g as u8).gamma_multiply(t)
        });
    }
    p.text(r.center(), Align2::CENTER_CENTER, texto, FontId::proportional(13.5), lerp_col(Color32::from_gray(200), Color32::WHITE, t));
    resp
}

/// Contenido de un menú que se puede deslizar (con el dedo o la rueda) si no entra en la pantalla.
fn desplazable<R>(ui: &mut Ui, f: impl FnOnce(&mut Ui) -> R) -> R {
    let alto = (ui.ctx().content_rect().height() - 70.0).max(140.0);
    egui::ScrollArea::vertical().max_height(alto).auto_shrink([true, true]).show(ui, f).inner
}
