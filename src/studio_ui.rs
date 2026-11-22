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
}
impl Default for Settings {
    fn default() -> Self {
        Settings { latency_ms: 20.0, version_min: 5, plugin_dirs: vec![], fav_dirs: vec![], midi_port: String::new(), midi_quant: true }
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
}

pub(super) enum FileMsg {
    Built(String, Result<Buf, String>),
    Preview(Result<Buf, String>),
}

pub(super) type MasterRes = Result<(f32, f32, f32), String>;

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
            } else if ext == "json" {
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
        let step = (((p.x - org.x) / BW * STEPS_PER_BAR as f32).max(0.0)) as u32;
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
            if ui.button("+ Pista").clicked() {
                self.snap();
                let n = self.p.tracks.len() as u32 + 1;
                self.ensure_tracks(n);
                self.send();
            }
            if ui.button("Importar audio…").clicked() {
                if let Some(p) = self.pick_audio() {
                    let t = self.free_track(self.play_from, 16);
                    self.import_to_playlist(&p, t, self.play_from);
                } else if self.plat.mobile {
                    self.left_tab = 1;
                }
            }
            ui.label(RichText::new("Clic: patrón · arrastrar: mover · borde: largo · clic derecho: menú · regla: tocar / arrastrar: bucle").small().color(DIM));
        });
    }

    pub(super) fn playlist_canvas(&mut self, ui: &mut Ui) {
        let song = self.p.song_bars();
        let bars = (song + 8).max(32);
        let (tn, rul) = (104.0, 22.0);
        let ntr = self.p.tracks.len();
        let size = Vec2::new(tn + bars as f32 * BW, rul + ntr as f32 * TH);
        let (rect, resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        let org = rect.min + Vec2::new(tn, rul);
        self.pl_org = org;
        self.pl_rect = Some(ui.clip_rect().intersect(rect));
        let spx = BW / STEPS_PER_BAR as f32;
        for t in 0..ntr {
            let r = Rect::from_min_size(Pos2::new(org.x, org.y + t as f32 * TH), Vec2::new(bars as f32 * BW, TH));
            painter.rect_filled(r, 0, if t % 2 == 0 { Color32::from_rgb(38, 42, 48) } else { Color32::from_rgb(33, 37, 42) });
            let nr = Rect::from_min_size(Pos2::new(rect.left(), r.top()), Vec2::new(tn, TH));
            painter.rect_filled(nr, 0, Color32::from_rgb(34, 38, 44));
            painter.text(Pos2::new(nr.left() + 8.0, nr.center().y), Align2::LEFT_CENTER, &self.p.tracks[t], FontId::proportional(13.0), DIM);
            painter.hline(rect.left()..=rect.right(), r.bottom() - 0.5, Stroke::new(1.0, Color32::from_rgb(24, 27, 31)));
        }
        painter.rect_filled(Rect::from_min_size(rect.min, Vec2::new(rect.width(), rul)), 0, Color32::from_rgb(28, 31, 36));
        // región de bucle / punch
        if self.p.loop_b > self.p.loop_a {
            let x0 = org.x + self.p.loop_a as f32 * BW;
            let x1 = org.x + self.p.loop_b as f32 * BW;
            let on = self.p.loop_on || self.p.punch;
            let col = if self.p.punch { Color32::from_rgb(200, 70, 70) } else { ACCENT };
            painter.rect_filled(Rect::from_min_max(Pos2::new(x0, rect.top() + 2.0), Pos2::new(x1, rect.top() + rul - 2.0)), 3, col.gamma_multiply(if on { 0.55 } else { 0.18 }));
            if on {
                painter.rect_filled(Rect::from_min_max(Pos2::new(x0, org.y), Pos2::new(x1, rect.bottom())), 0, col.gamma_multiply(0.06));
            }
        }
        for b in 0..=bars {
            let x = org.x + b as f32 * BW;
            painter.vline(x, org.y..=rect.bottom(), Stroke::new(1.0, if b % 4 == 0 { Color32::from_rgb(20, 22, 26) } else { Color32::from_rgb(28, 31, 35) }));
            if b < bars {
                painter.text(Pos2::new(x + 4.0, rect.top() + 11.0), Align2::LEFT_CENTER, format!("{}", b + 1), FontId::proportional(11.0), if b < song { DIM } else { Color32::from_rgb(80, 88, 98) });
            }
        }
        let pf = org.x + self.play_from as f32 * spx;
        painter.add(egui::Shape::convex_polygon(vec![Pos2::new(pf - 5.0, rect.top() + rul - 8.0), Pos2::new(pf + 5.0, rect.top() + rul - 8.0), Pos2::new(pf, rect.top() + rul)], ACCENT2, Stroke::NONE));
        // clips de patrón
        let pat_rect = |cl: &Clip, p: &Pattern| Rect::from_min_size(Pos2::new(org.x + cl.bar as f32 * BW + 1.0, org.y + cl.track as f32 * TH + 2.0), Vec2::new(p.bars as f32 * BW - 2.0, TH - 4.0));
        let step_rect = |track: u32, start: u32, len: u32| Rect::from_min_size(Pos2::new(org.x + start as f32 * spx + 1.0, org.y + track as f32 * TH + 2.0), Vec2::new((len as f32 * spx - 2.0).max(4.0), TH - 4.0));
        for cl in &self.p.playlist {
            let Some(p) = self.p.pattern(cl.pat) else { continue };
            let r = pat_rect(cl, p);
            painter.rect(r, 4, c32(p.color), Stroke::new(1.0, Color32::from_black_alpha(60)), StrokeKind::Inside);
            painter.text(r.left_top() + Vec2::new(5.0, 3.0), Align2::LEFT_TOP, &p.name, FontId::proportional(11.0), Color32::from_rgb(17, 17, 17));
            draw_pattern_preview(&painter, Rect::from_min_max(Pos2::new(r.left(), r.top() + 16.0), r.right_bottom() - Vec2::new(0.0, 2.0)), p);
        }
        // clips de audio
        let ss = self.p.step_secs() as f32;
        for cl in &self.p.audio_clips {
            let r = step_rect(cl.track, cl.start, cl.len);
            let chn = self.p.channel(cl.ch);
            let col = Color32::from_rgb(64, 120, 150);
            painter.rect(r, 4, col, Stroke::new(1.0, Color32::from_black_alpha(80)), StrokeKind::Inside);
            let file = cl.file().to_string();
            if let Some((pk, secs)) = self.wave_peaks.get(&file) {
                let ratio = cl.ratio(self.p.bpm);
                let mid = r.center().y + 6.0;
                let h = (r.height() - 18.0) / 2.0;
                let n = pk.len() as f32;
                let mut x = r.left();
                while x < r.right() {
                    let t_clip = (x - r.left()) / spx * ss; // segundos en la línea de tiempo
                    let src = cl.off + t_clip / ratio;
                    let src = if cl.reverse { secs - src } else { src };
                    if src >= 0.0 && src < *secs {
                        let v = pk[((src / secs * n) as usize).min(pk.len() - 1)] * cl.gain.min(2.0);
                        painter.vline(x, (mid - v * h)..=(mid + v * h), Stroke::new(1.0, Color32::from_rgb(190, 225, 240)));
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
            painter.with_clip_rect(r.intersect(painter.clip_rect())).text(r.left_top() + Vec2::new(5.0, 2.0), Align2::LEFT_TOP, label, FontId::proportional(10.5), Color32::from_rgb(12, 20, 26));
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
            let track = self.p.audio_clips.iter().find(|c| c.ch == tr.ch).map(|c| c.track).unwrap_or(self.free_track(tr.from, 16));
            let w = if tr.looped { tr.region.map(|r| (r.1 - r.0) as f32).unwrap_or(16.0) } else { (now as f32 - s0).max(0.5) };
            let r = Rect::from_min_size(Pos2::new(org.x + s0 * spx, org.y + track as f32 * TH + 2.0), Vec2::new(w * spx, TH - 4.0));
            painter.rect(r, 4, RED.gamma_multiply(0.35), Stroke::new(1.5, RED), StrokeKind::Inside);
            painter.text(r.left_top() + Vec2::new(5.0, 3.0), Align2::LEFT_TOP, "GRABANDO", FontId::proportional(11.0), Color32::WHITE);
        }
        // automatización
        for a in &self.p.autos {
            let r = step_rect(a.track, a.start, a.len);
            painter.rect(r, 4, Color32::from_rgb(52, 46, 70), Stroke::new(1.0, Color32::from_rgb(130, 110, 190)), StrokeKind::Inside);
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
            let x = org.x + (self.status.pos() / 16.0) as f32 * BW;
            painter.vline(x, rect.top()..=rect.bottom(), Stroke::new(2.0, ACCENT2));
        }

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
                            let b = ((pos.x - org.x) / BW).floor() as i32;
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
                        let b = ((pos.x - org.x) / BW).max(0.0).ceil() as u32;
                        let (x, y) = if b > *a { (*a, b) } else { (b.saturating_sub(1).min(*a), *a + 1) };
                        if self.p.loop_a != x || self.p.loop_b != y {
                            self.p.loop_a = x;
                            self.p.loop_b = y.max(x + 1);
                            changed = true;
                        }
                    }
                    PlDrag::Pat { idx, bar0, tr0, orig, moved } => {
                        let b = ((pos.x - org.x) / BW).floor() as i32;
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
                    let b = ((pos.x - org.x) / BW).floor().max(0.0) as u32;
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
                            let b = ((pos.x - org.x) / BW).floor().max(0.0) as u32;
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
        let area = egui::Area::new(egui::Id::new("plmenu")).fixed_pos(pos).order(egui::Order::Foreground).show(ctx, |ui| {
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
        });
        let outside = ctx.input(|i| i.pointer.any_pressed()) && ctx.input(|i| i.pointer.interact_pos()).is_some_and(|p| !area.response.rect.contains(p));
        if close || (outside && opened > 1) || ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.pl_menu = None;
        }
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
        p.rect_filled(rect, 4, Color32::from_rgb(22, 24, 30));
        let bars = (a.len as f32 / 16.0).max(0.25);
        let mut b = 0.0;
        while b <= bars {
            let x = rect.left() + b / bars * rect.width();
            p.vline(x, rect.y_range(), Stroke::new(1.0, Color32::from_rgb(40, 44, 52)));
            b += if bars > 16.0 { 4.0 } else { 1.0 };
        }
        for k in 1..4 {
            p.hline(rect.x_range(), rect.top() + rect.height() * k as f32 / 4.0, Stroke::new(1.0, Color32::from_rgb(36, 40, 48)));
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
        p.rect_filled(rect, 4, Color32::from_rgb(20, 23, 27));
        if let Some((pk, _)) = self.wave_peaks.get(&sp.file) {
            let n = pk.len();
            for x in 0..rect.width() as usize {
                let i = (x as f32 / rect.width() * n as f32) as usize;
                let v = pk[i.min(n - 1)] * sp.gain.min(2.0);
                let cx = rect.left() + x as f32;
                let inside = (x as f32 / rect.width()) >= sp.start && (x as f32 / rect.width()) <= sp.end;
                let col = if inside { Color32::from_rgb(242, 170, 90) } else { Color32::from_rgb(90, 80, 70) };
                p.vline(cx, (rect.center().y - v * 60.0)..=(rect.center().y + v * 60.0), Stroke::new(1.0, col));
            }
        } else {
            p.text(rect.center(), Align2::CENTER_CENTER, if sp.file.is_empty() { "Carga un sample o arrástralo desde el navegador" } else { "cargando…" }, FontId::proportional(12.0), DIM);
        }
        let xs = |f: f32| rect.left() + f.clamp(0.0, 1.0) * rect.width();
        if sp.slice && secs > 0.0 {
            for (i, s) in sp.slices.iter().enumerate() {
                let x = xs(s / secs);
                p.vline(x, rect.y_range(), Stroke::new(1.5, Color32::from_rgb(120, 210, 255)));
                p.text(Pos2::new(x + 3.0, rect.top() + 2.0), Align2::LEFT_TOP, format!("{}", i + 1), FontId::proportional(10.0), Color32::from_rgb(120, 210, 255));
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
        match audio::record(self.status.clone()) {
            Ok(r) => {
                self.song_mode = true;
                self.play(from);
                self.rec_track = Some(TrackRec { r, ch, from, region, looped, started: Instant::now() });
                self.msg = if looped { "Grabando en bucle: cada vuelta es una toma. Espacio o Detener para terminar.".into() } else if region.is_some() { "Grabando con punch: solo se guarda la región marcada.".into() } else { "Grabando en la pista de audio… Espacio o Detener para terminar.".into() };
            }
            Err(e) => self.msg = format!("No se pudo grabar: {e}"),
        }
    }

    pub(super) fn rec_track_stop(&mut self) {
        let Some(tr) = self.rec_track.take() else { return };
        self.cmd(Cmd::Stop);
        self.status.playing.store(false, Ordering::Relaxed);
        let sr = tr.r.sr as f32;
        let data = tr.r.buf.lock().map(|b| b.clone()).unwrap_or_default();
        drop(tr.r);
        let ss = self.p.step_secs() as f32;
        let lat = ((self.settings.latency_ms / 1000.0) * sr) as usize;
        let data: Vec<f32> = data.into_iter().skip(lat).collect();
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
        for (k, (start, len, audio_data)) in pieces.into_iter().enumerate() {
            let name = format!("grabacion_{}_{}_{}.wav", tr.ch, ts, k + 1);
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
                    let track = self.free_track(start, len);
                    self.ensure_tracks(track + 1);
                    let id = self.p.new_id();
                    self.p.audio_clips.push(AudioClip { id, ch: tr.ch, track, start, len, off: 0.0, takes: vec![name], active: 0, gain: 1.0, pitch: 0.0, bpm: 0.0, fade_in: 0.0, fade_out: 0.0, reverse: false });
                }
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
        ui.menu_button("Herramientas", |ui| {
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
        });
        // arpegiador del canal
        let Some(c) = self.p.channel(ch).cloned() else { return };
        let mut arp = c.arp;
        let before = arp;
        ui.menu_button(if arp.on { "Arpegiador (activo)" } else { "Arpegiador" }, |ui| {
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
        });
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
        p.rect_filled(r, 3, Color32::from_rgb(14, 16, 18));
        let fx = |db: f32| r.left() + ((db + 36.0) / 36.0).clamp(0.0, 1.0) * r.width();
        p.rect_filled(Rect::from_min_max(r.min, Pos2::new(fx(l[1]), r.bottom())), 3, if l[1] > -9.0 { RED } else if l[1] > -16.0 { GREEN } else { Color32::from_rgb(90, 155, 224) });
        let tgt = MASTER_TARGETS[self.master_target].1;
        p.vline(fx(tgt), r.y_range(), Stroke::new(2.0, Color32::WHITE));
        ui.label(RichText::new("Reproduce la canción en SONG para medir. La línea blanca es el objetivo.").small().color(DIM));
        ui.separator();
        ui.label(RichText::new("MASTERIZAR CON UN CLIC").strong());
        egui::ComboBox::from_id_salt("mt").width(220.0).selected_text(MASTER_TARGETS[self.master_target].0).show_ui(ui, |ui| {
            for (i, (n, _)) in MASTER_TARGETS.iter().enumerate() {
                ui.selectable_value(&mut self.master_target, i, *n);
            }
        });
        let busy = self.master_job.is_some();
        if ui.add_enabled(!busy, egui::Button::new(if busy { "Analizando la canción…" } else { "Masterizar canción" }).min_size(Vec2::new(220.0, 30.0))).clicked() {
            self.start_mastering();
        }
        ui.label(RichText::new("Ajusta NN EQ, NN Dynamics y el limitador del master para llegar a la sonoridad elegida sin distorsión. Se puede deshacer.").small().color(DIM));
        if ui.button("Quitar mastering (valores neutros)").clicked() {
            self.snap();
            self.p.master_eq = default_eq();
            self.p.master_comp = CompP::master();
            self.p.limiter = LimP::default();
            self.send();
        }
    }

    pub(super) fn start_mastering(&mut self) {
        if self.p.song_bars() == 0 {
            self.msg = "La playlist está vacía: no hay nada que masterizar.".into();
            return;
        }
        let (tx, rx) = mpsc::channel();
        let mut p = self.p.clone();
        let bank = self.bank();
        let (target, style) = (MASTER_TARGETS[self.master_target].1, self.master_target);
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            // cadena base según el estilo
            p.master_eq = default_eq();
            p.master_eq[0].on = true;
            p.master_eq[0].freq = 28.0;
            match style {
                1 => {
                    p.master_eq[1].gain = 1.5;
                    p.master_eq[4].gain = 1.5;
                }
                3 => {
                    p.master_eq[4].gain = -2.5;
                    p.master_eq[1].gain = 1.0;
                }
                2 => {}
                _ => {
                    p.master_eq[4].gain = 0.8;
                }
            }
            p.master_comp = CompP { on: true, threshold: -16.0, ratio: if style == 1 { 3.0 } else { 2.0 }, attack_ms: 20.0, release_ms: 180.0, makeup: 0.0, sc: 0 };
            p.limiter = LimP { on: true, gain_db: 0.0, ceiling_db: -1.0, release_ms: 60.0 };
            let (pr, ca) = (AtomicU32::new(0), AtomicBool::new(false));
            let mut gain = 0.0f32;
            let mut lufs = -70.0;
            // dos pasadas: medir, corregir, volver a medir
            for _ in 0..3 {
                p.limiter.gain_db = gain;
                let a = crate::engine::render_song(&p, &bank, 44_100.0, &pr, &ca);
                let (l, _) = crate::loudness::measure(&a, 44_100.0);
                lufs = l;
                if l <= -69.0 {
                    let _ = tx.send(Err("La canción está en silencio.".into()));
                    return;
                }
                let d = target - l;
                if d.abs() < 0.3 {
                    break;
                }
                gain = (gain + d).clamp(-12.0, 18.0);
            }
            let _ = tx.send(Ok((gain, lufs, p.master_comp.threshold)));
            if let Some(c) = ctx {
                c.request_repaint();
            }
            let _ = (p.master_eq, style);
        });
        self.master_job = Some((rx, self.master_target));
        self.msg = "Analizando la canción para masterizar…".into();
    }

    pub(super) fn poll_master(&mut self) {
        let Some((rx, style)) = &self.master_job else { return };
        let style = *style;
        match rx.try_recv() {
            Ok(r) => {
                self.master_job = None;
                match r {
                    Ok((gain, lufs, thr)) => {
                        self.snap();
                        self.p.master_eq = default_eq();
                        self.p.master_eq[0].on = true;
                        self.p.master_eq[0].freq = 28.0;
                        match style {
                            1 => {
                                self.p.master_eq[1].gain = 1.5;
                                self.p.master_eq[4].gain = 1.5;
                            }
                            3 => {
                                self.p.master_eq[4].gain = -2.5;
                                self.p.master_eq[1].gain = 1.0;
                            }
                            2 => {}
                            _ => self.p.master_eq[4].gain = 0.8,
                        }
                        self.p.master_comp = CompP { on: true, threshold: thr, ratio: if style == 1 { 3.0 } else { 2.0 }, attack_ms: 20.0, release_ms: 180.0, makeup: 0.0, sc: 0 };
                        self.p.limiter = LimP { on: true, gain_db: gain, ceiling_db: -1.0, release_ms: 60.0 };
                        self.msg = format!("Masterizado: {:.1} LUFS integrados (limitador +{:.1} dB). Deshacer para volver.", lufs, gain);
                        self.send();
                    }
                    Err(e) => self.msg = e,
                }
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(_) => {
                self.master_job = None;
                self.msg = "El análisis terminó sin respuesta.".into();
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
        let before = self.settings.clone();
        ui.label(RichText::new("GRABACIÓN").small().color(DIM));
        ui.add(egui::Slider::new(&mut self.settings.latency_ms, 0.0..=250.0).suffix(" ms").text("Compensación de latencia"));
        ui.label(RichText::new("Si lo grabado suena tarde respecto al beat, sube este valor.").small().color(DIM));
        ui.separator();
        ui.label(RichText::new("TECLADO MIDI").small().color(DIM));
        #[cfg(not(target_os = "android"))]
        {
            let ports = midi::input::ports();
            if ports.is_empty() {
                ui.label("No hay teclados MIDI conectados.");
            }
            let mut pick = None;
            for (i, p) in ports.iter().enumerate() {
                let on = self.midi_conn.as_ref().is_some_and(|c| c.name == *p);
                if ui.add(egui::Button::selectable(on, p)).clicked() {
                    pick = Some(i);
                    self.settings.midi_port = p.clone();
                }
            }
            if pick.is_some() || ui.button("Buscar teclados").clicked() {
                self.midi_connect();
            }
        }
        #[cfg(target_os = "android")]
        ui.label("En Android el teclado MIDI todavía no está disponible.");
        ui.checkbox(&mut self.settings.midi_quant, "Cuantizar las notas grabadas");
        ui.separator();
        ui.label(RichText::new("VERSIONES").small().color(DIM));
        ui.add(egui::Slider::new(&mut self.settings.version_min, 1..=30).suffix(" min").text("Guardar una versión cada"));
        #[cfg(windows)]
        {
            ui.separator();
            ui.label(RichText::new("PLUGINS CLAP").small().color(DIM));
            ui.label(RichText::new("Se buscan en Archivos de programa\\Common Files\\CLAP y en estas carpetas:").small().color(DIM));
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
        if format!("{before:?}") != format!("{:?}", self.settings) {
            self.save_settings();
        }
    }

    pub(super) fn new_from_template(&mut self, i: usize) {
        self.snap();
        self.p = templates::build(i);
        self.fix_selection();
        self.send();
        self.reload_takes();
        self.plugins_reload_all();
        self.msg = format!("Plantilla «{}» cargada ({} · {:.0} bpm). Deshacer para volver.", templates::TEMPLATES[i], self.p.key_label(), self.p.bpm);
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
        ui.menu_button("Nuevo desde plantilla", |ui| {
            for (i, n) in templates::TEMPLATES.iter().enumerate() {
                if ui.button(*n).clicked() {
                    ui.close();
                    self.new_from_template(i);
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
        });
        if ui.button("Guardar como plantilla").clicked() {
            ui.close();
            self.save_template();
        }
        ui.menu_button("Versiones anteriores", |ui| {
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
        });
        ui.separator();
        if ui.button("Importar audio a la playlist…").clicked() {
            ui.close();
            if let Some(p) = self.pick_audio() {
                let t = self.free_track(self.play_from, 16);
                self.import_to_playlist(&p, t, self.play_from);
            } else if self.plat.mobile {
                self.left_tab = 1;
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
        ui.menu_button("Aplicar kit de batería", |ui| {
            for (i, n) in templates::KITS.iter().enumerate() {
                if ui.button(*n).clicked() {
                    ui.close();
                    self.snap();
                    templates::apply_kit(&mut self.p, i);
                    self.msg = format!("Kit «{n}» aplicado a la batería.");
                    self.send();
                }
            }
        });
        ui.separator();
        if ui.button("Ajustes…").clicked() {
            ui.close();
            self.open_win(Win::Settings);
        }
    }

    /// Guarda una copia del proyecto cada cierto tiempo (historial de versiones).
    pub(super) fn version_tick(&mut self) {
        if !self.version_dirty || self.last_version.elapsed() < Duration::from_secs(self.settings.version_min.max(1) as u64 * 60) {
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
        ui.menu_button("Presets", |ui| {
            if ui.button("Guardar preset del canal").clicked() {
                ui.close();
                let path = self.user_dir("presets").join(format!("{} - {}.json", c.inst.name(), export::safe_name(&c.name)));
                self.msg = match serde_json::to_string_pretty(&c).map_err(|e| e.to_string()).and_then(|s| std::fs::write(&path, s).map_err(|e| e.to_string())) {
                    Ok(_) => format!("Preset «{}» guardado.", c.name),
                    Err(e) => format!("No se pudo guardar: {e}"),
                };
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
        });
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

