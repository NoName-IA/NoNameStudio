//! Piano Roll en el teléfono: la cuadrícula ocupa casi toda la pantalla; regla de compases arriba,
//! barra de transporte abajo, herramientas en un panel lateral (pestaña en el borde derecho) y un
//! teclado de piano para tocar en vivo con varios dedos.
use super::*;

/// Dibuja un ícono de piano chiquito (para el botón del teclado).
fn icono_piano(p: &egui::Painter, c: Pos2, col: Color32) {
    let r = Rect::from_center_size(c, Vec2::new(18.0, 13.0));
    p.rect_stroke(r, 2, Stroke::new(1.4, col), StrokeKind::Inside);
    for k in 1..4 {
        let x = r.left() + r.width() * k as f32 / 4.0;
        p.vline(x, r.y_range(), Stroke::new(1.0, col));
    }
    for k in [1.0f32, 2.0, 3.0] {
        let x = r.left() + r.width() * k / 4.0;
        p.rect_filled(Rect::from_center_size(Pos2::new(x, r.top() + 3.5), Vec2::new(3.0, 7.0)), 0, col);
    }
}

/// Botón redondo con un dibujo propio y un rótulo chiquito debajo (como en la barra de transporte).
fn boton_redondo(ui: &mut Ui, rotulo: &str, encendido: bool, dibujo: impl FnOnce(&egui::Painter, Pos2, Color32)) -> Response {
    let (r, resp) = ui.allocate_exact_size(Vec2::new(54.0, 52.0), Sense::click());
    let t = ui.ctx().animate_bool_with_time(resp.id, resp.hovered() || resp.is_pointer_button_down_on(), 0.1);
    let p = ui.painter();
    let c = Pos2::new(r.center().x, r.top() + 20.0);
    let fondo = if encendido { Color32::from_gray(230) } else { lerp_col(Color32::from_gray(30), Color32::from_gray(48), t) };
    p.circle_filled(c, 17.0, fondo);
    p.circle_stroke(c, 17.0, Stroke::new(1.0, Color32::from_gray(90)));
    let tinta = if encendido { Color32::from_gray(20) } else { Color32::from_gray(230) };
    dibujo(p, c, tinta);
    p.text(Pos2::new(r.center().x, r.bottom() - 6.0), Align2::CENTER_CENTER, rotulo, FontId::proportional(9.5), Color32::from_gray(160));
    resp
}

impl StudioApp {
    /// ¿El Piano Roll está a pantalla completa con el diseño estilo FL Studio? (teléfono y PC)
    pub(super) fn roll_cel(&self) -> bool {
        self.roll_es_completo()
    }

    /// Barra de transporte de abajo: teclado, grabar, inicio, play, tempo, más y salir.
    pub(super) fn roll_transporte(&mut self, ui: &mut Ui) {
        // PC: Esc sale del Piano Roll (si no hay un menú abierto)
        if !self.plat.mobile && self.roll_menu.is_none() && !ui.ctx().any_popup_open() && ui.input(|i| i.key_pressed(Key::Escape)) {
            self.roll_volver();
            return;
        }
        egui::Panel::bottom("roll_transporte").exact_size(60.0).frame(Frame::new().fill(Color32::from_gray(14)).stroke(Stroke::new(1.0, Color32::from_gray(40)))).show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                ui.add_space(8.0);
                if boton_redondo(ui, "TECLADO", self.roll_teclado, |p, c, col| icono_piano(p, c, col)).clicked() {
                    self.roll_teclado = !self.roll_teclado;
                }
                if boton_redondo(ui, "FUERZA", self.roll_fuerza_cel, |p, c, col| {
                    for (k, h) in [4.0f32, 9.0, 6.0, 11.0].iter().enumerate() {
                        let x = c.x - 6.0 + k as f32 * 4.0;
                        p.vline(x, (c.y + 6.0 - h)..=(c.y + 6.0), Stroke::new(2.0, col));
                    }
                })
                .clicked()
                {
                    self.roll_fuerza_cel = !self.roll_fuerza_cel;
                }
                // el centro: grabar, inicio, play, tempo y más
                let ancho_centro = 5.0 * 58.0;
                let libre = ui.available_width() - ancho_centro - 70.0;
                ui.add_space((libre / 2.0 - 54.0).max(4.0));
                let grabando = self.rec_track.is_some() || self.midi_rec;
                if boton_redondo(ui, "GRABAR", grabando, |p, c, _| { p.circle_filled(c, 5.5, RED); }).on_hover_text("Grabar lo que toques en el teclado").clicked() {
                    self.midi_rec = !self.midi_rec;
                    if self.midi_rec && !self.playing() {
                        self.toggle_play();
                    }
                }
                if boton_redondo(ui, "INICIO", false, |p, c, col| {
                    p.vline(c.x - 5.0, (c.y - 5.0)..=(c.y + 5.0), Stroke::new(2.0, col));
                    p.add(egui::Shape::convex_polygon(vec![c + Vec2::new(5.0, -5.5), c + Vec2::new(5.0, 5.5), c + Vec2::new(-3.0, 0.0)], col, Stroke::NONE));
                })
                .clicked()
                {
                    self.play_from = 0;
                    if self.playing() {
                        self.play(0);
                    }
                    self.roll_seguir = Some(0.0);
                }
                let sonando = self.playing();
                if boton_redondo(ui, if sonando { "PAUSA" } else { "PLAY" }, sonando, |p, c, col| {
                    if sonando {
                        p.rect_filled(Rect::from_center_size(c - Vec2::new(3.0, 0.0), Vec2::new(3.0, 11.0)), 1, col);
                        p.rect_filled(Rect::from_center_size(c + Vec2::new(3.0, 0.0), Vec2::new(3.0, 11.0)), 1, col);
                    } else {
                        p.add(egui::Shape::convex_polygon(vec![c + Vec2::new(-4.0, -6.5), c + Vec2::new(-4.0, 6.5), c + Vec2::new(6.5, 0.0)], col, Stroke::NONE));
                    }
                })
                .clicked()
                {
                    self.toggle_play();
                }
                // tempo: se cambia arrastrando el número
                ui.vertical(|ui| {
                    ui.add_space(6.0);
                    let mut bpm = self.p.bpm;
                    let r = ui.add_sized([54.0, 26.0], egui::DragValue::new(&mut bpm).range(50.0..=200.0).speed(0.25).fixed_decimals(0));
                    if r.changed() {
                        self.p.bpm = bpm.round();
                        self.cambios = true;
                        self.send();
                    }
                    ui.label(RichText::new("BPM").size(9.5).color(Color32::from_gray(160)));
                });
                if boton_redondo(ui, "MÁS", self.roll_panel, |p, c, col| {
                    for k in -1..=1 {
                        p.circle_filled(c + Vec2::new(k as f32 * 5.5, 0.0), 1.8, col);
                    }
                })
                .clicked()
                {
                    self.roll_panel = !self.roll_panel;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_space(8.0);
                    if boton_redondo(ui, "SALIR", false, |p, c, col| {
                        p.line_segment([c + Vec2::new(-5.0, -5.0), c + Vec2::new(5.0, 5.0)], Stroke::new(2.0, col));
                        p.line_segment([c + Vec2::new(-5.0, 5.0), c + Vec2::new(5.0, -5.0)], Stroke::new(2.0, col));
                    })
                    .clicked()
                    {
                        self.roll_volver();
                    }
                    for (m, t) in [(true, "SONG"), (false, "PAT")] {
                        if ui.add(egui::Button::selectable(self.song_mode == m, t)).clicked() {
                            self.set_song_mode(m);
                        }
                    }
                });
            });
        });
    }

    /// Pestaña de medio círculo en el borde derecho que abre y cierra el panel de herramientas.
    pub(super) fn roll_pestana(&mut self, ctx: &egui::Context, borde: Rect) {
        let r = Rect::from_center_size(Pos2::new(borde.right() - 18.0, borde.center().y), Vec2::new(36.0, 74.0));
        let resp = egui::Area::new(egui::Id::new("roll_pestana")).order(egui::Order::Foreground).fixed_pos(r.min).show(ctx, |ui| {
            let (rr, resp) = ui.allocate_exact_size(r.size(), Sense::click());
            let p = ui.painter();
            let c = Pos2::new(rr.right(), rr.center().y);
            // medio círculo pegado al borde
            let mut pts = vec![];
            for k in 0..=24 {
                let a = std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * k as f32 / 24.0;
                pts.push(c + Vec2::new(a.cos(), a.sin()) * 36.0);
            }
            p.add(egui::Shape::convex_polygon(pts, Color32::from_gray(232), Stroke::new(1.0, Color32::from_gray(120))));
            let d = if self.roll_panel { 1.0 } else { -1.0 };
            let m = Pos2::new(c.x - 16.0, c.y);
            let s = Stroke::new(2.2, Color32::from_gray(30));
            p.line_segment([m + Vec2::new(-4.0 * d, -9.0), m + Vec2::new(4.0 * d, 0.0)], s);
            p.line_segment([m + Vec2::new(4.0 * d, 0.0), m + Vec2::new(-4.0 * d, 9.0)], s);
            resp
        });
        if resp.inner.clicked() {
            self.roll_panel = !self.roll_panel;
        }
    }

    /// Regla de compases arriba de la cuadrícula (fija al desplazarse): marcas y número de cada compás.
    pub(super) fn roll_regla(&self, painter: &egui::Painter, rect: Rect, vp: Rect, gx: f32, cw: f32, steps: u32) {
        let alto = 24.0;
        // pegada arriba de lo que se ve (el pintor viene recortado a la vista)
        let _ = (rect, vp);
        let vista = painter.clip_rect();
        let banda = Rect::from_min_max(vista.min, Pos2::new(vista.right(), vista.top() + alto));
        painter.rect_filled(banda, 0, Color32::from_gray(16));
        painter.hline(banda.x_range(), banda.bottom() - 0.5, Stroke::new(1.0, Color32::from_gray(60)));
        for s in 0..=steps {
            let x = gx + s as f32 * cw;
            if x < banda.left() + KW - 2.0 || x > banda.right() {
                continue;
            }
            if s % 16 == 0 {
                painter.vline(x, (banda.bottom() - 9.0)..=banda.bottom(), Stroke::new(1.5, Color32::from_gray(170)));
                // número del compás en un circulito
                let c = Pos2::new(x, banda.top() + 9.0);
                painter.circle_filled(c, 8.0, Color32::from_gray(232));
                painter.text(c, Align2::CENTER_CENTER, format!("{}", s / 16 + 1), FontId::proportional(9.5), Color32::from_gray(20));
            } else if s % 4 == 0 {
                painter.vline(x, (banda.bottom() - 6.0)..=banda.bottom(), Stroke::new(1.0, Color32::from_gray(110)));
            }
        }
        if let Some(ls) = self.local_step() {
            let x = gx + ls as f32 * cw;
            if x >= banda.left() + KW {
                painter.add(egui::Shape::convex_polygon(vec![Pos2::new(x - 5.0, banda.bottom() - 7.0), Pos2::new(x + 5.0, banda.bottom() - 7.0), Pos2::new(x, banda.bottom())], ACCENT2, Stroke::NONE));
            }
        }
    }

    /// Teclado de piano para tocar en vivo con varios dedos (suena en el canal del Piano Roll).
    pub(super) fn roll_teclado_vivo(&mut self, ui: &mut Ui, ch: u32) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("OCTAVA").small().color(DIM));
            if ui.small_button("−").clicked() {
                self.roll_octava = (self.roll_octava - 1).max(1);
            }
            ui.label(RichText::new(format!("C{}", self.roll_octava)).strong());
            if ui.small_button("+").clicked() {
                self.roll_octava = (self.roll_octava + 1).min(6);
            }
            if self.midi_rec {
                ui.label(RichText::new("● grabando en el patrón").small().color(RED));
            }
        });
        let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        // teclas blancas visibles según el ancho (mínimo 2 octavas)
        let blancas = ((rect.width() / 46.0).floor() as usize).max(14);
        let wkw = rect.width() / blancas as f32;
        let base = (self.roll_octava as i32 + 1) * 12; // C de la octava elegida (C4 = 60)
        const PASO_BLANCA: [i32; 7] = [0, 2, 4, 5, 7, 9, 11];
        let nota_blanca = |k: usize| base + (k / 7) as i32 * 12 + PASO_BLANCA[k % 7];
        let tiene_negra = |k: usize| !matches!(k % 7, 2 | 6);
        let negra_rect = |k: usize| Rect::from_min_size(Pos2::new(rect.left() + (k as f32 + 1.0) * wkw - wkw * 0.3, rect.top()), Vec2::new(wkw * 0.6, rect.height() * 0.6));
        let nota_en = |p: Pos2| -> Option<u8> {
            if !rect.contains(p) {
                return None;
            }
            for k in 0..blancas {
                if tiene_negra(k) && negra_rect(k).contains(p) {
                    return Some((nota_blanca(k) + 1).clamp(0, 127) as u8);
                }
            }
            let k = (((p.x - rect.left()) / wkw).floor() as usize).min(blancas - 1);
            Some(nota_blanca(k).clamp(0, 127) as u8)
        };
        // cada dedo (o el mouse) toca su nota; al deslizar cambia de tecla
        let mut ahora: std::collections::HashMap<u64, u8> = Default::default();
        ui.input(|i| {
            for e in &i.events {
                if let egui::Event::Touch { id, phase, pos, .. } = e {
                    match phase {
                        egui::TouchPhase::Start | egui::TouchPhase::Move => {
                            self.roll_dedos.insert(id.0, *pos);
                        }
                        _ => {
                            self.roll_dedos.remove(&id.0);
                        }
                    }
                }
            }
        });
        for (id, p) in &self.roll_dedos {
            if let Some(n) = nota_en(*p) {
                ahora.insert(*id, n);
            }
        }
        if self.roll_dedos.is_empty() && resp.is_pointer_button_down_on() {
            if let Some(n) = resp.interact_pointer_pos().and_then(nota_en) {
                ahora.insert(u64::MAX, n);
            }
        }
        // apagar las que se soltaron o cambiaron, prender las nuevas
        let antes = std::mem::take(&mut self.roll_tocando);
        for (id, n) in &antes {
            if ahora.get(id) != Some(n) {
                self.cmd(Cmd::NoteOff { ch, m: *n });
            }
        }
        for (id, n) in &ahora {
            if antes.get(id) != Some(n) {
                let vel = self.note_vel;
                self.cmd(Cmd::NoteOn { ch, m: *n, v: vel });
                // grabando: la nota queda escrita en el patrón donde va el cabezal
                if self.midi_rec {
                    if let Some(pp) = self.local_step() {
                        let paso = pp.round() as u32;
                        let largo = self.note_len.max(1);
                        let pid = self.sel_pat;
                        if let Some(pat) = self.p.pattern_mut(pid) {
                            let len = pat.len_steps();
                            pat.notes.entry(ch).or_default().push(Note { s: paso % len, l: largo.min(len - paso % len), m: *n, v: vel, o: 0.0 });
                        }
                        self.cambios = true;
                        self.send();
                    }
                }
            }
        }
        self.roll_tocando = ahora;
        if !self.roll_tocando.is_empty() || !self.roll_dedos.is_empty() {
            ui.ctx().request_repaint();
        }
        // dibujo: blancas con su nombre abajo (C con la octava) y negras encima
        let sonando: Vec<u8> = self.roll_tocando.values().copied().collect();
        let p = ui.painter();
        for k in 0..blancas {
            let n = nota_blanca(k);
            let r = Rect::from_min_size(Pos2::new(rect.left() + k as f32 * wkw, rect.top()), Vec2::new(wkw, rect.height()));
            let on = sonando.contains(&(n as u8));
            p.rect_filled(r.shrink2(Vec2::new(0.5, 0.0)), 0, if on { Color32::from_gray(185) } else { Color32::from_gray(238) });
            p.vline(r.left(), r.y_range(), Stroke::new(1.0, Color32::from_gray(120)));
            let es_do = n % 12 == 0;
            let nombre = if es_do { format!("C{}", n / 12 - 1) } else { ["C", "D", "E", "F", "G", "A", "B"][k % 7].to_string() };
            p.text(Pos2::new(r.center().x, r.bottom() - 10.0), Align2::CENTER_CENTER, nombre, FontId::proportional(if es_do { 12.0 } else { 10.0 }), Color32::from_gray(if es_do { 30 } else { 140 }));
            if es_do {
                p.rect_filled(Rect::from_min_size(Pos2::new(r.left() + 3.0, r.bottom() - 3.0), Vec2::new(r.width() - 6.0, 3.0)), 1, ACCENT);
            }
        }
        for k in 0..blancas {
            if tiene_negra(k) && k + 1 < blancas {
                let n = nota_blanca(k) + 1;
                let on = sonando.contains(&(n as u8));
                p.rect_filled(negra_rect(k), 2, if on { Color32::from_gray(90) } else { Color32::from_gray(12) });
            }
        }
    }

    /// Apaga las notas del teclado en vivo (al cerrarlo o salir del Piano Roll).
    pub(super) fn roll_teclado_soltar(&mut self, ch: u32) {
        let viejas: Vec<u8> = self.roll_tocando.drain().map(|(_, n)| n).collect();
        for n in viejas {
            self.cmd(Cmd::NoteOff { ch, m: n });
        }
        self.roll_dedos.clear();
    }
}
