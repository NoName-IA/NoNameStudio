//! Pads: tocar la batería con pads (con repetición de nota al ritmo) y acordes de la escala.
//! Mientras suena el patrón, lo que tocas se puede grabar en él.
use super::*;

/// Estado de los pads.
pub(super) struct Pads {
    /// 0 = batería, 1 = acordes
    pub modo: u8,
    /// repetición de nota: 0 = no, si no pasos entre golpes (4 = 1/4, 2 = 1/8, 1 = 1/16, 0,5 = 1/32)
    pub repetir: f32,
    pub grabar: bool,
    /// acordes con séptima
    pub septima: bool,
    /// último golpe de cada pad (para repetir) y acorde sonando (canal, notas)
    pub ultimo: std::collections::HashMap<usize, f64>,
    pub sonando: Option<(u32, Vec<u8>, usize)>,
}

impl Default for Pads {
    fn default() -> Self {
        Pads { modo: 0, repetir: 0.0, grabar: false, septima: false, ultimo: Default::default(), sonando: None }
    }
}

const GRADOS: [&str; 7] = ["I", "II", "III", "IV", "V", "VI", "VII"];

impl StudioApp {
    pub(super) fn pads(&mut self, ui: &mut Ui) {
        // ---------- barra
        ui.horizontal_wrapped(|ui| {
            for (k, n) in [(0u8, "Batería"), (1, "Acordes")] {
                if pestana_metal(ui, self.pads.modo == k, n, 100.0).clicked() {
                    self.pads.modo = k;
                }
            }
            ui.add_space(12.0);
            if self.pads.modo == 0 {
                ui.label(RichText::new("Repetir").small().color(DIM));
                for (v, n) in [(0.0f32, "No"), (4.0, "1/4"), (2.0, "1/8"), (1.0, "1/16"), (0.5, "1/32")] {
                    if ui.add(egui::Button::selectable(self.pads.repetir == v, n)).on_hover_text("Mientras mantienes un pad apretado, se repite al ritmo").clicked() {
                        self.pads.repetir = v;
                    }
                }
            } else {
                ui.checkbox(&mut self.pads.septima, "Con séptima");
                ui.label(RichText::new(format!("Escala: {}", self.p.key_label())).small().color(DIM));
            }
            ui.add_space(12.0);
            ui.checkbox(&mut self.pads.grabar, "Grabar en el patrón").on_hover_text("Con el patrón sonando (PAT), lo que tocas queda escrito en él");
        });
        ui.add_space(8.0);
        let ahora = ui.input(|i| i.time);
        let paso_s = self.p.step_secs();
        let grabando = self.pads.grabar && self.playing() && !self.song_mode;
        let pos = if grabando { self.local_step() } else { None };
        if self.pads.modo == 0 {
            // ---------- batería: un pad por canal de golpes o samples
            let canales: Vec<(u32, String, [u8; 3], f32)> = self
                .p
                .channels
                .iter()
                .filter(|c| c.inst.uses_steps())
                .take(16)
                .map(|c| (c.id, c.name.clone(), PALETTE[(c.id as usize) % PALETTE.len()], if c.inst == Inst::Sampler { c.smp.root as f32 } else { 60.0 }))
                .collect();
            if canales.is_empty() {
                ui.label(RichText::new("Agrega canales de batería o samples en el Channel Rack para tocarlos aquí.").color(DIM));
                return;
            }
            let ancho = ui.available_width();
            let cols = 4usize;
            let lado = ((ancho - 30.0) / cols as f32).min((ui.available_height() - 20.0) / 4.0).clamp(60.0, 160.0);
            for fila in canales.chunks(cols) {
                ui.horizontal(|ui| {
                    for (k, (id, nombre, col, nota)) in fila.iter().enumerate() {
                        let idx = canales.iter().position(|c| c.0 == *id).unwrap_or(k);
                        let (r, resp) = ui.allocate_exact_size(Vec2::splat(lado), Sense::click_and_drag());
                        let apretado = resp.is_pointer_button_down_on();
                        // fuerza según la altura donde tocas (arriba = más fuerte)
                        let vel = resp.interact_pointer_pos().map(|p| (1.0 - (p.y - r.top()) / r.height()).clamp(0.25, 1.0)).unwrap_or(0.9);
                        let mut golpe = resp.drag_started() || resp.clicked() && !self.pads.ultimo.contains_key(&idx);
                        if apretado && self.pads.repetir > 0.0 {
                            let intervalo = paso_s * self.pads.repetir as f64;
                            if self.pads.ultimo.get(&idx).is_some_and(|t| ahora - t >= intervalo) {
                                golpe = true;
                            }
                            ui.ctx().request_repaint();
                        }
                        if !apretado && !resp.clicked() {
                            self.pads.ultimo.remove(&idx);
                        }
                        if golpe {
                            self.pads.ultimo.insert(idx, ahora);
                            self.preview(*id, *nota as u8, vel);
                            if let Some(pp) = pos {
                                let paso = pp.round() as usize;
                                if let Some(pat) = self.p.pattern_mut(self.sel_pat) {
                                    let len = pat.len_steps() as usize;
                                    let fila = pat.steps.entry(*id).or_insert_with(|| vec![0.0; len]);
                                    if fila.len() < len {
                                        fila.resize(len, 0.0);
                                    }
                                    fila[paso % len] = vel;
                                }
                                self.cambios = true;
                                self.send();
                            }
                        }
                        let c = Color32::from_rgb(col[0], col[1], col[2]);
                        let t = ui.ctx().animate_bool_with_time(resp.id.with("brillo"), apretado, 0.12);
                        let p = ui.painter();
                        p.rect_filled(r.shrink(3.0), 14, lerp_col(Color32::from_gray(28), lerp_col(c, Color32::WHITE, 0.2), 0.25 + 0.6 * t));
                        p.rect_stroke(r.shrink(3.0), 14, Stroke::new(1.5, lerp_col(c, Color32::WHITE, t)), StrokeKind::Inside);
                        p.text(r.center(), Align2::CENTER_CENTER, nombre, FontId::proportional(14.0), Color32::WHITE);
                        p.text(r.left_bottom() + Vec2::new(10.0, -10.0), Align2::LEFT_BOTTOM, format!("{}", idx + 1), FontId::proportional(11.0), Color32::from_gray(170));
                    }
                });
            }
        } else {
            // ---------- acordes de la escala (tocan en el canal melódico elegido)
            let melodicos = self.melodic();
            if melodicos.is_empty() {
                ui.label(RichText::new("Agrega un instrumento (Rhodes, NN Wave, bajo…) para tocar acordes.").color(DIM));
                return;
            }
            let ch = if melodicos.iter().any(|(id, _)| *id == self.sel_ch) { self.sel_ch } else { melodicos[0].0 };
            ui.horizontal(|ui| {
                ui.label(RichText::new("Suena en").small().color(DIM));
                let nombre = melodicos.iter().find(|(id, _)| *id == ch).map(|(_, n)| n.clone()).unwrap_or_default();
                egui::ComboBox::from_id_salt("pads_ch").selected_text(nombre).show_ui(ui, |ui| {
                    for (id, n) in &melodicos {
                        if ui.selectable_label(*id == ch, n).clicked() {
                            self.sel_ch = *id;
                        }
                    }
                });
            });
            ui.add_space(6.0);
            let lado = ((ui.available_width() - 40.0) / 7.0).clamp(60.0, 150.0);
            let alto = (lado * 1.3).min(ui.available_height() - 10.0).max(70.0);
            let mut soltar = false;
            ui.horizontal(|ui| {
                for g in 0..7u8 {
                    let (r, resp) = ui.allocate_exact_size(Vec2::new(lado, alto), Sense::click_and_drag());
                    let apretado = resp.is_pointer_button_down_on();
                    let notas = crate::templates::diatonic_chord(&self.p, g, 48, self.pads.septima);
                    let nombre_ac = nombre_acorde_notas(&notas);
                    if resp.drag_started() || (resp.clicked() && self.pads.sonando.as_ref().is_none_or(|s| s.2 != g as usize)) {
                        if let Some((c0, viejas, _)) = self.pads.sonando.take() {
                            for m in viejas {
                                self.cmd(Cmd::NoteOff { ch: c0, m });
                            }
                        }
                        for &m in &notas {
                            self.cmd(Cmd::NoteOn { ch, m, v: 0.8 });
                        }
                        if let Some(pp) = pos {
                            let paso = pp.round() as u32;
                            if let Some(pat) = self.p.pattern_mut(self.sel_pat) {
                                let len = pat.len_steps();
                                let lista = pat.notes.entry(ch).or_default();
                                for &m in &notas {
                                    lista.push(Note { s: paso % len, l: 4, m, v: 0.8, o: 0.0 });
                                }
                            }
                            self.cambios = true;
                            self.send();
                        }
                        self.pads.sonando = Some((ch, notas.clone(), g as usize));
                    }
                    if resp.drag_stopped() || (resp.clicked() && !apretado) {
                        soltar = true;
                    }
                    let t = ui.ctx().animate_bool_with_time(resp.id.with("brillo"), apretado, 0.12);
                    let p = ui.painter();
                    p.rect_filled(r.shrink(3.0), 14, lerp_col(Color32::from_gray(28), Color32::from_gray(200), 0.2 + 0.6 * t));
                    p.rect_stroke(r.shrink(3.0), 14, Stroke::new(1.5, lerp_col(Color32::from_gray(80), Color32::WHITE, t)), StrokeKind::Inside);
                    p.text(r.center() - Vec2::new(0.0, 10.0), Align2::CENTER_CENTER, GRADOS[g as usize], FontId::proportional(20.0), lerp_col(Color32::WHITE, Color32::from_gray(15), t));
                    p.text(r.center() + Vec2::new(0.0, 16.0), Align2::CENTER_CENTER, nombre_ac, FontId::proportional(12.0), lerp_col(Color32::from_gray(180), Color32::from_gray(30), t));
                }
            });
            if soltar || (!ui.input(|i| i.pointer.any_down()) && self.pads.sonando.is_some()) {
                if let Some((c0, viejas, _)) = self.pads.sonando.take() {
                    for m in viejas {
                        self.cmd(Cmd::NoteOff { ch: c0, m });
                    }
                }
            }
        }
    }
}

/// Nombre de un acorde a partir de sus notas MIDI (raíz = la más grave).
fn nombre_acorde_notas(notas: &[u8]) -> String {
    const N: [&str; 12] = ["Do", "Do#", "Re", "Mib", "Mi", "Fa", "Fa#", "Sol", "Lab", "La", "Sib", "Si"];
    let Some(&raiz) = notas.first() else { return String::new() };
    let int: Vec<i32> = notas.iter().map(|m| (*m as i32 - raiz as i32).rem_euclid(12)).collect();
    let tipo = if int.contains(&3) && int.contains(&6) {
        "dim"
    } else if int.contains(&3) {
        "m"
    } else if int.contains(&8) {
        "aum"
    } else {
        ""
    };
    let septima = if int.contains(&10) { "7" } else if int.contains(&11) { "maj7" } else { "" };
    format!("{}{}{}", N[(raiz % 12) as usize], tipo, septima)
}
