//! Panel «Masterizando» (a la derecha) y su píldora en la barra de arriba:
//! estilo, intensidad, ecualizador de 3 bandas, canción de referencia y comparar con/sin master.
use super::*;

/// Interruptor redondo (encendido/apagado).
pub(super) fn interruptor(ui: &mut Ui, on: &mut bool) -> Response {
    let (r, mut resp) = ui.allocate_exact_size(Vec2::new(34.0, 18.0), Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let t = ui.ctx().animate_bool_with_time(resp.id, *on, 0.15);
    let p = ui.painter();
    p.rect_filled(r, 99, lerp_col(Color32::from_gray(55), ACCENT, t));
    let x = egui::lerp(r.left() + 9.0..=r.right() - 9.0, t);
    p.circle_filled(Pos2::new(x, r.center().y), 6.5, lerp_col(Color32::from_gray(200), Color32::from_gray(20), t));
    resp
}

/// Icono de cada estilo de masterizado, dibujado con líneas.
fn icono_estilo(p: &egui::Painter, i: usize, c: Pos2, col: Color32) {
    let s = Stroke::new(1.6, col);
    let v = |x: f32, y: f32| c + Vec2::new(x, y);
    match i {
        // Universal: dos corcheas unidas
        0 => {
            p.circle_stroke(v(-6.0, 6.0), 3.0, s);
            p.circle_stroke(v(6.0, 4.0), 3.0, s);
            p.line_segment([v(-3.0, 6.0), v(-3.0, -8.0)], s);
            p.line_segment([v(9.0, 4.0), v(9.0, -10.0)], s);
            p.line_segment([v(-3.0, -8.0), v(9.0, -10.0)], Stroke::new(2.4, col));
        }
        // Fuego: llama
        1 => {
            let ext = [(0.0, -11.0), (5.0, -4.0), (8.0, 2.0), (7.0, 7.0), (3.0, 10.0), (-3.0, 10.0), (-7.0, 7.0), (-8.0, 2.0), (-6.0, -3.0), (-4.0, 0.0), (-2.0, -6.0)];
            p.add(egui::Shape::closed_line(ext.iter().map(|(x, y)| v(*x, *y)).collect(), s));
            let int = [(0.0, 1.0), (3.0, 5.0), (2.0, 8.0), (-2.0, 8.0), (-3.0, 5.0)];
            p.add(egui::Shape::closed_line(int.iter().map(|(x, y)| v(*x, *y)).collect(), Stroke::new(1.3, col)));
        }
        // Claridad: diamante
        2 => {
            let pts = vec![v(-10.0, -3.0), v(-5.0, -9.0), v(5.0, -9.0), v(10.0, -3.0), v(0.0, 10.0)];
            p.add(egui::Shape::closed_line(pts, s));
            p.line_segment([v(-10.0, -3.0), v(10.0, -3.0)], s);
            p.line_segment([v(-3.0, -3.0), v(0.0, 10.0)], s);
            p.line_segment([v(3.0, -3.0), v(0.0, 10.0)], s);
        }
        // Cinta: casete
        3 => {
            p.rect_stroke(Rect::from_center_size(c, Vec2::new(22.0, 15.0)), 3, s, StrokeKind::Middle);
            p.circle_stroke(v(-5.0, -1.0), 2.5, s);
            p.circle_stroke(v(5.0, -1.0), 2.5, s);
            p.line_segment([v(-6.0, 7.5), v(-4.0, 4.0)], s);
            p.line_segment([v(-4.0, 4.0), v(4.0, 4.0)], s);
            p.line_segment([v(4.0, 4.0), v(6.0, 7.5)], s);
        }
        // Equilibrio: balanza
        4 => {
            p.line_segment([v(0.0, -9.0), v(0.0, 9.0)], s);
            p.line_segment([v(-6.0, 9.0), v(6.0, 9.0)], s);
            p.line_segment([v(-10.0, -6.0), v(10.0, -6.0)], s);
            for x in [-8.0f32, 8.0] {
                let pts: Vec<Pos2> = (0..=8).map(|k| {
                    let a = k as f32 / 8.0 * std::f32::consts::PI;
                    v(x + a.cos() * 4.0, -1.0 + a.sin() * 3.0)
                }).collect();
                p.add(egui::Shape::line(pts, s));
                p.line_segment([v(x, -6.0), v(x - 4.0, -1.0)], Stroke::new(1.0, col));
                p.line_segment([v(x, -6.0), v(x + 4.0, -1.0)], Stroke::new(1.0, col));
            }
        }
        // Espacio: espiral
        5 => {
            let pts: Vec<Pos2> = (0..60)
                .map(|k| {
                    let t = k as f32 / 60.0;
                    let a = t * std::f32::consts::TAU * 2.5;
                    let r = 1.0 + t * 10.0;
                    v(a.cos() * r, a.sin() * r)
                })
                .collect();
            p.add(egui::Shape::line(pts, s));
        }
        // Cine: carrete de película
        6 => {
            p.circle_stroke(c, 10.0, s);
            p.circle_filled(c, 1.6, col);
            for k in 0..5 {
                let a = k as f32 / 5.0 * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
                p.circle_stroke(c + Vec2::angled(a) * 5.8, 2.4, Stroke::new(1.3, col));
            }
        }
        // Brillo: destello
        _ => {
            for k in 0..8 {
                let a = k as f32 / 8.0 * std::f32::consts::TAU;
                let (r0, r1) = if k % 2 == 0 { (4.0, 11.0) } else { (4.0, 7.0) };
                p.line_segment([c + Vec2::angled(a) * r0, c + Vec2::angled(a) * r1], s);
            }
            p.circle_stroke(c, 2.0, s);
        }
    }
}

impl StudioApp {
    /// Aplica al instante el ecualizador y el compresor del estilo elegido (sin medir el volumen).
    pub(super) fn master_en_vivo(&mut self) {
        let eq3 = if self.master_eq_on { self.master_eq3 } else { [0.0; 3] };
        let (eq, comp) = cadena_master(self.master_estilo, self.master_int, eq3);
        self.p.master_eq = eq;
        self.p.master_comp = comp;
        self.p.limiter.on = true;
        self.cambios = true;
        self.send();
    }

    /// Activa o desactiva el masterizado (para comparar).
    pub(super) fn poner_sin_master(&mut self, sin: bool) {
        self.sin_master = sin;
        self.cmd(Cmd::SinMaster(sin));
    }

    /// Píldora de la barra de arriba: estilo + interruptor, y volumen general (de derecha a izquierda).
    pub(super) fn pildora_master(&mut self, ui: &mut Ui, compact: bool) {
        if !compact {
            // volumen general (de derecha a izquierda: valor, barra, icono)
            let db = 20.0 * self.p.master.max(1e-4).log10();
            ui.label(RichText::new(if self.p.master < 0.001 { "−∞ dB".into() } else { format!("{db:+.1} dB") }).monospace().size(11.0).color(DIM));
            ui.spacing_mut().slider_width = 90.0;
            let mut m = self.p.master;
            let r = ui.add(egui::Slider::new(&mut m, 0.0..=1.4).show_value(false)).on_hover_text("Volumen general (doble clic: 0 dB)");
            if r.drag_started() {
                self.snap();
            }
            if r.double_clicked() {
                m = 1.0;
            }
            if m != self.p.master {
                self.p.master = m;
                self.cambios = true;
                self.send();
            }
        }
        let activo = self.mast_panel;
        Frame::new()
            .fill(if activo { Color32::from_gray(38) } else { Color32::from_gray(28) })
            .stroke(Stroke::new(1.0, if activo { Color32::from_gray(90) } else { Color32::from_gray(45) }))
            .corner_radius(99)
            .inner_margin(Margin::symmetric(10, 3))
            .show(ui, |ui| {
                // de derecha a izquierda: interruptor y después el nombre
                let mut on = !self.sin_master;
                if interruptor(ui, &mut on).on_hover_text("Escuchar con o sin el masterizado").changed() {
                    self.poner_sin_master(!on);
                }
                let (nombre, sub) = (MASTER_ESTILOS[self.master_estilo].0, if self.sin_master { "Sin master" } else { "Masterizado" });
                let (r, resp) = ui.allocate_exact_size(Vec2::new(if compact { 74.0 } else { 92.0 }, 26.0), Sense::click());
                let p = ui.painter();
                p.text(r.left_top() + Vec2::new(0.0, 1.0), Align2::LEFT_TOP, nombre, FontId::proportional(12.5), Color32::WHITE);
                p.text(r.left_bottom() + Vec2::new(0.0, -1.0), Align2::LEFT_BOTTOM, sub, FontId::proportional(10.0), DIM);
                if resp.on_hover_text("Abrir el masterizado").clicked() {
                    self.mast_panel = !self.mast_panel;
                }
            });
    }

    /// Panel derecho «Masterizando».
    pub(super) fn panel_masterizado(&mut self, ui: &mut Ui) {
        let mut vivo = false;
        ui.horizontal(|ui| {
            ui.label(RichText::new("Masterizando").strong().size(16.0));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if icon_button(ui, Ic::Close, Vec2::new(26.0, 24.0), false, None).on_hover_text("Cerrar").clicked() {
                    self.mast_panel = false;
                }
                let txt = if self.sin_master { "Sin master" } else { "Con master" };
                if ui.add(egui::Button::selectable(self.sin_master, RichText::new(txt).small())).on_hover_text("Compara cómo suena la canción sin el masterizado").clicked() {
                    let s = !self.sin_master;
                    self.poner_sin_master(s);
                }
            });
        });
        ui.add_space(8.0);
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            // ---------- referencia
            Frame::new().fill(PANEL2).corner_radius(10).inner_margin(Margin::same(12)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width((ui.available_width() - 40.0).max(80.0));
                        ui.label(RichText::new("Igualar a una referencia").strong());
                        match &self.master_ref {
                            Some((n, l, _)) => {
                                ui.label(RichText::new(format!("{n} · {l:.1} LUFS")).small().color(ACCENT2));
                            }
                            None => {
                                ui.label(RichText::new("Elige una canción que te guste para copiar su volumen y su balance de graves, medios y agudos").small().color(DIM));
                            }
                        }
                    });
                    if self.master_ref.is_some() {
                        if icon_button(ui, Ic::Close, Vec2::new(30.0, 30.0), false, None).on_hover_text("Quitar la referencia").clicked() {
                            self.master_ref = None;
                        }
                    } else if ui.add(egui::Button::new(RichText::new("+").size(18.0)).corner_radius(99).min_size(Vec2::new(32.0, 32.0))).on_hover_text("Elegir canción").clicked() {
                        if let Some(path) = self.pick_audio() {
                            match crate::media::decode(&path) {
                                Ok(b) => {
                                    let st: Vec<f32> = if b.ch == 1 { b.data.iter().flat_map(|v| [*v, *v]).collect() } else { b.data.clone() };
                                    let (l, _) = crate::loudness::measure(&st, b.sr);
                                    let bal = balance_3(&st, b.sr);
                                    let n = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                                    self.master_ref = Some((n, l, bal));
                                }
                                Err(e) => self.msg = format!("No se pudo leer la referencia: {e}"),
                            }
                        }
                    }
                });
            });
            ui.add_space(10.0);
            // ---------- estilos (4 por fila)
            // tarjetas de 4 por fila (8 si hay espacio, por ejemplo en el teléfono a pantalla completa), sin pasarse de tamaño
            let ancho = ui.available_width();
            let cols = if ancho >= 8.0 * 70.0 + 42.0 { 8 } else { 4 };
            let lado = ((ancho - 6.0 * (cols as f32 - 1.0)) / cols as f32).floor().min(96.0);
            for fila in 0..MASTER_ESTILOS.len().div_ceil(cols) {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    for i in fila * cols..(fila * cols + cols).min(MASTER_ESTILOS.len()) {
                        let (n, d, _, _) = MASTER_ESTILOS[i];
                        let sel = self.master_estilo == i;
                        let (r, resp) = ui.allocate_exact_size(Vec2::new(lado, lado * 0.92), Sense::click());
                        let t = ui.ctx().animate_bool_with_time(resp.id, sel, 0.18);
                        let p = ui.painter();
                        let fondo = if resp.hovered() { Color32::from_gray(40) } else { Color32::from_gray(28) };
                        p.rect(r, 10, lerp_col(fondo, Color32::from_gray(58), t), Stroke::new(1.0, lerp_col(LINE, Color32::from_gray(150), t)), StrokeKind::Inside);
                        let col = lerp_col(Color32::from_gray(120), Color32::WHITE, t);
                        icono_estilo(p, i, r.center() - Vec2::new(0.0, r.height() * 0.13), col);
                        p.text(r.center_bottom() - Vec2::new(0.0, r.height() * 0.13), Align2::CENTER_CENTER, n, FontId::proportional(11.5), col);
                        if resp.on_hover_text(d).clicked() && !sel {
                            self.snap();
                            self.master_estilo = i;
                            vivo = true;
                        }
                    }
                });
                ui.add_space(6.0);
            }
            ui.label(RichText::new(MASTER_ESTILOS[self.master_estilo].1).small().color(DIM));
            ui.add_space(8.0);
            // ---------- intensidad (barra con marcas)
            Frame::new().fill(PANEL2).corner_radius(10).inner_margin(Margin::same(12)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new("Intensidad").strong());
                ui.add_space(4.0);
                let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 24.0), Sense::click_and_drag());
                let x0 = r.left() + 6.0;
                let x1 = r.right() - 6.0;
                if resp.drag_started() || resp.clicked() {
                    self.snap();
                }
                if let Some(pp) = resp.interact_pointer_pos() {
                    if resp.dragged() || resp.clicked() {
                        // se engancha a las 11 marcas
                        let v = (((pp.x - x0) / (x1 - x0)).clamp(0.0, 1.0) * 10.0).round() / 5.0;
                        if v != self.master_int {
                            self.master_int = v;
                            vivo = true;
                        }
                    }
                }
                if resp.double_clicked() {
                    self.master_int = 1.0;
                    vivo = true;
                }
                let p = ui.painter();
                let y = r.center().y;
                p.hline(x0..=x1, y, Stroke::new(1.0, Color32::from_gray(70)));
                for k in 0..=10 {
                    let x = egui::lerp(x0..=x1, k as f32 / 10.0);
                    let h = if k % 5 == 0 { 6.0 } else { 3.5 };
                    p.vline(x, (y - h)..=(y + h), Stroke::new(1.0, Color32::from_gray(90)));
                }
                let xv = egui::lerp(x0..=x1, self.master_int / 2.0);
                p.rect_filled(Rect::from_center_size(Pos2::new(xv, y), Vec2::new(5.0, 16.0)), 2, Color32::WHITE);
                let (lufs, _) = int_valores(self.master_int);
                resp.on_hover_text(format!("Volumen final: {lufs:.1} LUFS"));
                ui.horizontal(|ui| {
                    let w = ui.available_width();
                    let (r, _) = ui.allocate_exact_size(Vec2::new(w, 14.0), Sense::hover());
                    let p = ui.painter();
                    for (k, n) in ["Ligero", "Normal", "Pesado"].iter().enumerate() {
                        let al = [Align2::LEFT_CENTER, Align2::CENTER_CENTER, Align2::RIGHT_CENTER][k];
                        let x = [r.left(), r.center().x, r.right()][k];
                        p.text(Pos2::new(x, r.center().y), al, *n, FontId::proportional(11.0), DIM);
                    }
                });
            });
            ui.add_space(8.0);
            // ---------- ecualizador
            Frame::new().fill(PANEL2).corner_radius(10).inner_margin(Margin::same(12)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Ecualizador").strong());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let mut on = self.master_eq_on;
                        if interruptor(ui, &mut on).changed() {
                            self.snap();
                            self.master_eq_on = on;
                            vivo = true;
                        }
                    });
                });
                ui.add_space(4.0);
                ui.add_enabled_ui(self.master_eq_on, |ui| {
                    ui.horizontal(|ui| {
                        let w = (ui.available_width() / 3.0 - 6.0).max(50.0);
                        for (k, n) in ["Graves", "Medios", "Agudos"].iter().enumerate() {
                            ui.vertical(|ui| {
                                ui.set_width(w);
                                ui.vertical_centered(|ui| {
                                    let antes = self.master_eq3[k];
                                    let r = knob(ui, &mut self.master_eq3[k], -6.0, 6.0, 0.0, n, |v| format!("{v:+.1} dB"));
                                    if r.drag_started() {
                                        self.snap();
                                    }
                                    if self.master_eq3[k] != antes {
                                        vivo = true;
                                    }
                                });
                            });
                        }
                    });
                });
            });
            ui.add_space(12.0);
            // ---------- aplicar
            if self.master_job.is_some() {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Analizando la canción…");
                });
            } else {
                let w = ui.available_width();
                if boton_metal(ui, "Aplicar y ajustar el volumen", Vec2::new(w, 38.0)).on_hover_text("Mide la canción y deja el volumen final listo para publicar").clicked() {
                    self.start_mastering();
                }
            }
            ui.add_space(4.0);
            ui.label(RichText::new("El estilo, la intensidad y el ecualizador se escuchan al instante. «Aplicar» además ajusta el volumen final (y copia la referencia, si elegiste una).").small().color(DIM));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.small_button("Quitar masterizado").clicked() {
                    self.snap();
                    self.p.master_eq = default_eq();
                    self.p.master_comp = CompP::master();
                    self.p.limiter = LimP::default();
                    self.cambios = true;
                    self.send();
                }
                if ui.small_button("Medidores (NN Master)…").clicked() {
                    self.open_win(Win::Master);
                }
            });
        });
        if vivo {
            if self.sin_master {
                self.poner_sin_master(false);
            }
            self.master_en_vivo();
        }
    }
}
