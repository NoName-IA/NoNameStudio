//! Ventanas de los plugins: NN EQ, NN Dynamics, NN Verb, NN Space, NN Wave y NN Tune.
use super::*;
use crate::fx::{self, eq_response_db, WT_NAMES};

#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Win {
    Eq(Option<u32>),
    Comp(u32),
    Dyn,
    Verb,
    Space,
    Wave(u32),
    Tune(u32),
    Sampler(u32),
    Auto(u32),
    Clip(u32),
    Inserts(u32),
    Master,
    Settings,
}

pub(super) struct TakeView {
    pub peaks: Vec<f32>,
    pub det: Vec<f32>,
    pub cor: Vec<f32>,
    pub dur: f32,
}

pub(super) struct RecState {
    pub r: audio::Recorder,
    pub pat: u32,
    pub ch: u32,
    pub max_s: f32,
    pub started: Instant,
}

pub(super) struct TuneDone {
    pub pat: u32,
    pub ch: u32,
    pub sr: f32,
    pub res: fx::TuneResult,
}

/// Perilla con seguimiento de inicio (para deshacer) y cambio.
macro_rules! kn {
    ($ui:expr, $st:ident, $ch:ident, $v:expr, $min:expr, $max:expr, $def:expr, $lab:expr, $fmt:expr) => {{
        let r = knob($ui, $v, $min, $max, $def, $lab, $fmt);
        $st |= r.drag_started() || r.double_clicked();
        $ch |= r.changed();
    }};
}

fn log_norm(v: f32, lo: f32, hi: f32) -> f32 {
    ((v / lo).ln() / (hi / lo).ln()).clamp(0.0, 1.0)
}
fn log_val(n: f32, lo: f32, hi: f32) -> f32 {
    lo * (hi / lo).powf(n)
}
fn hz_txt(f: f32) -> String {
    if f >= 1000.0 { format!("{:.2}k", f / 1000.0) } else { format!("{:.0}", f) }
}

impl StudioApp {
    /// Abre ventanas de plugins para revisión visual (opción --plugins).
    pub fn debug_open(&mut self, which: &str) {
        if which.contains(',') {
            for w in which.split(',') {
                self.debug_open(w);
            }
            return;
        }
        let first_sample = || -> Option<PathBuf> {
            let mut v: Vec<PathBuf> = std::fs::read_dir(crate::media::samples_dir(&self.plat.data_dir)).ok()?.flatten().map(|e| e.path()).filter(|p| crate::media::is_audio(p)).collect();
            v.sort();
            v.into_iter().next()
        };
        match which {
            "trap" => self.new_from_template(1),
            "house" => self.new_from_template(4),
            "browser" => {
                self.left_tab = 1;
                self.browser.dir = Some(crate::media::samples_dir(&self.plat.data_dir));
                return;
            }
            "sampler" => {
                if let Some(f) = first_sample() {
                    self.import_to_rack(&f);
                    let id = self.sel_ch;
                    if let Some(c) = self.p.channel_mut(id) {
                        c.smp.slice = true;
                    }
                    self.open_win(Win::Sampler(id));
                }
                return;
            }
            "clip" => {
                if let Some(f) = first_sample() {
                    self.import_to_playlist(&f, 3, 16);
                }
                return;
            }
            "auto" => {
                if let Some(id) = self.p.channels.iter().find(|c| c.inst == Inst::Rhodes).map(|c| c.id) {
                    self.add_auto(AutoTarget::Cut(id));
                    if let Some(a) = self.p.autos.last_mut() {
                        a.pts = vec![[0.0, 0.3], [0.5, 0.9], [1.0, 0.5]];
                    }
                    self.send();
                }
                return;
            }
            "master" => {
                self.open_win(Win::Master);
                return;
            }
            "settings" => {
                self.open_win(Win::Settings);
                return;
            }
            "fx" if self.p.channels.iter().any(|c| c.inst == Inst::Plugin) => {
                return;
            }
            "inserts" => {
                self.rescan_plugins();
                self.open_win(Win::Inserts(0));
                return;
            }
            #[cfg(windows)]
            "plugin" => {
                self.rescan_plugins();
                if let Some(pi) = self.plug.list.iter().find(|p| !p.instrument).cloned() {
                    let uid = self.p.new_id();
                    let slot = PluginSlot { path: pi.path.clone(), id: pi.id.clone(), name: pi.name.clone(), on: true, state: String::new(), uid };
                    self.p.master_inserts.push(slot.clone());
                    self.send();
                    self.plugins_reload_all();
                    self.open_win(Win::Inserts(0));
                    self.debug_gui(&slot);
                }
                return;
            }
            "mixer" => {
                self.view = View::Mixer;
                return;
            }
            "rec" => {
                self.p.loop_on = true;
                self.p.loop_a = 0;
                self.p.loop_b = 1;
                self.send();
                self.rec_track_start();
                return;
            }
            "play" => {
                self.song_mode = true;
                self.play(0);
                return;
            }
            "loop" => {
                self.p.loop_on = true;
                self.p.loop_a = 2;
                self.p.loop_b = 4;
                self.send();
                return;
            }
            "roll" => {
                self.view = View::Roll;
                return;
            }
            _ => {}
        }
        let synth = self.p.channels.iter().find(|c| c.inst == Inst::Synth).map(|c| c.id);
        let rhodes = self.p.channels.iter().find(|c| c.inst == Inst::Rhodes).map(|c| c.id);
        match which {
            "eq" => self.open_win(Win::Eq(None)),
            "dyn" => self.open_win(Win::Dyn),
            "fx" => {
                self.open_win(Win::Verb);
                self.open_win(Win::Space);
            }
            "wave" => {
                let id = match synth {
                    Some(id) => id,
                    None => {
                        let id = self.p.new_id();
                        self.p.channels.push(Channel::new(id, Inst::Synth));
                        self.send();
                        id
                    }
                };
                self.open_win(Win::Wave(id));
            }
            "tune" => {
                let id = match self.p.channels.iter().find(|c| c.inst == Inst::Vocal) {
                    Some(c) => c.id,
                    None => {
                        let id = self.p.new_id();
                        self.p.channels.push(Channel::new(id, Inst::Vocal));
                        self.send();
                        id
                    }
                };
                self.open_win(Win::Tune(id));
            }
            "chan" => {
                if let Some(id) = rhodes {
                    self.open_win(Win::Eq(Some(id)));
                    self.open_win(Win::Comp(id));
                }
            }
            _ => {}
        }
    }

    pub(super) fn open_win(&mut self, w: Win) {
        if !self.wins.contains(&w) {
            self.wins.push(w);
        }
    }

    fn win_title(&self, w: Win) -> String {
        let cn = |id: u32| self.p.channel(id).map(|c| c.name.clone()).unwrap_or_default();
        match w {
            Win::Eq(Some(id)) => format!("NN EQ — {}", cn(id)),
            Win::Eq(None) => "NN EQ — Master".into(),
            Win::Comp(id) => format!("NN Dynamics — {}", cn(id)),
            Win::Dyn => "NN Dynamics — Master".into(),
            Win::Verb => "NN Verb".into(),
            Win::Space => "NN Space".into(),
            Win::Wave(id) => format!("NN Wave — {}", cn(id)),
            Win::Tune(id) => format!("NN Tune — {}", cn(id)),
            Win::Sampler(id) => format!("NN Sampler — {}", cn(id)),
            Win::Auto(_) => "Automatización".into(),
            Win::Clip(_) => "Clip de audio".into(),
            Win::Inserts(0) => "Plugins — Master".into(),
            Win::Inserts(id) => format!("Plugins — {}", cn(id)),
            Win::Master => "NN Master — sonoridad y mastering".into(),
            Win::Settings => "Ajustes".into(),
        }
    }

    pub(super) fn plugin_windows(&mut self, ctx: &egui::Context) {
        let wins = self.wins.clone();
        let mut close = vec![];
        for (i, w) in wins.into_iter().enumerate() {
            let exists = match w {
                Win::Eq(Some(id)) | Win::Comp(id) | Win::Wave(id) | Win::Tune(id) | Win::Sampler(id) => self.p.channel(id).is_some(),
                Win::Inserts(id) if id != 0 => self.p.channel(id).is_some(),
                Win::Auto(id) => self.p.auto_clip(id).is_some(),
                Win::Clip(id) => self.p.audio_clip(id).is_some(),
                _ => true,
            };
            if !exists {
                close.push(w);
                continue;
            }
            let mut open = true;
            egui::Window::new(RichText::new(self.win_title(w)).strong())
                .id(egui::Id::new(("plugin", format!("{w:?}"))))
                .open(&mut open)
                .resizable(false)
                .collapsible(true)
                .default_pos(Pos2::new(120.0 + i as f32 * 30.0, 110.0 + i as f32 * 30.0))
                .frame(Frame::window(&ctx.global_style()).fill(Color32::from_rgb(30, 33, 38)))
                .show(ctx, |ui| match w {
                    Win::Eq(ch) => self.eq_window(ui, ch),
                    Win::Comp(ch) => self.comp_window(ui, ch),
                    Win::Dyn => self.dyn_window(ui),
                    Win::Verb => self.verb_window(ui),
                    Win::Space => self.space_window(ui),
                    Win::Wave(ch) => self.wave_window(ui, ch),
                    Win::Tune(ch) => self.tune_window(ui, ch),
                    Win::Sampler(ch) => self.sampler_window(ui, ch),
                    Win::Auto(id) => self.auto_window(ui, id),
                    Win::Clip(id) => self.clip_window(ui, id),
                    Win::Inserts(ch) => self.inserts_window(ui, ch),
                    Win::Master => self.master_window(ui),
                    Win::Settings => self.settings_window(ui),
                });
            if !open {
                close.push(w);
            }
        }
        self.wins.retain(|w| !close.contains(w));
    }

    // ---------------- NN EQ ----------------
    fn spectrum(&mut self) {
        let data = match self.status.scope.lock() {
            Ok(s) => s.clone(),
            Err(_) => return,
        };
        let n = data.len();
        let sr = self.audio.as_ref().map(|a| a.sr as f32).unwrap_or(44100.0);
        let bins = 96;
        if self.spec.len() != bins {
            self.spec = vec![-90.0; bins];
        }
        for k in 0..bins {
            let f = log_val(k as f32 / (bins - 1) as f32, 25.0, 18000.0);
            let w = TAU_F * f / sr;
            let (cw, sw) = (w.cos(), w.sin());
            let (mut c, mut s) = (1.0f32, 0.0f32);
            let (mut re, mut im) = (0.0f32, 0.0f32);
            for (i, x) in data.iter().enumerate() {
                let win = 0.5 - 0.5 * (TAU_F * i as f32 / n as f32).cos();
                re += x * win * c;
                im += x * win * s;
                let nc = c * cw - s * sw;
                s = c * sw + s * cw;
                c = nc;
            }
            let db = 20.0 * ((re * re + im * im).sqrt() * 4.0 / n as f32).max(1e-6).log10();
            self.spec[k] = db.max(self.spec[k] - 1.2);
        }
    }

    fn eq_editor(&mut self, ui: &mut Ui, eq: &mut [Band; 6], show_spec: bool) -> (bool, bool) {
        let (mut started, mut changed) = (false, false);
        let sr = 44100.0;
        let w = if self.plat.mobile { 520.0 } else { 620.0 };
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 250.0), Sense::click_and_drag());
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 6, Color32::from_rgb(16, 18, 21));
        let fx_ = |f: f32| rect.left() + (f / 20.0).log10() / 3.0 * rect.width();
        let xf = |x: f32| 20.0 * 10f32.powf(3.0 * ((x - rect.left()) / rect.width()).clamp(0.0, 1.0));
        let gy = |db: f32| rect.center().y - db / 18.0 * (rect.height() / 2.0 - 10.0);
        let yg = |y: f32| ((rect.center().y - y) / (rect.height() / 2.0 - 10.0) * 18.0).clamp(-18.0, 18.0);
        for f in [50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0] {
            let x = fx_(f);
            p.vline(x, rect.y_range(), Stroke::new(1.0, Color32::from_rgb(34, 38, 44)));
            p.text(Pos2::new(x + 3.0, rect.bottom() - 4.0), Align2::LEFT_BOTTOM, hz_txt(f), FontId::proportional(9.5), DIM);
        }
        for db in [-12.0, -6.0, 0.0, 6.0, 12.0] {
            p.hline(rect.x_range(), gy(db), Stroke::new(1.0, if db == 0.0 { Color32::from_rgb(52, 58, 66) } else { Color32::from_rgb(30, 34, 39) }));
            p.text(Pos2::new(rect.left() + 4.0, gy(db) - 2.0), Align2::LEFT_BOTTOM, format!("{db:+}"), FontId::proportional(9.0), Color32::from_rgb(90, 98, 110));
        }
        if show_spec && !self.spec.is_empty() {
            let n = self.spec.len();
            let mut pts = vec![Pos2::new(rect.left(), rect.bottom())];
            for (k, db) in self.spec.iter().enumerate() {
                let f = log_val(k as f32 / (n - 1) as f32, 25.0, 18000.0);
                let y = rect.bottom() - ((db + 90.0) / 90.0).clamp(0.0, 1.0) * rect.height();
                pts.push(Pos2::new(fx_(f), y));
            }
            pts.push(Pos2::new(rect.right(), rect.bottom()));
            p.add(egui::Shape::line(pts, Stroke::new(1.0, Color32::from_rgb(70, 110, 140))));
        }
        // curva total y bandas
        let curve: Vec<Pos2> = (0..=rect.width() as usize / 2).map(|i| {
            let x = rect.left() + i as f32 * 2.0;
            Pos2::new(x, gy(eq_response_db(eq, xf(x), sr).clamp(-18.0, 18.0)))
        }).collect();
        p.add(egui::Shape::line(curve, Stroke::new(2.4, ACCENT)));
        let cols = [Color32::from_rgb(230, 90, 90), Color32::from_rgb(240, 170, 60), Color32::from_rgb(220, 220, 80), Color32::from_rgb(90, 210, 120), Color32::from_rgb(90, 170, 240), Color32::from_rgb(180, 120, 240)];
        let handle = |b: &Band| Pos2::new(fx_(b.freq), gy(if matches!(b.kind, BandKind::LowCut | BandKind::HighCut) { 0.0 } else { b.gain }));
        for (i, b) in eq.iter().enumerate() {
            let h = handle(b);
            let col = if b.on { cols[i] } else { Color32::from_gray(80) };
            p.circle(h, 8.0, col.gamma_multiply(if self.eq_drag == Some(i) { 1.0 } else { 0.8 }), Stroke::new(1.5, Color32::BLACK));
            p.text(h, Align2::CENTER_CENTER, format!("{}", i + 1), FontId::proportional(10.0), Color32::BLACK);
        }
        let near = |pos: Pos2, eq: &[Band; 6]| eq.iter().enumerate().map(|(i, b)| (i, handle(b).distance(pos))).filter(|(_, d)| *d < 16.0).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(i, _)| i);
        if resp.drag_started() {
            if let Some(pos) = ui.input(|i| i.pointer.press_origin()) {
                self.eq_drag = near(pos, eq);
                started |= self.eq_drag.is_some();
            }
        }
        if let (Some(i), Some(pos)) = (self.eq_drag, resp.interact_pointer_pos()) {
            let b = &mut eq[i];
            b.freq = xf(pos.x).clamp(20.0, 20000.0).round();
            if !matches!(b.kind, BandKind::LowCut | BandKind::HighCut) {
                b.gain = (yg(pos.y) * 10.0).round() / 10.0;
            }
            b.on = true;
            changed = true;
        }
        if resp.drag_stopped() {
            self.eq_drag = None;
        }
        if let Some(pos) = resp.hover_pos() {
            if let Some(i) = near(pos, eq) {
                let dy = ui.input(|inp| inp.smooth_scroll_delta.y);
                if dy != 0.0 {
                    started |= true;
                    eq[i].q = (eq[i].q * 1.1f32.powf(dy / 30.0)).clamp(0.1, 18.0);
                    changed = true;
                }
                if resp.double_clicked() {
                    started = true;
                    eq[i].on = !eq[i].on;
                    changed = true;
                }
            }
        }
        ui.add_space(4.0);
        egui::Grid::new("eqgrid").spacing(Vec2::new(10.0, 4.0)).show(ui, |ui| {
            for (i, b) in eq.iter_mut().enumerate() {
                let mut on = b.on;
                if ui.checkbox(&mut on, RichText::new(format!("{}", i + 1)).color(cols[i])).changed() {
                    b.on = on;
                    started = true;
                    changed = true;
                }
                let mut kind = b.kind;
                egui::ComboBox::from_id_salt(("eqk", i)).width(120.0).selected_text(kind.name()).show_ui(ui, |ui| {
                    for k in BandKind::ALL {
                        ui.selectable_value(&mut kind, k, k.name());
                    }
                });
                if kind != b.kind {
                    b.kind = kind;
                    started = true;
                    changed = true;
                }
                for (v, lo, hi, sp, suf) in [(&mut b.freq, 20.0, 20000.0, 5.0, " Hz"), (&mut b.gain, -18.0, 18.0, 0.1, " dB"), (&mut b.q, 0.1, 18.0, 0.02, " Q")] {
                    let r = ui.add(egui::DragValue::new(v).range(lo..=hi).speed(sp).max_decimals(2).suffix(suf));
                    started |= r.drag_started() || r.gained_focus();
                    changed |= r.changed();
                }
                ui.end_row();
            }
        });
        ui.label(RichText::new("Arrastra los puntos · rueda sobre un punto: ancho (Q) · doble clic: activar/desactivar").small().color(DIM));
        (started, changed)
    }

    fn eq_window(&mut self, ui: &mut Ui, ch: Option<u32>) {
        let mut eq = match ch {
            Some(id) => self.p.channel(id).map(|c| c.eq).unwrap_or_else(default_eq),
            None => self.p.master_eq,
        };
        let show = ch.is_none();
        if show {
            self.spectrum();
        }
        let (st, chg) = self.eq_editor(ui, &mut eq, show);
        ui.horizontal(|ui| {
            if ui.button("Restablecer").clicked() {
                self.snap();
                eq = default_eq();
                self.apply_eq(ch, eq);
            }
            if show {
                ui.label(RichText::new("Analizador: salida del master (azul)").small().color(DIM));
            }
        });
        if st {
            self.snap();
        }
        if chg {
            self.apply_eq(ch, eq);
        }
    }
    fn apply_eq(&mut self, ch: Option<u32>, eq: [Band; 6]) {
        match ch {
            Some(id) => {
                if let Some(c) = self.p.channel_mut(id) {
                    c.eq = eq;
                }
            }
            None => self.p.master_eq = eq,
        }
        self.send();
    }

    // ---------------- NN Dynamics ----------------
    fn comp_section(ui: &mut Ui, c: &mut CompP, gr: Option<f32>) -> (bool, bool) {
        let (mut st, mut chg) = (false, false);
        ui.horizontal(|ui| {
            ui.label(RichText::new("COMPRESOR").strong().color(ACCENT2));
            if ui.checkbox(&mut c.on, "Activado").changed() {
                st = true;
                chg = true;
            }
        });
        ui.horizontal(|ui| {
            // curva de transferencia
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(120.0), Sense::hover());
            let p = ui.painter();
            p.rect_filled(rect, 4, Color32::from_rgb(16, 18, 21));
            let map = |db: f32| (db + 60.0) / 60.0;
            p.line_segment([rect.left_bottom(), rect.right_top()], Stroke::new(1.0, Color32::from_rgb(45, 50, 58)));
            let pts: Vec<Pos2> = (0..=60).map(|i| {
                let inp = -60.0 + i as f32;
                let over = inp - c.threshold;
                let out = if over > 0.0 && c.on { c.threshold + over / c.ratio.max(1.0) } else { inp } + if c.on { c.makeup } else { 0.0 };
                Pos2::new(rect.left() + map(inp) * rect.width(), rect.bottom() - map(out).clamp(0.0, 1.0) * rect.height())
            }).collect();
            p.add(egui::Shape::line(pts, Stroke::new(2.0, ACCENT)));
            let ty = rect.bottom() - map(c.threshold) * rect.height();
            p.hline(rect.x_range(), ty, Stroke::new(1.0, Color32::from_rgb(90, 70, 40)));
            ui.vertical(|ui| {
                egui::Grid::new(ui.next_auto_id()).spacing(Vec2::new(2.0, 2.0)).show(ui, |ui| {
                    kn!(ui, st, chg, &mut c.threshold, -48.0, 0.0, -18.0, "Umbral", |v| format!("{v:.0} dB"));
                    kn!(ui, st, chg, &mut c.ratio, 1.0, 20.0, 3.0, "Ratio", |v| format!("{v:.1}:1"));
                    kn!(ui, st, chg, &mut c.attack_ms, 0.1, 100.0, 10.0, "Ataque", |v| format!("{v:.1} ms"));
                    ui.end_row();
                    kn!(ui, st, chg, &mut c.release_ms, 5.0, 1000.0, 120.0, "Liberación", |v| format!("{v:.0} ms"));
                    kn!(ui, st, chg, &mut c.makeup, 0.0, 24.0, 0.0, "Ganancia", |v| format!("+{v:.1} dB"));
                    ui.end_row();
                });
                if let Some(g) = gr {
                    gr_bar(ui, g);
                }
            });
        });
        (st, chg)
    }

    fn comp_window(&mut self, ui: &mut Ui, id: u32) {
        let Some(mut c) = self.p.channel(id).map(|c| c.comp) else { return };
        let (mut st, mut chg) = Self::comp_section(ui, &mut c, None);
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(RichText::new("SIDECHAIN").strong().color(ACCENT2));
            let srcs = super::studio_ui::sc_sources(&self.p, id);
            let cur = srcs.iter().find(|s| s.0 == c.sc).map(|s| s.1.clone()).unwrap_or_else(|| "Ninguno (su propia señal)".into());
            let mut sc = c.sc;
            egui::ComboBox::from_id_salt(("sc", id)).width(170.0).selected_text(cur).show_ui(ui, |ui| {
                ui.selectable_value(&mut sc, 0, "Ninguno (su propia señal)");
                for (sid, n) in &srcs {
                    ui.selectable_value(&mut sc, *sid, n);
                }
            });
            if sc != c.sc {
                c.sc = sc;
                st = true;
                chg = true;
            }
        });
        let kick = self.p.channels.iter().find(|x| x.inst == Inst::Kick && x.id != id).map(|x| x.id);
        if let Some(k) = kick {
            if ui.button("Bombeo con el kick (ducking)").on_hover_text("Compresión fuerte disparada por el kick: el canal se agacha en cada golpe").clicked() {
                c = CompP::ducking(k);
                st = true;
                chg = true;
            }
        }
        ui.label(RichText::new("Con sidechain el compresor escucha a otro canal: típico para que el bajo o los pads dejen espacio al kick.").small().color(DIM));
        if st {
            self.snap();
        }
        if chg {
            if let Some(cm) = self.p.channel_mut(id) {
                cm.comp = c;
            }
            self.send();
        }
    }

    fn dyn_window(&mut self, ui: &mut Ui) {
        let gr = self.gr_hold;
        let mut c = self.p.master_comp;
        let (mut st, mut chg) = Self::comp_section(ui, &mut c, Some(gr[0]));
        ui.separator();
        let mut l = self.p.limiter;
        ui.horizontal(|ui| {
            ui.label(RichText::new("LIMITADOR").strong().color(ACCENT2));
            if ui.checkbox(&mut l.on, "Activado").changed() {
                st = true;
                chg = true;
            }
        });
        ui.horizontal(|ui| {
            kn!(ui, st, chg, &mut l.gain_db, -6.0, 18.0, 0.0, "Ganancia", |v| format!("{v:+.1} dB"));
            kn!(ui, st, chg, &mut l.ceiling_db, -12.0, 0.0, -1.0, "Techo", |v| format!("{v:.1} dB"));
            kn!(ui, st, chg, &mut l.release_ms, 5.0, 500.0, 80.0, "Liberación", |v| format!("{v:.0} ms"));
            ui.vertical(|ui| {
                ui.add_space(14.0);
                gr_bar(ui, gr[1]);
            });
        });
        if st {
            self.snap();
        }
        if chg {
            self.p.master_comp = c;
            self.p.limiter = l;
            self.send();
        }
    }

    // ---------------- NN Verb ----------------
    fn verb_window(&mut self, ui: &mut Ui) {
        let mut v = self.p.verb;
        let (mut st, mut chg) = (false, false);
        let (rect, _) = ui.allocate_exact_size(Vec2::new(420.0, 90.0), Sense::hover());
        let p = ui.painter();
        p.rect_filled(rect, 5, Color32::from_rgb(16, 18, 21));
        let pre = v.predelay_ms / 250.0 * rect.width() * 0.25;
        let tail = 0.4 + v.decay * 3.0 + v.size;
        let pts: Vec<Pos2> = (0..=100).map(|i| {
            let t = i as f32 / 100.0;
            let x = rect.left() + pre + t * (rect.width() - pre);
            let a = (-t * 5.0 / tail).exp() * (1.0 - v.damp * 0.5 * t);
            Pos2::new(x, rect.bottom() - 6.0 - a * (rect.height() - 14.0))
        }).collect();
        p.vline(rect.left() + pre, rect.y_range(), Stroke::new(1.0, Color32::from_rgb(90, 70, 40)));
        p.add(egui::Shape::line(pts, Stroke::new(2.0, ACCENT)));
        p.text(rect.right_top() + Vec2::new(-6.0, 6.0), Align2::RIGHT_TOP, format!("cola ~ {:.1} s", tail * 0.8), FontId::proportional(10.0), DIM);
        egui::Grid::new("verbk").spacing(Vec2::new(4.0, 2.0)).show(ui, |ui| {
            kn!(ui, st, chg, &mut v.size, 0.0, 1.0, 0.6, "Tamaño", |x| format!("{:.0}%", x * 100.0));
            kn!(ui, st, chg, &mut v.decay, 0.0, 1.0, 0.55, "Decaimiento", |x| format!("{:.0}%", x * 100.0));
            kn!(ui, st, chg, &mut v.damp, 0.0, 0.95, 0.25, "Amortiguar", |x| format!("{:.0}%", x * 100.0));
            kn!(ui, st, chg, &mut v.predelay_ms, 0.0, 250.0, 20.0, "Predelay", |x| format!("{x:.0} ms"));
            ui.end_row();
            kn!(ui, st, chg, &mut v.width, 0.0, 1.0, 1.0, "Ancho", |x| format!("{:.0}%", x * 100.0));
            let mut lc = log_norm(v.lowcut, 20.0, 1000.0);
            kn!(ui, st, chg, &mut lc, 0.0, 1.0, 0.6, "Corte grave", |x| format!("{} Hz", hz_txt(log_val(x, 20.0, 1000.0))));
            v.lowcut = log_val(lc, 20.0, 1000.0);
            kn!(ui, st, chg, &mut v.ret, 0.0, 2.0, 1.0, "Retorno", |x| format!("{:.0}%", x * 100.0));
            ui.end_row();
        });
        ui.label(RichText::new("Cada canal envía con su perilla «Reverb» del mixer.").small().color(DIM));
        if st {
            self.snap();
        }
        if chg {
            self.p.verb = v;
            self.send();
        }
    }

    // ---------------- NN Space ----------------
    fn space_window(&mut self, ui: &mut Ui) {
        let mut v = self.p.space;
        let (mut st, mut chg) = (false, false);
        let (rect, _) = ui.allocate_exact_size(Vec2::new(420.0, 90.0), Sense::hover());
        let p = ui.painter();
        p.rect_filled(rect, 5, Color32::from_rgb(12, 14, 20));
        let t = ui.input(|i| i.time) as f32;
        for i in 0..8 {
            let r = [1.0, 1.131, 1.273, 1.414, 1.531, 1.672, 1.797, 1.971][i];
            let x = rect.left() + 20.0 + (v.delay_ms * r / 2000.0).min(1.0) * (rect.width() - 40.0);
            let wob = (t * v.mod_rate * 6.0 + i as f32).sin() * v.mod_depth * 6.0;
            let h = (rect.height() - 20.0) * v.feedback.powf(i as f32 * 0.3 + 1.0);
            p.vline(x + wob, (rect.bottom() - 8.0 - h)..=(rect.bottom() - 8.0), Stroke::new(3.0, Color32::from_rgb(120, 140, 255).gamma_multiply(0.4 + 0.6 * v.density)));
        }
        ui.ctx().request_repaint_after(Duration::from_millis(33));
        egui::Grid::new("spacek").spacing(Vec2::new(4.0, 2.0)).show(ui, |ui| {
            let mut dn = log_norm(v.delay_ms, 10.0, 2000.0);
            kn!(ui, st, chg, &mut dn, 0.0, 1.0, 0.6, "Tiempo", |x| format!("{:.0} ms", log_val(x, 10.0, 2000.0)));
            v.delay_ms = log_val(dn, 10.0, 2000.0);
            kn!(ui, st, chg, &mut v.feedback, 0.0, 0.985, 0.82, "Realiment.", |x| format!("{:.0}%", x * 100.0));
            kn!(ui, st, chg, &mut v.density, 0.0, 1.0, 0.7, "Densidad", |x| format!("{:.0}%", x * 100.0));
            kn!(ui, st, chg, &mut v.damp, 0.0, 0.95, 0.35, "Amortiguar", |x| format!("{:.0}%", x * 100.0));
            ui.end_row();
            kn!(ui, st, chg, &mut v.mod_depth, 0.0, 1.0, 0.4, "Mod. prof.", |x| format!("{:.0}%", x * 100.0));
            kn!(ui, st, chg, &mut v.mod_rate, 0.01, 4.0, 0.3, "Mod. vel.", |x| format!("{x:.2} Hz"));
            kn!(ui, st, chg, &mut v.ret, 0.0, 2.0, 0.9, "Retorno", |x| format!("{:.0}%", x * 100.0));
            ui.end_row();
        });
        ui.label(RichText::new("Espacios enormes: cada canal envía con su perilla «Space» del mixer.").small().color(DIM));
        if st {
            self.snap();
        }
        if chg {
            self.p.space = v;
            self.send();
        }
    }

    // ---------------- NN Wave ----------------
    fn wave_window(&mut self, ui: &mut Ui, id: u32) {
        let Some(mut s) = self.p.channel(id).map(|c| c.synth) else { return };
        let (mut st, mut chg) = (false, false);
        ui.horizontal(|ui| {
            ui.label(RichText::new("PRESET").small().color(DIM));
            for (i, n) in SYNTH_PRESETS.iter().enumerate() {
                if ui.button(*n).clicked() {
                    s = SynthP::preset(i);
                    st = true;
                    chg = true;
                    self.preview(id, 60, 0.9);
                }
            }
        });
        ui.horizontal_top(|ui| {
            for (k, o) in s.osc.iter_mut().enumerate() {
                Frame::new().fill(Color32::from_rgb(22, 25, 30)).corner_radius(6).inner_margin(6).show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("OSC {}", if k == 0 { "A" } else { "B" })).strong().color(ACCENT2));
                            if ui.checkbox(&mut o.on, "").changed() {
                                st = true;
                                chg = true;
                            }
                            ui.label(RichText::new(WT_NAMES[(o.wt.round() as usize).min(4)]).small().color(DIM));
                        });
                        let (rect, _) = ui.allocate_exact_size(Vec2::new(200.0, 60.0), Sense::hover());
                        let p = ui.painter();
                        p.rect_filled(rect, 4, Color32::from_rgb(12, 14, 17));
                        p.hline(rect.x_range(), rect.center().y, Stroke::new(1.0, Color32::from_rgb(34, 38, 44)));
                        let pts: Vec<Pos2> = (0..=100).map(|i| {
                            let ph = i as f32 / 100.0;
                            Pos2::new(rect.left() + ph * rect.width(), rect.center().y - fx::wt_sample(o.wt, ph) * (rect.height() / 2.0 - 4.0))
                        }).collect();
                        p.add(egui::Shape::line(pts, Stroke::new(2.0, if o.on { ACCENT } else { Color32::GRAY })));
                        egui::Grid::new(("osc", k)).spacing(Vec2::new(2.0, 2.0)).show(ui, |ui| {
                            kn!(ui, st, chg, &mut o.wt, 0.0, 4.0, 2.0, "Tabla", |v| format!("{v:.2}"));
                            let mut oc = o.oct as f32;
                            kn!(ui, st, chg, &mut oc, -3.0, 3.0, 0.0, "Octava", |v| format!("{:+}", v.round()));
                            o.oct = oc.round() as i32;
                            let mut se = o.semi as f32;
                            kn!(ui, st, chg, &mut se, -12.0, 12.0, 0.0, "Semitono", |v| format!("{:+}", v.round()));
                            o.semi = se.round() as i32;
                            kn!(ui, st, chg, &mut o.fine, -100.0, 100.0, 0.0, "Fino", |v| format!("{v:+.0} ct"));
                            ui.end_row();
                            let mut un = o.unison as f32;
                            kn!(ui, st, chg, &mut un, 1.0, 8.0, 1.0, "Unísono", |v| format!("{}", v.round()));
                            o.unison = un.round() as u32;
                            kn!(ui, st, chg, &mut o.detune, 0.0, 1.0, 0.2, "Desafinar", |v| format!("{:.0}%", v * 100.0));
                            kn!(ui, st, chg, &mut o.level, 0.0, 1.0, 0.7, "Nivel", |v| format!("{:.0}%", v * 100.0));
                            ui.end_row();
                        });
                    });
                });
            }
        });
        ui.horizontal_top(|ui| {
            Frame::new().fill(Color32::from_rgb(22, 25, 30)).corner_radius(6).inner_margin(6).show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("FILTRO").strong().color(ACCENT2));
                        for (i, n) in ["LP", "HP", "BP"].iter().enumerate() {
                            if ui.add(egui::Button::selectable(s.ftype == i as u8, *n)).clicked() {
                                s.ftype = i as u8;
                                st = true;
                                chg = true;
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        let mut cn = log_norm(s.cutoff, 20.0, 18000.0);
                        kn!(ui, st, chg, &mut cn, 0.0, 1.0, 0.8, "Corte", |v| format!("{} Hz", hz_txt(log_val(v, 20.0, 18000.0))));
                        s.cutoff = log_val(cn, 20.0, 18000.0);
                        kn!(ui, st, chg, &mut s.reso, 0.0, 1.0, 0.15, "Resonancia", |v| format!("{:.0}%", v * 100.0));
                        kn!(ui, st, chg, &mut s.fenv, -1.0, 1.0, 0.25, "Env.", |v| format!("{:+.0}%", v * 100.0));
                    });
                });
            });
            for (label, env) in [("AMP", &mut s.amp), ("ENV FILTRO", &mut s.fadsr)] {
                Frame::new().fill(Color32::from_rgb(22, 25, 30)).corner_radius(6).inner_margin(6).show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new(label).strong().color(ACCENT2));
                        adsr_view(ui, env);
                        ui.horizontal(|ui| {
                            kn!(ui, st, chg, &mut env[0], 0.001, 3.0, 0.01, "A", |v| format!("{:.0} ms", v * 1000.0));
                            kn!(ui, st, chg, &mut env[1], 0.001, 3.0, 0.3, "D", |v| format!("{:.0} ms", v * 1000.0));
                            kn!(ui, st, chg, &mut env[2], 0.0, 1.0, 0.8, "S", |v| format!("{:.0}%", v * 100.0));
                            kn!(ui, st, chg, &mut env[3], 0.005, 4.0, 0.35, "R", |v| format!("{:.0} ms", v * 1000.0));
                        });
                    });
                });
            }
            Frame::new().fill(Color32::from_rgb(22, 25, 30)).corner_radius(6).inner_margin(6).show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("LFO").strong().color(ACCENT2));
                    ui.horizontal(|ui| {
                        for (i, n) in ["Filtro", "Tabla", "Tono"].iter().enumerate() {
                            if ui.add(egui::Button::selectable(s.lfo_target == i as u8, *n)).clicked() {
                                s.lfo_target = i as u8;
                                st = true;
                                chg = true;
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        kn!(ui, st, chg, &mut s.lfo_rate, 0.05, 12.0, 0.3, "Vel.", |v| format!("{v:.2} Hz"));
                        kn!(ui, st, chg, &mut s.lfo_depth, 0.0, 1.0, 0.0, "Prof.", |v| format!("{:.0}%", v * 100.0));
                        kn!(ui, st, chg, &mut s.level, 0.0, 1.0, 0.55, "Volumen", |v| format!("{:.0}%", v * 100.0));
                    });
                });
            });
        });
        // teclado para probar
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width().min(760.0), 46.0), Sense::click());
        let p = ui.painter_at(rect);
        let whites: Vec<u8> = (48u8..72).filter(|m| !is_black(*m)).collect();
        let kw = rect.width() / whites.len() as f32;
        for (i, m) in whites.iter().enumerate() {
            let r = Rect::from_min_size(Pos2::new(rect.left() + i as f32 * kw, rect.top()), Vec2::new(kw - 1.0, rect.height()));
            p.rect_filled(r, 2, Color32::from_rgb(230, 230, 230));
            if m % 12 == 0 {
                p.text(r.center_bottom() - Vec2::new(0.0, 4.0), Align2::CENTER_BOTTOM, nname(*m), FontId::proportional(9.0), Color32::DARK_GRAY);
            }
        }
        let mut blacks = vec![];
        for (i, m) in whites.iter().enumerate() {
            if *m + 1 < 72 && is_black(*m + 1) {
                let r = Rect::from_min_size(Pos2::new(rect.left() + (i as f32 + 0.65) * kw, rect.top()), Vec2::new(kw * 0.7, rect.height() * 0.6));
                p.rect_filled(r, 2, Color32::from_rgb(20, 20, 22));
                blacks.push((r, *m + 1));
            }
        }
        if resp.clicked() {
            if let Some(pos) = resp.interact_pointer_pos() {
                let m = blacks.iter().find(|(r, _)| r.contains(pos)).map(|b| b.1).unwrap_or_else(|| whites[(((pos.x - rect.left()) / kw) as usize).min(whites.len() - 1)]);
                self.preview(id, m, 0.9);
            }
        }
        if st {
            self.snap();
        }
        if chg {
            if let Some(c) = self.p.channel_mut(id) {
                c.synth = s;
            }
            self.send();
        }
    }

    // ---------------- NN Tune ----------------
    fn tune_window(&mut self, ui: &mut Ui, id: u32) {
        let Some(mut t) = self.p.channel(id).map(|c| c.tunefx) else { return };
        let pat = self.sel_pat;
        let (mut st, mut chg) = (false, false);
        ui.horizontal(|ui| {
            if ui.checkbox(&mut t.on, "Corrección activa").changed() {
                st = true;
                chg = true;
            }
            ui.label(RichText::new("TÓNICA").small().color(DIM));
            let mut key = t.key;
            egui::ComboBox::from_id_salt(("key", id)).width(60.0).selected_text(NOTE_NAMES[key as usize % 12]).show_ui(ui, |ui| {
                for (i, n) in NOTE_NAMES.iter().enumerate() {
                    ui.selectable_value(&mut key, i as u8, *n);
                }
            });
            let mut sc = t.scale;
            let names = ["Menor", "Mayor", "Cromática"];
            egui::ComboBox::from_id_salt(("scale", id)).width(100.0).selected_text(names[sc as usize % 3]).show_ui(ui, |ui| {
                for (i, n) in names.iter().enumerate() {
                    ui.selectable_value(&mut sc, i as u8, *n);
                }
            });
            if key != t.key || sc != t.scale {
                t.key = key;
                t.scale = sc;
                st = true;
                chg = true;
            }
        });
        let mut released = false;
        ui.horizontal(|ui| {
            let r = knob(ui, &mut t.speed, 0.0, 1.0, 0.15, "Velocidad", |v| if v < 0.04 { "robot".into() } else { format!("{:.0}%", v * 100.0) });
            st |= r.drag_started() || r.double_clicked();
            chg |= r.changed();
            released |= r.drag_stopped() || r.double_clicked();
            let r = knob(ui, &mut t.mix, 0.0, 1.0, 1.0, "Mezcla", |v| format!("{:.0}%", v * 100.0));
            st |= r.drag_started() || r.double_clicked();
            chg |= r.changed();
            released |= r.drag_stopped() || r.double_clicked();
            let r = knob(ui, &mut t.gain, 0.0, 2.0, 1.0, "Ganancia", |v| format!("{:+.1} dB", 20.0 * v.max(1e-4).log10()));
            st |= r.drag_started() || r.double_clicked();
            chg |= r.changed();
            released |= r.drag_stopped() || r.double_clicked();
            ui.add_space(12.0);
            ui.vertical(|ui| {
                let recording = self.rec.as_ref().is_some_and(|r| r.ch == id);
                if recording {
                    let el = self.rec.as_ref().map(|r| r.started.elapsed().as_secs_f32()).unwrap_or(0.0);
                    let mx = self.rec.as_ref().map(|r| r.max_s).unwrap_or(0.0);
                    if icon_button(ui, Ic::Stop, Vec2::new(120.0, 30.0), false, Some(RED)).on_hover_text("Detener la grabación").clicked() {
                        self.rec_stop();
                    }
                    ui.label(RichText::new(format!("Grabando {el:.1} / {mx:.1} s")).color(RED));
                    let lvl = self.in_meter;
                    let (r, _) = ui.allocate_exact_size(Vec2::new(120.0, 8.0), Sense::hover());
                    ui.painter().rect_filled(r, 2, Color32::from_rgb(11, 13, 15));
                    let f = ((20.0 * lvl.max(1e-5).log10() + 48.0) / 48.0).clamp(0.0, 1.0);
                    ui.painter().rect_filled(Rect::from_min_size(r.min, Vec2::new(r.width() * f, r.height())), 2, if f > 0.9 { RED } else { GREEN });
                } else {
                    let txt = format!("Grabar en «{}»", self.cur_pat().name);
                    if ui.add(egui::Button::new(RichText::new(txt.clone()).color(Color32::WHITE)).fill(Color32::from_rgb(150, 40, 40)).min_size(Vec2::new(150.0, 30.0))).clicked() {
                        self.rec_start(id);
                    }
                    if self.raws.contains_key(&(pat, id)) && ui.button("Borrar toma").clicked() {
                        self.delete_take(pat, id);
                    }
                }
                if self.tune_busy > 0 {
                    ui.label(RichText::new("Procesando afinación…").small().color(ACCENT2));
                }
            });
        });
        // gráfico de afinación
        let (rect, _) = ui.allocate_exact_size(Vec2::new(640.0, 200.0), Sense::hover());
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 5, Color32::from_rgb(14, 16, 19));
        match self.takes.get(&(pat, id)) {
            None => {
                p.text(rect.center(), Align2::CENTER_CENTER, "Sin toma en este patrón: pulsa «Grabar» y canta mientras suena el beat.", FontId::proportional(13.0), DIM);
            }
            Some(v) => {
                // forma de onda tenue
                let n = v.peaks.len().max(1);
                for (i, pk) in v.peaks.iter().enumerate() {
                    let x = rect.left() + i as f32 / n as f32 * rect.width();
                    let h = pk * rect.height() * 0.45;
                    p.vline(x, (rect.center().y - h)..=(rect.center().y + h), Stroke::new(1.0, Color32::from_rgb(38, 44, 52)));
                }
                let voiced: Vec<f32> = v.det.iter().copied().filter(|m| *m > 0.0).collect();
                let (lo, hi) = if voiced.is_empty() { (48.0, 72.0) } else { (voiced.iter().cloned().fold(f32::MAX, f32::min).floor() - 2.0, voiced.iter().cloned().fold(f32::MIN, f32::max).ceil() + 2.0) };
                let rg = (hi - lo).max(6.0);
                let y = |m: f32| rect.bottom() - (m - lo) / rg * rect.height();
                for m in lo as i32..=hi as i32 {
                    if fx::scale_has(t.key, t.scale, m) {
                        p.hline(rect.x_range(), y(m as f32), Stroke::new(1.0, if (m - t.key as i32).rem_euclid(12) == 0 { Color32::from_rgb(90, 70, 40) } else { Color32::from_rgb(32, 36, 42) }));
                        p.text(Pos2::new(rect.left() + 3.0, y(m as f32) - 1.0), Align2::LEFT_BOTTOM, nname(m as u8), FontId::proportional(8.5), Color32::from_rgb(90, 98, 110));
                    }
                }
                let nf = v.det.len().max(1);
                let line = |vals: &[f32], col: Color32, w: f32| {
                    let mut seg: Vec<Pos2> = vec![];
                    for (i, m) in vals.iter().enumerate() {
                        if *m > 0.0 {
                            seg.push(Pos2::new(rect.left() + i as f32 / nf as f32 * rect.width(), y(*m)));
                        } else if !seg.is_empty() {
                            p.add(egui::Shape::line(std::mem::take(&mut seg), Stroke::new(w, col)));
                        }
                    }
                    if !seg.is_empty() {
                        p.add(egui::Shape::line(seg, Stroke::new(w, col)));
                    }
                };
                line(&v.det, Color32::from_rgb(150, 160, 175), 1.2);
                if t.on {
                    line(&v.cor, ACCENT, 2.0);
                }
                p.text(rect.right_top() + Vec2::new(-6.0, 6.0), Align2::RIGHT_TOP, format!("{:.1} s · gris: voz original · naranja: corregida", v.dur), FontId::proportional(10.0), DIM);
            }
        }
        if st {
            self.snap();
        }
        if chg {
            if let Some(c) = self.p.channel_mut(id) {
                c.tunefx = t;
            }
            self.send();
            let knob_drag = ui.ctx().input(|i| i.pointer.any_down());
            if !knob_drag || released {
                self.retune_all(id);
            }
        } else if released {
            self.retune_all(id);
        }
    }

    // ---------------- tomas de voz ----------------
    pub(super) fn rec_start(&mut self, ch: u32) {
        if self.rec.is_some() {
            return;
        }
        if self.audio.is_none() {
            self.msg = "No hay salida de audio.".into();
            return;
        }
        match audio::record(self.status.clone()) {
            Ok(r) => {
                let max_s = (self.cur_pat().len_steps() as f64 * self.p.step_secs()) as f32;
                let pat = self.sel_pat;
                self.song_mode = false;
                self.play(0);
                self.rec = Some(RecState { r, pat, ch, max_s, started: Instant::now() });
                self.msg = format!("Grabando en «{}» ({:.1} s)…", self.cur_pat().name, max_s);
            }
            Err(e) => self.msg = format!("No se pudo grabar: {e}"),
        }
    }

    pub(super) fn rec_stop(&mut self) {
        let Some(rs) = self.rec.take() else { return };
        self.stop();
        let sr = rs.r.sr;
        let mut data = rs.r.buf.lock().map(|b| b.clone()).unwrap_or_default();
        drop(rs.r);
        data.truncate((rs.max_s * sr as f32) as usize);
        if data.len() < sr as usize / 4 {
            self.msg = "La toma es demasiado corta.".into();
            return;
        }
        let dir = self.plat.data_dir.join("audio");
        let _ = std::fs::create_dir_all(&dir);
        let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let name = format!("toma_{}_{}_{}.wav", rs.pat, rs.ch, ts);
        if let Err(e) = export::write_wav_mono(&dir.join(&name), &data, sr) {
            self.msg = format!("No se pudo guardar la toma: {e}");
            return;
        }
        self.snap();
        if let Some(p) = self.p.pattern_mut(rs.pat) {
            p.audio.insert(rs.ch, name);
        }
        self.send();
        self.raws.insert((rs.pat, rs.ch), (Arc::new(data), sr as f32));
        self.retune(rs.pat, rs.ch);
        self.msg = "Toma grabada. NN Tune la está afinando…".into();
    }

    pub(super) fn delete_take(&mut self, pat: u32, ch: u32) {
        self.snap();
        if let Some(p) = self.p.pattern_mut(pat) {
            p.audio.remove(&ch);
        }
        self.raws.remove(&(pat, ch));
        self.takes.remove(&(pat, ch));
        self.samples.remove(&(pat, ch));
        self.cmd(Cmd::Sample { pat, ch, data: None });
        self.send();
    }

    /// Carga (otra vez) todas las tomas del proyecto y las procesa con NN Tune.
    pub(super) fn reload_takes(&mut self) {
        let mut wanted = vec![];
        for p in &self.p.patterns {
            for (ch, file) in &p.audio {
                wanted.push((p.id, *ch, file.clone()));
            }
        }
        let old: Vec<(u32, u32)> = self.samples.keys().copied().collect();
        for k in old {
            if !wanted.iter().any(|w| (w.0, w.1) == k) {
                self.samples.remove(&k);
                self.takes.remove(&k);
                self.raws.remove(&k);
                self.cmd(Cmd::Sample { pat: k.0, ch: k.1, data: None });
            }
        }
        for (pat, ch, file) in wanted {
            if let Some((x, sr)) = export::read_wav_mono(&self.plat.data_dir.join("audio").join(&file)) {
                self.raws.insert((pat, ch), (Arc::new(x), sr as f32));
                self.retune(pat, ch);
            }
        }
    }

    fn retune_all(&mut self, ch: u32) {
        let keys: Vec<(u32, u32)> = self.raws.keys().copied().filter(|k| k.1 == ch).collect();
        for (p, c) in keys {
            self.retune(p, c);
        }
    }

    pub(super) fn retune(&mut self, pat: u32, ch: u32) {
        let Some((raw, sr)) = self.raws.get(&(pat, ch)).cloned() else { return };
        let tp = self.p.channel(ch).map(|c| c.tunefx).unwrap_or_default();
        let tx = self.tune_tx.clone();
        self.tune_busy += 1;
        std::thread::spawn(move || {
            let res = fx::autotune(&raw, sr, &tp);
            let _ = tx.send(TuneDone { pat, ch, sr, res });
        });
    }

    pub(super) fn poll_tune(&mut self) {
        while let Ok(d) = self.tune_rx.try_recv() {
            self.tune_busy = self.tune_busy.saturating_sub(1);
            let buckets = 600;
            let n = d.res.out.len();
            let raw = self.raws.get(&(d.pat, d.ch)).map(|r| r.0.clone());
            let peaks = match &raw {
                Some(x) => (0..buckets).map(|b| {
                    let a = b * x.len() / buckets;
                    let e = ((b + 1) * x.len() / buckets).max(a + 1).min(x.len());
                    x[a..e].iter().fold(0f32, |m, v| m.max(v.abs()))
                }).collect(),
                None => vec![],
            };
            self.takes.insert((d.pat, d.ch), TakeView { peaks, det: d.res.detected, cor: d.res.corrected, dur: n as f32 / d.sr });
            let data = Arc::new(d.res.out);
            self.samples.insert((d.pat, d.ch), (data.clone(), d.sr));
            self.cmd(Cmd::Sample { pat: d.pat, ch: d.ch, data: Some((data, d.sr)) });
            if self.tune_busy == 0 && self.msg.contains("afinando") {
                self.msg = "Toma lista y afinada con NN Tune.".into();
            }
        }
    }
}

const TAU_F: f32 = std::f32::consts::TAU;

fn gr_bar(ui: &mut Ui, gr_db: f32) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("GR").small().color(DIM));
        let (r, _) = ui.allocate_exact_size(Vec2::new(140.0, 10.0), Sense::hover());
        let p = ui.painter();
        p.rect_filled(r, 2, Color32::from_rgb(11, 13, 15));
        let f = (gr_db / 24.0).clamp(0.0, 1.0);
        p.rect_filled(Rect::from_min_max(Pos2::new(r.right() - r.width() * f, r.top()), r.right_bottom()), 2, ACCENT);
        ui.label(RichText::new(format!("-{:.1} dB", gr_db.abs())).small().monospace().color(ACCENT2));
    });
}

fn adsr_view(ui: &mut Ui, e: &[f32; 4]) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(168.0, 50.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 4, Color32::from_rgb(12, 14, 17));
    let total = e[0] + e[1] + 0.4 + e[3];
    let sx = rect.width() / total.max(0.01);
    let y = |v: f32| rect.bottom() - 4.0 - v * (rect.height() - 8.0);
    let x0 = rect.left();
    let pts = vec![
        Pos2::new(x0, y(0.0)),
        Pos2::new(x0 + e[0] * sx, y(1.0)),
        Pos2::new(x0 + (e[0] + e[1]) * sx, y(e[2])),
        Pos2::new(x0 + (e[0] + e[1] + 0.4) * sx, y(e[2])),
        Pos2::new(x0 + total * sx, y(0.0)),
    ];
    p.add(egui::Shape::line(pts, Stroke::new(2.0, ACCENT)));
}
