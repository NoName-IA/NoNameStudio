//! Vista en vivo de lo que se graba con el micrófono: forma de onda, nivel, tiempo y avisos.
use super::*;

/// Tramos de 10 ms con el pico de cada uno.
pub(super) const TRAMO_S: f32 = 0.01;

impl StudioApp {
    /// Velocidad de corrección de NN Tune según la intensidad elegida (1 = lo más intenso).
    pub(super) fn tune_speed(&self) -> f32 {
        (1.0 - self.settings.tune_int.clamp(0.0, 1.0)) * 0.9
    }
    pub(super) fn tune_tonica(&self) -> u8 {
        if self.settings.tune_tonica > 11 { self.p.key % 12 } else { self.settings.tune_tonica }
    }
    pub(super) fn tune_escala(&self) -> u8 {
        if self.settings.tune_escala as usize >= SCALES.len() { self.p.scale } else { self.settings.tune_escala }
    }
    pub(super) fn tune_key_label(&self) -> String {
        format!("{} {}", KEY_NAMES[self.tune_tonica() as usize], SCALES[(self.tune_escala() as usize).min(SCALES.len() - 1)].0.to_lowercase())
    }
    /// Parámetros de NN Tune según el panel del micrófono.
    pub(super) fn tune_params(&self) -> TuneP {
        TuneP { on: true, key: self.tune_tonica(), scale: 0, speed: self.tune_speed(), mix: 1.0, gain: 1.0, mask: scale_mask(self.tune_tonica(), self.tune_escala()), estilo: self.settings.tune_estilo, algoritmo: self.settings.tune_alg, mezcla: self.settings.tune_mezcla }
    }

    /// Deja la escucha del micrófono como dicen los ajustes: abre o cierra la entrada y pasa
    /// volumen y autotune al motor. Se llama al cambiar algo y al empezar o terminar de grabar.
    pub(super) fn escucha_sync(&mut self) {
        let grabando = self.rec.is_some() || self.rec_track.is_some();
        let tp = self.tune_params();
        let m = &self.status.monitor;
        m.gain.store(self.settings.vol_escucha.to_bits(), Ordering::Relaxed);
        m.pan.store(self.settings.pan_escucha.to_bits(), Ordering::Relaxed);
        m.reverb.store(self.settings.reverb_escucha.to_bits(), Ordering::Relaxed);
        m.tune_key.store(tp.key as u32, Ordering::Relaxed);
        m.tune_mask.store(tp.mask as u32, Ordering::Relaxed);
        m.tune_estilo.store(tp.estilo as u32, Ordering::Relaxed);
        m.tune_speed.store(tp.speed.to_bits(), Ordering::Relaxed);
        m.tune_on.store(self.settings.tune_vivo, Ordering::Relaxed);
        m.seguir.store(self.settings.tune_acordes, Ordering::Relaxed);
        m.tune_alg.store(self.settings.tune_alg as u32, Ordering::Relaxed);
        m.tune_mezcla.store(self.settings.tune_mezcla.to_bits(), Ordering::Relaxed);
        if !self.settings.escucharme {
            m.on.store(false, Ordering::Relaxed);
            self.mic_mon = None;
            return;
        }
        if grabando {
            // la grabación ya tiene el micrófono abierto y también alimenta la escucha
            self.mic_mon = None;
        } else if self.mic_mon.is_none() {
            match audio::record(self.status.clone(), &self.settings.entrada, false, self.settings.canal_entrada as usize) {
                Ok(r) => self.mic_mon = Some(r),
                Err(e) => {
                    self.settings.escucharme = false;
                    self.msg = format!("No se pudo abrir el micrófono: {e}");
                }
            }
        }
        self.status.monitor.on.store(self.settings.escucharme, Ordering::Relaxed);
    }

    /// Botón «Micrófono» de la barra de arriba: abre o cierra el panel de abajo.
    pub(super) fn micro_boton(&mut self, ui: &mut Ui) {
        let on = self.settings.escucharme;
        let txt = if on { "Micrófono: te escuchas" } else { "Micrófono" };
        let b = egui::Button::selectable(self.mic_panel || on, RichText::new(txt).size(12.5)).min_size(Vec2::new(0.0, 28.0));
        if ui.add(b).on_hover_text("Escúchate y usa el autotune en vivo").clicked() {
            self.mic_panel = !self.mic_panel;
        }
    }

    /// Panel de abajo: entrada, escucha, NN Tune en vivo (estilo, intensidad, escala) y volumen.
    pub(super) fn micro_panel(&mut self, ui: &mut Ui) {
        let antes = format!("{:?}{:?}", (self.settings.pan_escucha, self.settings.reverb_escucha, self.settings.tune_avanzado), (self.settings.escucharme, self.settings.vol_escucha, self.settings.tune_vivo, self.settings.tune_int, self.settings.tune_tonica, self.settings.tune_escala, self.settings.tune_estilo, &self.settings.entrada, self.settings.tune_acordes, self.settings.tune_alg, self.settings.tune_mezcla, self.settings.tune_categoria));
        // lo que se está cantando ahora
        let m = &self.status.monitor;
        let cantada = f32::from_bits(m.cantada.load(Ordering::Relaxed));
        let objetivo = m.objetivo.load(Ordering::Relaxed) as i32 - 1;
        let acorde = m.acorde.load(Ordering::Relaxed) as u16;
        let oyendo = self.settings.escucharme && m.on.load(Ordering::Relaxed);
        if oyendo {
            if self.trazo.len() >= 360 {
                self.trazo.pop_front();
            }
            self.trazo.push_back((cantada, if self.settings.tune_vivo { objetivo as f32 } else { 0.0 }));
            ui.ctx().request_repaint_after(Duration::from_millis(16));
        }
        ui.horizontal(|ui| {
            if icon_button(ui, Ic::Close, Vec2::new(26.0, 24.0), false, None).on_hover_text("Cerrar el panel").clicked() {
                self.mic_panel = false;
            }
            ui.label(RichText::new("Micrófono").strong().size(14.0));
            ui.label(RichText::new("· tu voz en vivo").color(DIM));
        });
        ui.add_space(6.0);
        let apilado = self.plat.mobile || ui.available_width() < 1000.0;
        ui.with_layout(if apilado { Layout::top_down(Align::Min) } else { Layout::left_to_right(Align::Min) }, |ui| {
            // ---------------- izquierda: entrada y escucha
            ui.vertical(|ui| {
                ui.set_width(230.0);
                ui.label(RichText::new("Entrada").strong());
                // micrófonos conectados (se vuelven a buscar cada 3 s: así aparecen los que se conectan)
                if self.entradas_t.is_none_or(|t| t.elapsed() > Duration::from_secs(3)) {
                    self.entradas = audio::inputs();
                    self.salidas = audio::outputs();
                    self.entradas_t = Some(Instant::now());
                }
                let entradas = self.entradas.clone();
                let actual = if self.settings.entrada.is_empty() { audio::PREDETERMINADO_ENTRADA.to_string() } else { self.settings.entrada.clone() };
                let mut nueva = self.settings.entrada.clone();
                egui::ComboBox::from_id_salt("mic_in").width(220.0).truncate().selected_text(actual).show_ui(ui, |ui| {
                    ui.selectable_value(&mut nueva, String::new(), audio::PREDETERMINADO_ENTRADA);
                    for n in &entradas {
                        ui.selectable_value(&mut nueva, n.clone(), n);
                    }
                });
                if nueva != self.settings.entrada {
                    self.settings.entrada = nueva;
                    self.mic_mon = None;
                }
                ui.add_space(4.0);
                let nombres = ["Todos los canales", "Canal 1", "Canal 2"];
                let mut canal = self.settings.canal_entrada.min(2);
                egui::ComboBox::from_id_salt("mic_canal").width(220.0).selected_text(nombres[canal as usize]).show_ui(ui, |ui| {
                    for (k, n) in nombres.iter().enumerate() {
                        ui.selectable_value(&mut canal, k as u8, *n);
                    }
                }).response.on_hover_text("Si tu interfaz tiene varias entradas, elige en cuál está conectado el micrófono");
                if canal != self.settings.canal_entrada {
                    self.settings.canal_entrada = canal;
                    self.mic_mon = None;
                    self.save_settings();
                    // vuelve a abrir la escucha con el canal nuevo (si no, se queda en silencio)
                    self.escucha_sync();
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Nivel").small().color(DIM));
                    let (r, _) = ui.allocate_exact_size(Vec2::new(180.0, 8.0), Sense::hover());
                    let p = ui.painter();
                    p.rect_filled(r, 4, Color32::from_gray(12));
                    let db = if self.in_meter > 0.0 { 20.0 * self.in_meter.log10() } else { -99.0 };
                    let f = ((db + 48.0) / 48.0).clamp(0.0, 1.0);
                    let col = if self.in_meter > 0.97 { RED } else if db > -9.0 { Color32::from_rgb(220, 200, 70) } else { GREEN };
                    p.rect_filled(Rect::from_min_size(r.min, Vec2::new(r.width() * f, r.height())), 4, col);
                });
                ui.add_space(10.0);
                // botón tipo pastilla con audífonos
                let on = self.settings.escucharme;
                let (r, resp) = ui.allocate_exact_size(Vec2::new(200.0, 36.0), Sense::click());
                let p = ui.painter();
                let fondo = if on { Color32::WHITE } else if resp.hovered() { Color32::from_gray(47) } else { PANEL2 };
                let tinta = if on { Color32::from_gray(17) } else { Color32::WHITE };
                p.rect_filled(r, 18, fondo);
                // audífonos dibujados
                let c = Pos2::new(r.left() + 26.0, r.center().y + 1.0);
                p.add(egui::Shape::line((0..=16).map(|i| {
                    let a = std::f32::consts::PI * (1.0 + i as f32 / 16.0);
                    c + Vec2::new(a.cos() * 8.0, a.sin() * 8.0)
                }).collect(), Stroke::new(1.8, tinta)));
                p.rect_filled(Rect::from_center_size(c + Vec2::new(-7.5, 3.0), Vec2::new(4.0, 7.0)), 1, tinta);
                p.rect_filled(Rect::from_center_size(c + Vec2::new(7.5, 3.0), Vec2::new(4.0, 7.0)), 1, tinta);
                p.text(Pos2::new(r.left() + 44.0, r.center().y), Align2::LEFT_CENTER, if on { "Escuchándote" } else { "Escucharme" }, FontId::proportional(14.0), tinta);
                if resp.on_hover_text("Oír tu voz en los audífonos").clicked() {
                    self.settings.escucharme = !on;
                }
                ui.label(RichText::new("Usa audífonos: con parlantes se produce un pitido fuerte.").small().color(DIM));
                ui.add_space(6.0);
                let mut niv = self.settings.nivelar_grabacion;
                if ui.checkbox(&mut niv, "Grabar con el volumen nivelado").on_hover_text("La toma queda tan fuerte y pareja como te escuchas (si lo apagas, se graba tal cual llega del micrófono)").changed() {
                    self.settings.nivelar_grabacion = niv;
                    self.save_settings();
                }
                // efectos de voz del canal elegido (se oyen en la canción y al exportar)
                if let Some(c) = self.p.channel(self.sel_ch).filter(|c| matches!(c.inst, Inst::Audio | Inst::Vocal)).cloned() {
                    ui.add_space(10.0);
                    ui.label(RichText::new(format!("Efectos de voz · {}", c.name)).small().color(DIM));
                    let mut i_sel = usize::MAX;
                    egui::ComboBox::from_id_salt("voz_fx").width(220.0).selected_text("Elegir efectos…").show_ui(ui, |ui| {
                        for (i, (n, ayuda, ..)) in VOZ_FX.iter().enumerate() {
                            if ui.selectable_label(false, *n).on_hover_text(*ayuda).clicked() {
                                i_sel = i;
                            }
                        }
                    });
                    if i_sel != usize::MAX {
                        self.snap();
                        if let Some(cm) = self.p.channel_mut(c.id) {
                            aplicar_voz_fx(cm, i_sel);
                        }
                        self.msg = format!("Efectos de voz en «{}»: {}", c.name, VOZ_FX[i_sel].0);
                        self.send();
                    }
                }
            });
            ui.add_space(12.0);
            // ---------------- centro: tarjeta de NN Tune
            let ancho = (ui.available_width() - 70.0).clamp(560.0, 860.0);
            Frame::new().fill(Color32::from_gray(27)).stroke(Stroke::new(1.0, LINE)).corner_radius(12).inner_margin(Margin::same(16)).show(ui, |ui| {
                ui.set_width(ancho);
                ui.vertical(|ui| {
                let activo = self.settings.tune_vivo;
                // fila de arriba: interruptor a la izquierda, título al centro, detectar y ajustes a la derecha
                let (fila, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 30.0), Sense::hover());
                {
                    let r = Rect::from_min_size(Pos2::new(fila.left(), fila.center().y - 11.0), Vec2::new(40.0, 22.0));
                    let resp = ui.interact(r, ui.id().with("nn_tune_sw"), Sense::click());
                    let t = ui.ctx().animate_bool_with_time(resp.id, activo, 0.18);
                    let p = ui.painter();
                    p.rect_filled(r, 11, lerp_col(Color32::from_gray(64), Color32::WHITE, t));
                    let cx = r.left() + 11.0 + (r.width() - 22.0) * t;
                    p.circle_filled(Pos2::new(cx, r.center().y), 7.0, lerp_col(Color32::WHITE, Color32::from_gray(17), t));
                    if resp.on_hover_text(if activo { "Apagar el autotune" } else { "Prender el autotune" }).clicked() {
                        self.settings.tune_vivo = !activo;
                    }
                    p.text(fila.center(), Align2::CENTER_CENTER, "NN Tune", FontId::proportional(17.0), Color32::WHITE);
                    // ajustes avanzados (ícono de controles)
                    let ra = Rect::from_min_size(Pos2::new(fila.right() - 34.0, fila.center().y - 15.0), Vec2::new(34.0, 30.0));
                    let ra_resp = ui.interact(ra, ui.id().with("nn_tune_adv"), Sense::click());
                    let p = ui.painter();
                    let fondo = if self.settings.tune_avanzado { Color32::WHITE } else if ra_resp.hovered() { Color32::from_gray(56) } else { Color32::from_gray(43) };
                    let tinta = if self.settings.tune_avanzado { Color32::from_gray(17) } else { Color32::WHITE };
                    p.rect_filled(ra, 15, fondo);
                    for (k, x) in [(-5.0f32, 3.0f32), (0.0, -3.0), (5.0, 2.0)] {
                        let y = ra.center().y + k;
                        p.line_segment([Pos2::new(ra.center().x - 8.0, y), Pos2::new(ra.center().x + 8.0, y)], Stroke::new(1.3, tinta));
                        p.circle_filled(Pos2::new(ra.center().x + x, y), 2.2, tinta);
                    }
                    if ra_resp.on_hover_text("Ajustes avanzados: algoritmo y mezcla de armonía").clicked() {
                        self.settings.tune_avanzado = !self.settings.tune_avanzado;
                    }
                    // detectar tonalidad
                    let rd = Rect::from_min_size(Pos2::new(ra.left() - 8.0 - 168.0, fila.center().y - 15.0), Vec2::new(168.0, 30.0));
                    let rd_resp = ui.interact(rd, ui.id().with("nn_tune_det"), Sense::click());
                    let p = ui.painter();
                    p.rect_filled(rd, 15, if rd_resp.hovered() { Color32::from_gray(56) } else { Color32::from_gray(43) });
                    p.text(rd.center(), Align2::CENTER_CENTER, "Detectar tonalidad", FontId::proportional(13.5), Color32::WHITE);
                    if rd_resp.on_hover_text("Busca la tonalidad de la canción a partir de sus notas").clicked() {
                        match detect_key(&self.p) {
                            Some((k, esc)) => {
                                self.settings.tune_tonica = k;
                                self.settings.tune_escala = esc;
                                self.msg = format!("Tonalidad detectada: {}", self.tune_key_label());
                            }
                            None => self.msg = "La canción todavía no tiene notas para detectar la tonalidad.".into(),
                        }
                    }
                }
                ui.add_space(8.0);
                let atenuar = |c: Color32| if activo { c } else { c.gamma_multiply(0.45) };
                ui.horizontal_top(|ui| {
                    // estilos
                    ui.vertical(|ui| {
                        ui.set_width(250.0);
                        ui.add_space(6.0);
                        ui.label(RichText::new("Categoría").size(13.5).color(atenuar(Color32::WHITE)));
                        let cat = (self.settings.tune_categoria as usize).min(crate::fx::CATEGORIAS.len() - 1);
                        egui::ComboBox::from_id_salt("tune_cat").width(160.0).selected_text(crate::fx::CATEGORIAS[cat]).show_ui(ui, |ui| {
                            for (k, n) in crate::fx::CATEGORIAS.iter().enumerate() {
                                ui.selectable_value(&mut self.settings.tune_categoria, k as u8, *n);
                            }
                        });
                        ui.add_space(8.0);
                        let lista: Vec<usize> = (0..crate::fx::ESTILOS.len()).filter(|i| crate::fx::ESTILOS[*i].categoria == self.settings.tune_categoria).collect();
                        let tam = Vec2::new(78.0, 46.0);
                        for fila in lista.chunks(3) {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 4.0;
                                for &i in fila {
                                    let sel = self.settings.tune_estilo as usize == i;
                                    let (r, resp) = ui.allocate_exact_size(tam, Sense::click());
                                    let p = ui.painter();
                                    let ts = ui.ctx().animate_bool_with_time(resp.id.with("sel"), sel, 0.2);
                                    let base = if resp.hovered() { Color32::from_gray(33) } else { Color32::from_gray(23) };
                                    let fondo = lerp_col(base, Color32::WHITE, ts);
                                    let tinta = atenuar(lerp_col(Color32::from_rgb(160, 166, 176), Color32::from_gray(17), ts));
                                    p.rect(r, 5, atenuar(fondo), Stroke::new(1.0, lerp_col(Color32::from_gray(43), Color32::WHITE, ts)), StrokeKind::Inside);
                                    icono_estilo(p, [0, 1, 2, 3, 4, 5, 4, 2, 1, 3, 5, 5, 2, 4, 3][i.min(14)], Pos2::new(r.center().x, r.top() + 16.0), tinta);
                                    p.text(Pos2::new(r.center().x, r.bottom() - 10.0), Align2::CENTER_CENTER, crate::fx::ESTILOS[i].nombre, FontId::proportional(11.0), tinta);
                                    let ayuda = crate::fx::ESTILOS[i].ayuda;
                                    if resp.on_hover_text(ayuda).clicked() {
                                        self.settings.tune_estilo = i as u8;
                                        self.settings.tune_vivo = true;
                                    }
                                }
                            });
                        }
                    });
                    // perilla de intensidad (grande, como en la foto)
                    ui.vertical(|ui| {
                        ui.set_width(210.0);
                        let (r, resp) = ui.allocate_exact_size(Vec2::new(210.0, 196.0), Sense::click_and_drag());
                        if resp.dragged() {
                            let d = resp.drag_delta();
                            self.settings.tune_int = (self.settings.tune_int + (d.x - d.y) / 220.0).clamp(0.0, 1.0);
                        }
                        if resp.double_clicked() {
                            self.settings.tune_int = 0.5;
                        }
                        let v = ui.ctx().animate_value_with_time(resp.id.with("v"), self.settings.tune_int, 0.12);
                        let p = ui.painter();
                        let c = Pos2::new(r.center().x, r.top() + 88.0);
                        let (a0, span) = (std::f32::consts::PI * 0.72, std::f32::consts::PI * 1.56);
                        let n = 44;
                        for k in 0..n {
                            let t = k as f32 / (n - 1) as f32;
                            let a = a0 + span * t;
                            let dir = Vec2::new(a.cos(), a.sin());
                            let on = t <= v + 1e-3;
                            // las rayitas crecen hacia el final, como en la foto
                            let largo = 4.0 + 14.0 * t;
                            let grosor = 2.0 + 2.2 * t;
                            let col = if on { lerp_col(Color32::from_gray(95), Color32::from_gray(245), t) } else { Color32::from_gray(41) };
                            p.line_segment([c + dir * 66.0, c + dir * (66.0 + largo)], Stroke::new(grosor, atenuar(col)));
                        }
                        // cuerpo de la perilla con un degradado suave
                        for k in 0..12 {
                            let t = k as f32 / 11.0;
                            p.circle_filled(c - Vec2::new(0.0, t * 6.0), 60.0 - t * 8.0, lerp_col(Color32::from_gray(17), Color32::from_gray(33), t));
                        }
                        p.circle_stroke(c, 60.0, Stroke::new(1.0, Color32::from_gray(48)));
                        let a = a0 + span * v;
                        let dir = Vec2::new(a.cos(), a.sin());
                        p.line_segment([c + dir * 34.0, c + dir * 50.0], Stroke::new(3.0, atenuar(Color32::WHITE)));
                        let et = if crate::fx::ESTILOS[(self.settings.tune_estilo as usize).min(crate::fx::ESTILOS.len() - 1)].vel == (0.0, 0.0) { "Robot (siempre al máximo)" } else if v < 0.25 { "Suave" } else if v < 0.55 { "Medio" } else if v < 0.85 { "Intenso" } else { "Lo más intenso" };
                        p.text(Pos2::new(c.x, r.bottom() - 12.0), Align2::CENTER_CENTER, et, FontId::proportional(14.0), atenuar(Color32::from_gray(221)));
                        resp.on_hover_text("Arrastra para cambiar la intensidad (qué tan rápido pasa de una nota a otra) · doble clic: medio");
                    });
                    // escala y teclado
                    ui.vertical(|ui| {
                        ui.add_space(6.0);
                        ui.label(RichText::new("Escala").size(13.5).color(atenuar(Color32::WHITE)));
                        let mut esc = self.tune_escala();
                        egui::ComboBox::from_id_salt("tune_esc").width(170.0).selected_text(SCALES[(esc as usize).min(SCALES.len() - 1)].0).show_ui(ui, |ui| {
                            for (i, (n, _)) in SCALES.iter().enumerate() {
                                ui.selectable_value(&mut esc, i as u8, *n);
                            }
                        });
                        if esc != self.tune_escala() {
                            self.settings.tune_escala = esc;
                        }
                        ui.add_space(22.0);
                        // teclado: negras arriba, blancas abajo
                        let tonica = self.tune_tonica();
                        let sigue = self.settings.tune_acordes && acorde.count_ones() >= 3;
                        let mask = if sigue { acorde } else { scale_mask(tonica, self.tune_escala()) };
                        let pc_cantada = if cantada > 0.0 { Some((cantada.round() as i32).rem_euclid(12) as u8) } else { None };
                        let pc_obj = if objetivo >= 0 { Some((objetivo.rem_euclid(12)) as u8) } else { None };
                        let tecla = |ui: &mut Ui, nota: u8, txt: &str| -> bool {
                            let (r, resp) = ui.allocate_exact_size(Vec2::new(30.0, 30.0), Sense::click());
                            // transiciones suaves: al cambiar de acorde las teclas se encienden y apagan despacio
                            let ctx = ui.ctx().clone();
                            let anim = |k: &str, v: bool, t: f32| ctx.animate_bool_with_time(egui::Id::new(("nn_tecla", k, nota)), v, t);
                            let es_t = nota == tonica && !sigue;
                            let en = (mask >> nota) & 1 == 1;
                            let a_obj = anim("obj", pc_obj == Some(nota), 0.35);
                            let a_cant = anim("cant", pc_cantada == Some(nota), 0.25);
                            let a_ton = anim("ton", es_t, 0.6);
                            let a_en = anim("en", en, 0.6);
                            let a_ac = anim("ac", en && sigue, 0.6);
                            let p = ui.painter();
                            if a_obj > 0.01 {
                                p.rect_filled(r.expand(3.0), 7, GREEN.gamma_multiply(0.35 * a_obj));
                            }
                            if a_cant > 0.01 {
                                p.rect_stroke(r.expand(2.0), 7, Stroke::new(2.0, Color32::WHITE.gamma_multiply(a_cant)), StrokeKind::Outside);
                            }
                            let mut fondo = Color32::from_gray(19);
                            fondo = lerp_col(fondo, lerp_col(Color32::from_gray(19), ACCENT, 0.6), a_ac);
                            fondo = lerp_col(fondo, Color32::WHITE, a_ton);
                            fondo = lerp_col(fondo, GREEN, a_obj);
                            let borde = lerp_col(Color32::from_gray(39), Color32::WHITE, a_en.max(a_ton));
                            p.rect(r, 4, atenuar(fondo), Stroke::new(1.2, atenuar(borde)), StrokeKind::Inside);
                            let letra = atenuar(lerp_col(lerp_col(Color32::from_rgb(120, 126, 136), Color32::WHITE, a_en), Color32::from_gray(17), a_ton.max(a_obj)));
                            let lineas: Vec<&str> = txt.split('\n').collect();
                            if lineas.len() == 2 {
                                p.text(r.center() - Vec2::new(0.0, 6.0), Align2::CENTER_CENTER, lineas[0], FontId::proportional(10.0), letra);
                                p.text(r.center() + Vec2::new(0.0, 6.0), Align2::CENTER_CENTER, lineas[1], FontId::proportional(10.0), letra);
                            } else {
                                p.text(r.center(), Align2::CENTER_CENTER, txt, FontId::proportional(11.5), letra);
                            }
                            resp.on_hover_text(format!("Tónica: {}", KEY_NAMES[nota as usize])).clicked()
                        };
                        let mut nueva = None;
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            ui.add_space(17.0);
                            for (n, t) in [(1u8, "Do#\nReb"), (3, "Re#\nMib")] {
                                if tecla(ui, n, t) {
                                    nueva = Some(n);
                                }
                            }
                            ui.add_space(34.0);
                            for (n, t) in [(6u8, "Fa#\nSolb"), (8, "Sol#\nLab"), (10, "La#\nSib")] {
                                if tecla(ui, n, t) {
                                    nueva = Some(n);
                                }
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            for (n, t) in [(0u8, "Do"), (2, "Re"), (4, "Mi"), (5, "Fa"), (7, "Sol"), (9, "La"), (11, "Si")] {
                                if tecla(ui, n, t) {
                                    nueva = Some(n);
                                }
                            }
                        });
                        if let Some(n) = nueva {
                            self.settings.tune_tonica = n;
                        }
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(self.tune_key_label()).small().color(atenuar(ACCENT2)));
                        });
                    });
                });
                ui.add_space(8.0);
                // ajustes avanzados: algoritmo y volumen de la armonía (botón de controles, arriba a la derecha)
                if self.settings.tune_avanzado {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Algoritmo").small().color(DIM));
                    let algs = [("Limpio", "Sonido claro y moderno que conserva el timbre de tu voz (unos 35 ms de demora)"), ("Baja demora", "Responde al instante: mejor para cantar escuchándote; un poco más metálico")];
                    for (k, (n, ayuda)) in algs.iter().enumerate() {
                        if ui.add(egui::Button::selectable(self.settings.tune_alg as usize == k, *n)).on_hover_text(*ayuda).clicked() {
                            self.settings.tune_alg = k as u8;
                        }
                    }
                    ui.add_space(18.0);
                    let hay_armonia = crate::fx::ESTILOS[(self.settings.tune_estilo as usize).min(crate::fx::ESTILOS.len() - 1)].voces.iter().any(|v| v.is_some());
                    ui.add_enabled_ui(hay_armonia, |ui| {
                        ui.label(RichText::new("Mezcla de armonía").small().color(DIM));
                        ui.spacing_mut().slider_width = 140.0;
                        ui.add(egui::Slider::new(&mut self.settings.tune_mezcla, 0.0..=1.5).custom_formatter(|v, _| format!("{:.0} %", v * 100.0)));
                    });
                });
                }
                ui.add_space(8.0);
                self.fila_vivo(ui, cantada, objetivo, acorde, oyendo);
                if apilado {
                    // en pantallas chicas el volumen va dentro de la tarjeta
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Volumen").strong());
                        ui.spacing_mut().slider_width = 200.0;
                        ui.add(egui::Slider::new(&mut self.settings.vol_escucha, 0.0..=2.0).custom_formatter(|v, _| format!("{:.0} %", v * 100.0)));
                        ui.add_space(12.0);
                        perilla(ui, &mut self.settings.pan_escucha, -1.0, 1.0, 0.0, "Paneo", "Mueve tu voz a la izquierda o a la derecha · doble toque: al centro");
                        perilla(ui, &mut self.settings.reverb_escucha, 0.0, 1.0, 0.0, "Reverb", "Reverberación: tu voz como en una sala · doble toque: sin reverberación");
                    });
                }
                });
            });
            ui.add_space(12.0);
            // ---------------- derecha: volumen de la voz (en pantallas chicas va dentro de la tarjeta)
            if !apilado {
            ui.vertical(|ui| {
                ui.set_width(130.0);
                ui.add_space(10.0);
                // volumen: riel con marcas y perilla blanca
                let (r, resp) = ui.allocate_exact_size(Vec2::new(130.0, 160.0), Sense::click_and_drag());
                let (y0, y1) = (r.top() + 8.0, r.bottom() - 8.0);
                let maxv = 2.0f32;
                if resp.dragged() || resp.clicked() {
                    if let Some(pp) = resp.interact_pointer_pos() {
                        self.settings.vol_escucha = ((y1 - pp.y) / (y1 - y0) * maxv).clamp(0.0, maxv);
                    }
                }
                if resp.double_clicked() {
                    self.settings.vol_escucha = 0.5;
                }
                let p = ui.painter();
                let cx = r.center().x;
                for k in 0..=10 {
                    let y = y0 + (y1 - y0) * k as f32 / 10.0;
                    p.line_segment([Pos2::new(cx - 20.0, y), Pos2::new(cx - 9.0, y)], Stroke::new(1.0, Color32::from_gray(56)));
                    p.line_segment([Pos2::new(cx + 9.0, y), Pos2::new(cx + 20.0, y)], Stroke::new(1.0, Color32::from_gray(56)));
                }
                p.line_segment([Pos2::new(cx, y0), Pos2::new(cx, y1)], Stroke::new(2.0, Color32::from_gray(48)));
                let yv = y1 - (self.settings.vol_escucha / maxv) * (y1 - y0);
                p.line_segment([Pos2::new(cx, yv), Pos2::new(cx, y1)], Stroke::new(2.5, ACCENT));
                p.circle_filled(Pos2::new(cx, yv), 7.0, Color32::WHITE);
                resp.on_hover_text(format!("Volumen de tu voz: {:.0} % · doble clic: 50 %", self.settings.vol_escucha * 100.0));
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("Volumen").size(13.0).strong().color(Color32::WHITE));
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    perilla(ui, &mut self.settings.pan_escucha, -1.0, 1.0, 0.0, "Paneo", "Mueve tu voz a la izquierda o a la derecha · doble clic: al centro");
                    perilla(ui, &mut self.settings.reverb_escucha, 0.0, 1.0, 0.0, "Reverb", "Reverberación: tu voz como en una sala · doble clic: sin reverberación");
                });
            });
            }
        });
        ui.add_space(4.0);
        ui.label(RichText::new("Lo que grabas se guarda con tu voz natural; si NN Tune está prendido, al terminar se crea además la toma con este efecto (la original queda en «Tomas»).").small().color(DIM));
        let despues = format!("{:?}{:?}", (self.settings.pan_escucha, self.settings.reverb_escucha, self.settings.tune_avanzado), (self.settings.escucharme, self.settings.vol_escucha, self.settings.tune_vivo, self.settings.tune_int, self.settings.tune_tonica, self.settings.tune_escala, self.settings.tune_estilo, &self.settings.entrada, self.settings.tune_acordes, self.settings.tune_alg, self.settings.tune_mezcla, self.settings.tune_categoria));
        if antes != despues {
            self.escucha_sync();
            self.save_settings();
        }
    }

    /// Fila de la tarjeta de NN Tune: nota en vivo, afinador, gráfico de la voz y acordes.
    fn fila_vivo(&mut self, ui: &mut Ui, cantada: f32, objetivo: i32, acorde: u16, oyendo: bool) {
        let nombre = |m: i32| format!("{}{}", KEY_NAMES[m.rem_euclid(12) as usize], m.div_euclid(12) - 1);
        ui.horizontal_top(|ui| {
            // nota y afinador
            ui.vertical(|ui| {
                ui.set_width(200.0);
                if !oyendo {
                    ui.label(RichText::new("Prende «Escucharme» para ver tu voz en vivo.").small().color(DIM));
                } else if cantada > 0.0 {
                    let cerca = cantada.round() as i32;
                    let cents = ((cantada - cerca as f32) * 100.0).round();
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(nombre(cerca)).strong().size(24.0).color(Color32::WHITE));
                        ui.label(RichText::new(format!("{:+.0} ¢", cents)).size(13.0).color(if cents.abs() < 12.0 { GREEN } else { Color32::from_rgb(240, 200, 90) }));
                        if objetivo >= 0 && self.settings.tune_vivo {
                            ui.label(RichText::new(format!("> {}", nombre(objetivo))).size(15.0).color(GREEN));
                        }
                    });
                    // afinador: -50 … +50 cents
                    let (r, _) = ui.allocate_exact_size(Vec2::new(190.0, 10.0), Sense::hover());
                    let p = ui.painter();
                    p.rect_filled(r, 5, Color32::from_gray(16));
                    p.rect_filled(Rect::from_center_size(r.center(), Vec2::new(r.width() * 0.24, r.height())), 5, GREEN.gamma_multiply(0.25));
                    let x = r.center().x + (cents / 50.0).clamp(-1.0, 1.0) * r.width() * 0.5;
                    p.circle_filled(Pos2::new(x, r.center().y), 5.0, if cents.abs() < 12.0 { GREEN } else { Color32::WHITE });
                } else {
                    ui.label(RichText::new("—").size(24.0).color(DIM));
                    ui.label(RichText::new("Canta para ver la nota").small().color(DIM));
                }
            });
            // gráfico de los últimos segundos: voz (blanco) y corrección (verde)
            let (r, _) = ui.allocate_exact_size(Vec2::new((ui.available_width() - 210.0).max(160.0), 64.0), Sense::hover());
            let p = ui.painter();
            p.rect_filled(r, 8, Color32::from_gray(16));
            let vals: Vec<f32> = self.trazo.iter().flat_map(|t| [t.0, t.1]).filter(|v| *v > 0.0).collect();
            if !vals.is_empty() {
                let lo = vals.iter().fold(f32::MAX, |m, v| m.min(*v)).floor() - 2.0;
                let hi = vals.iter().fold(f32::MIN, |m, v| m.max(*v)).ceil() + 2.0;
                let y = |v: f32| r.bottom() - 4.0 - (v - lo) / (hi - lo).max(1.0) * (r.height() - 8.0);
                let n = self.trazo.len().max(2);
                let x = |i: usize| r.left() + i as f32 / (n - 1) as f32 * r.width();
                for k in [1usize, 0] {
                    let col = if k == 0 { Color32::WHITE } else { GREEN };
                    let mut tramo: Vec<Pos2> = vec![];
                    for (i, t) in self.trazo.iter().enumerate() {
                        let v = if k == 0 { t.0 } else { t.1 };
                        if v > 0.0 {
                            tramo.push(Pos2::new(x(i), y(v)));
                        } else if tramo.len() > 1 {
                            p.add(egui::Shape::line(std::mem::take(&mut tramo), Stroke::new(if k == 0 { 1.5 } else { 2.5 }, col)));
                        } else {
                            tramo.clear();
                        }
                    }
                    if tramo.len() > 1 {
                        p.add(egui::Shape::line(tramo, Stroke::new(if k == 0 { 1.5 } else { 2.5 }, col)));
                    }
                }
            } else {
                p.text(r.center(), Align2::CENTER_CENTER, "Aquí se dibuja tu voz (blanco) y la nota corregida (verde)", FontId::proportional(11.0), DIM);
            }
            // acordes
            ui.vertical(|ui| {
                ui.label(RichText::new("Acorde que suena").small().color(DIM));
                let ahora = ui.input(|i| i.time);
                let id = egui::Id::new("nn_acorde_cambio");
                let (antes, desde) = ui.data(|d| d.get_temp::<(u16, f64)>(id)).unwrap_or((acorde, 0.0));
                let desde = if antes != acorde { ahora } else { desde };
                ui.data_mut(|d| d.insert_temp(id, (acorde, desde)));
                let t = (((ahora - desde) / 0.6) as f32).clamp(0.0, 1.0);
                let t = t * t * (3.0 - 2.0 * t);
                if t < 1.0 {
                    ui.ctx().request_repaint();
                }
                let col = if acorde.count_ones() >= 3 { ACCENT2 } else { DIM };
                let (r, _) = ui.allocate_exact_size(Vec2::new(120.0, 22.0), Sense::hover());
                ui.painter().text(Pos2::new(r.left(), r.center().y + (1.0 - t) * 5.0), Align2::LEFT_CENTER, nombre_acorde(acorde), FontId::proportional(15.0), lerp_col(PANEL, col, t));
                let mut seguir = self.settings.tune_acordes;
                if ui.checkbox(&mut seguir, "Seguir los acordes").on_hover_text("La voz cae en las notas del acorde que suena en cada momento (necesita acordes en el teclado de notas)").changed() {
                    self.settings.tune_acordes = seguir;
                    self.settings.tune_vivo = true;
                }
            });
        });
    }

    /// Lee lo nuevo que llegó del micrófono y lo resume en picos para dibujarlo.
    pub(super) fn rec_update(&mut self) {
        let fuente = self.rec_track.as_ref().map(|t| (t.r.buf.clone(), t.r.sr)).or_else(|| self.rec.as_ref().map(|t| (t.r.buf.clone(), t.r.sr)));
        let Some((buf, sr)) = fuente else {
            self.rec_peaks.clear();
            self.rec_leido = 0;
            return;
        };
        let tramo = ((sr as f32 * TRAMO_S) as usize).max(1);
        let Ok(b) = buf.lock() else { return };
        if b.len() < self.rec_leido {
            self.rec_leido = 0;
            self.rec_peaks.clear();
        }
        while self.rec_leido + tramo <= b.len() {
            let pk = b[self.rec_leido..self.rec_leido + tramo].iter().fold(0f32, |m, x| m.max(x.abs()));
            self.rec_peaks.push(pk);
            self.rec_leido += tramo;
        }
    }

    /// Segundos grabados hasta ahora.
    pub(super) fn rec_secs(&self) -> f32 {
        self.rec_peaks.len() as f32 * TRAMO_S
    }

    /// Indicador de la barra superior: punto rojo que late, tiempo, nivel y avisos.
    pub(super) fn rec_indicator(&mut self, ui: &mut Ui) {
        if self.rec_track.is_none() && self.rec.is_none() {
            return;
        }
        let t = ui.input(|i| i.time) as f32;
        Frame::new().fill(Color32::from_gray(24)).stroke(Stroke::new(1.0, RED)).corner_radius(5).inner_margin(Margin::symmetric(6, 3)).show(ui, |ui| {
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());
                let a = 0.5 + 0.5 * (t * 5.0).sin();
                ui.painter().circle_filled(r.center(), 5.0, RED.gamma_multiply(0.4 + 0.6 * a));
                let s = self.rec_secs() as u32;
                ui.label(RichText::new(format!("REC {}:{:02}", s / 60, s % 60)).strong().monospace().size(12.0).color(Color32::WHITE)).on_hover_text("Grabando desde el micrófono");
                // nivel de entrada del micrófono
                let (r, _) = ui.allocate_exact_size(Vec2::new(46.0, 10.0), Sense::hover());
                let p = ui.painter();
                p.rect_filled(r, 2, Color32::from_gray(11));
                let db = if self.in_meter > 0.0 { 20.0 * self.in_meter.log10() } else { -99.0 };
                let f = ((db + 48.0) / 48.0).clamp(0.0, 1.0);
                let col = if self.in_meter > 0.97 { RED } else if db > -9.0 { Color32::from_rgb(220, 200, 70) } else { GREEN };
                p.rect_filled(Rect::from_min_size(r.min, Vec2::new(r.width() * f, r.height())), 2, col);
                // avisos
                let ultimos = self.rec_peaks.iter().rev().take(200).fold(0f32, |m, x| m.max(*x));
                if self.rec_secs() > 2.0 && ultimos < 0.003 {
                    ui.label(RichText::new("Sin sonido").small().strong().color(Color32::from_rgb(240, 190, 80)))
                        .on_hover_text("Revisa que el micrófono esté conectado, con volumen y elegido como entrada en Windows.");
                } else if self.in_meter > 0.97 {
                    ui.label(RichText::new("¡Muy fuerte!").small().strong().color(RED)).on_hover_text("La señal se distorsiona: aléjate del micrófono o bájale el volumen.");
                }
            });
        });
    }

    /// Dibuja la forma de onda grabada dentro de `r`. `escala` = píxeles por segundo; `desde` = segundo inicial.
    pub(super) fn rec_draw(&self, painter: &egui::Painter, r: Rect, px_por_s: f32, desde: f32, hasta: f32) {
        let mid = r.center().y + 5.0;
        let h = (r.height() - 16.0) / 2.0;
        let n = self.rec_peaks.len();
        let mut x = r.left();
        while x < r.right() {
            let s0 = desde + (x - r.left()) / px_por_s;
            let s1 = desde + (x + 1.0 - r.left()) / px_por_s;
            if s0 >= hasta {
                break;
            }
            let (i0, i1) = ((s0 / TRAMO_S) as usize, ((s1 / TRAMO_S) as usize).max((s0 / TRAMO_S) as usize + 1));
            if i0 >= n {
                break;
            }
            let v = self.rec_peaks[i0..i1.min(n)].iter().fold(0f32, |m, v| m.max(*v)).min(1.0);
            // en pantalla se realzan los niveles bajos para que se note que entra sonido
            let alto = v.sqrt();
            let col = if v > 0.97 { Color32::from_rgb(255, 120, 110) } else { Color32::from_rgb(255, 225, 225) };
            painter.vline(x, (mid - alto * h).min(mid - 0.5)..=(mid + alto * h).max(mid + 0.5), Stroke::new(1.0, col));
            x += 1.0;
        }
    }
}

/// Íconos de los estilos de NN Tune.
fn icono_estilo(p: &egui::Painter, i: usize, c: Pos2, col: Color32) {
    let st = Stroke::new(1.6, col);
    match i {
        0 => {
            // nota que se endereza
            p.add(egui::Shape::line(vec![c + Vec2::new(-12.0, 3.0), c + Vec2::new(-6.0, -4.0), c + Vec2::new(-1.0, 2.0), c + Vec2::new(12.0, 2.0)], st));
        }
        1 => {
            p.add(egui::Shape::line((0..=24).map(|k| {
                let x = -12.0 + k as f32;
                c + Vec2::new(x, (x * 0.5).sin() * 4.0)
            }).collect(), st));
        }
        2 => {
            p.add(egui::Shape::line(vec![c + Vec2::new(-12.0, 4.0), c + Vec2::new(-12.0, -4.0), c + Vec2::new(-4.0, -4.0), c + Vec2::new(-4.0, 4.0), c + Vec2::new(4.0, 4.0), c + Vec2::new(4.0, -4.0), c + Vec2::new(12.0, -4.0)], st));
        }
        3 => {
            p.circle_stroke(c + Vec2::new(-4.0, 0.0), 6.0, st);
            p.circle_stroke(c + Vec2::new(4.0, 0.0), 6.0, st);
        }
        4 => {
            for (k, h) in [(-8.0f32, 4.0f32), (0.0, 8.0), (8.0, 12.0)] {
                p.line_segment([c + Vec2::new(k, 6.0), c + Vec2::new(k, 6.0 - h)], Stroke::new(2.2, col));
            }
        }
        _ => {
            p.line_segment([c + Vec2::new(0.0, 7.0), c + Vec2::new(0.0, -6.0)], st);
            p.add(egui::Shape::line(vec![c + Vec2::new(-6.0, 0.0), c + Vec2::new(0.0, -7.0), c + Vec2::new(6.0, 0.0)], st));
        }
    }
}

/// Perilla pequeña blanca (paneo, reverberación) con su nombre debajo.
fn perilla(ui: &mut Ui, v: &mut f32, lo: f32, hi: f32, def: f32, nombre: &str, ayuda: &str) {
    ui.vertical(|ui| {
        ui.set_width(58.0);
        let (r, resp) = ui.allocate_exact_size(Vec2::new(58.0, 34.0), Sense::click_and_drag());
        if resp.dragged() {
            let d = resp.drag_delta();
            *v = (*v + (d.x - d.y) / 150.0 * (hi - lo)).clamp(lo, hi);
        }
        if resp.double_clicked() {
            *v = def;
        }
        let p = ui.painter();
        let c = r.center();
        let t = (*v - lo) / (hi - lo);
        let (a0, span) = (std::f32::consts::PI * 0.75, std::f32::consts::PI * 1.5);
        p.circle_filled(c, 13.0, Color32::WHITE);
        let a = a0 + span * t;
        let dir = Vec2::new(a.cos(), a.sin());
        p.line_segment([c + dir * 4.0, c + dir * 11.0], Stroke::new(2.2, Color32::from_gray(17)));
        if lo < 0.0 {
            p.text(Pos2::new(c.x - 19.0, c.y + 10.0), Align2::CENTER_CENTER, "I", FontId::proportional(9.0), DIM);
            p.text(Pos2::new(c.x + 19.0, c.y + 10.0), Align2::CENTER_CENTER, "D", FontId::proportional(9.0), DIM);
        }
        resp.on_hover_text(ayuda);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new(nombre).size(11.5).color(Color32::from_gray(207)));
        });
    });
}

/// Nombre de un acorde a partir de sus notas (máscara de 12 bits).
fn nombre_acorde(m: u16) -> String {
    if m.count_ones() < 3 {
        return "ninguno".into();
    }
    let tiene = |r: usize, d: usize| (m >> ((r + d) % 12)) & 1 == 1;
    let tipos: [(&str, &[usize]); 8] = [("7", &[4, 7, 10]), ("maj7", &[4, 7, 11]), ("m7", &[3, 7, 10]), ("mayor", &[4, 7]), ("menor", &[3, 7]), ("disminuido", &[3, 6]), ("aumentado", &[4, 8]), ("sus4", &[5, 7])];
    for (n, ints) in tipos {
        for r in 0..12 {
            if tiene(r, 0) && ints.iter().all(|d| tiene(r, *d)) {
                return format!("{} {}", KEY_NAMES[r], n);
            }
        }
    }
    (0..12).filter(|i| (m >> i) & 1 == 1).map(|i| KEY_NAMES[i]).collect::<Vec<_>>().join(" · ")
}
