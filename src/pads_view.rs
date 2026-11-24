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
    /// cuándo empezó el acorde que suena y si ya arrancó al apretar (así el clic al soltar no lo repite)
    pub desde: f64,
    pub por_toque: bool,
    /// kit elegido y cómo estaba la batería antes de elegir uno (para «Reiniciar»)
    pub kit: Option<usize>,
    pub antes_kit: Option<AntesKit>,
    /// dedos apoyados en la pantalla (id del toque → posición): varios pads a la vez
    pub dedos: std::collections::HashMap<u64, Pos2>,
    /// la pantalla es táctil (llegaron toques): se ignora el puntero que imita al primer dedo
    pub tactil: bool,
    /// último cuadro en que se vieron los pads (si saliste de Pads, los dedos guardados ya no valen)
    pub cuadro: u64,
    /// acordes sonando: pad → (canal, notas, desde cuándo)
    pub acordes: std::collections::HashMap<usize, (u32, Vec<u8>, f64)>,
}



/// Batería guardada antes de elegir un kit: canales de golpes y lo que tenían escrito en cada patrón.
pub(super) struct AntesKit {
    canales: Vec<Channel>,
    pasos: Vec<(u32, std::collections::HashMap<u32, Vec<f32>>, std::collections::HashMap<u32, Vec<Note>>)>,
}

impl Default for Pads {
    fn default() -> Self {
        Pads { modo: 0, repetir: 0.0, grabar: false, septima: false, ultimo: Default::default(), sonando: None, desde: 0.0, por_toque: false, kit: None, antes_kit: None, dedos: Default::default(), tactil: false, cuadro: 0, acordes: Default::default() }
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
                // kit: los pads quedan solo con los sonidos de ese estilo
                ui.label(RichText::new("Kit").small().color(DIM));
                let actual = self.pads.kit.map(|k| crate::templates::KITS[k]).unwrap_or("Elegir…");
                egui::ComboBox::from_id_salt("pads_kit").selected_text(actual).show_ui(ui, |ui| {
                    for (i, n) in crate::templates::KITS.iter().enumerate() {
                        if ui.selectable_label(self.pads.kit == Some(i), *n).clicked() {
                            self.elegir_kit(i);
                        }
                    }
                });
                if ui.add_enabled(self.pads.antes_kit.is_some(), egui::Button::new("Reiniciar")).on_hover_text("Vuelve los pads a como estaban antes de elegir un kit").clicked() {
                    self.reiniciar_pads();
                }
                ui.add_space(8.0);
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
            if self.pads.modo == 0 && !self.plat.mobile {
                ui.add_space(8.0);
                ui.label(RichText::new("Cada pad muestra su tecla").small().color(DIM))
                    .on_hover_text("Las teclas de los pads se cambian en Ajustes > Teclas. Los números sirven arriba de las letras o en el teclado numérico (con Bloq Num).");
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
                ui.label(RichText::new("Elige un kit arriba, o toca «+» para cargar un audio tuyo.").color(DIM));
            }
            // cuadrícula de pads cuadrados (4 columnas), centrada; el «+» al final si queda lugar
            let mut celdas: Vec<Option<(u32, String, [u8; 3], f32)>> = canales.iter().cloned().map(Some).collect();
            if celdas.len() < 16 {
                celdas.push(None);
            }
            let cols = 4usize;
            let filas = celdas.len().div_ceil(cols).max(1);
            let hueco = 8.0;
            let ancho = ui.available_width();
            let por_ancho = (ancho - hueco * (cols as f32 - 1.0)) / cols as f32;
            let por_alto = (ui.available_height() - 6.0 - hueco * (filas as f32 - 1.0)) / filas as f32;
            let l = por_ancho.min(por_alto).clamp(48.0, 200.0);
            let lado = Vec2::splat(l);
            let margen = ((ancho - (l * cols as f32 + hueco * (cols as f32 - 1.0))) / 2.0).max(0.0);
            ui.spacing_mut().item_spacing = Vec2::splat(hueco);

            // dedos: cada toque se sigue por separado (varios pads a la vez en el teléfono)
            let mut nuevos: Vec<Pos2> = vec![];
            let cuadro = ui.ctx().cumulative_pass_nr();
            if cuadro > self.pads.cuadro + 1 {
                self.pads.dedos.clear();
            }
            self.pads.cuadro = cuadro;
            ui.input(|i| {
                for e in &i.events {
                    if let egui::Event::Touch { id, phase, pos, .. } = e {
                        self.pads.tactil = true;
                        match phase {
                            egui::TouchPhase::Start => {
                                self.pads.dedos.insert(id.0, *pos);
                                nuevos.push(*pos);
                            }
                            egui::TouchPhase::Move => {
                                self.pads.dedos.insert(id.0, *pos);
                            }
                            egui::TouchPhase::End | egui::TouchPhase::Cancel => {
                                self.pads.dedos.remove(&id.0);
                            }
                        }
                    }
                }
            });
            if !self.pads.dedos.is_empty() {
                ui.ctx().request_repaint();
            }
            // teclas de la PC (no mientras se escribe en un campo)
            let teclado = !self.plat.mobile && self.tecla_captura.is_none() && !ui.ctx().egui_wants_keyboard_input();
            for (n_fila, fila) in celdas.chunks(cols).enumerate() {
                ui.horizontal(|ui| {
                    ui.add_space(margen);
                    for (k, celda) in fila.iter().enumerate() {
                        let Some((id, nombre, col, nota)) = celda else {
                            self.pad_mas(ui, lado);
                            continue;
                        };
                        let idx = n_fila * cols + k;
                        let (r, resp) = ui.allocate_exact_size(lado, Sense::click_and_drag());
                        // fuerza según la altura donde tocas (arriba = más fuerte)
                        let fuerza = |p: Pos2| (1.0 - (p.y - r.top()) / r.height()).clamp(0.25, 1.0);
                        // tecla del pad (se cambia en Ajustes > Teclas)
                        let atajo = teclas::PADS_IDS.get(idx).and_then(|id| self.atajo(id));
                        let letra = atajo.map(|a| a.corto()).unwrap_or_default();
                        let tecla_abajo = teclado && atajo.is_some_and(|a| ui.input(|i| a.abajo(i)));
                        let tecla_golpe = teclado && atajo.is_some_and(|a| ui.input(|i| a.apretado(i)));
                        let dedo = self.pads.dedos.values().find(|p| r.contains(**p)).copied();
                        let dedo_nuevo = nuevos.iter().find(|p| r.contains(**p)).copied();
                        // con pantalla táctil manda el dedo; con mouse, el puntero
                        let raton = !self.pads.tactil && resp.is_pointer_button_down_on();
                        let apretado = dedo.is_some() || raton || tecla_abajo;
                        let vel = dedo_nuevo.or(dedo).map(fuerza).or_else(|| if raton { resp.interact_pointer_pos().map(fuerza) } else { None }).unwrap_or(0.9);
                        let golpe_raton = !self.pads.tactil && (resp.drag_started() || resp.clicked() && !self.pads.ultimo.contains_key(&idx));
                        let mut golpe = dedo_nuevo.is_some() || golpe_raton || tecla_golpe;
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
                        p.text(r.center(), Align2::CENTER_CENTER, nombre, FontId::proportional((l * 0.15).clamp(12.0, 17.0)), Color32::WHITE);
                        if !self.plat.mobile && !letra.is_empty() {
                            // la tecla que toca este pad, en una cajita como una tecla (sin el número del pad: confundía)
                            let tam = (l * 0.16).clamp(16.0, 24.0);
                            let caja = Rect::from_min_size(r.left_top() + Vec2::new(9.0, 9.0), Vec2::splat(tam));
                            p.rect_filled(caja, 5, Color32::from_black_alpha(90));
                            p.rect_stroke(caja, 5, Stroke::new(1.0, Color32::from_gray(140)), StrokeKind::Inside);
                            let caja = if letra.chars().count() > 1 { Rect::from_min_size(caja.min, Vec2::new(tam * 0.55 * letra.chars().count() as f32 + 6.0, tam)) } else { caja };
                            p.text(caja.center(), Align2::CENTER_CENTER, &letra, FontId::monospace(tam * 0.6), Color32::from_gray(225));
                        }
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
            // dedos: cada uno toca su acorde (varios a la vez); suena y brilla mientras lo aprietas
            let nuevos = self.leer_dedos(ui);
            // un toque muy corto suena al menos esto (si no, apenas se oye)
            const MINIMO: f64 = 0.12;
            let recien_apretado = ui.input(|i| i.pointer.any_pressed());
            let mut soltar: Vec<usize> = vec![];
            ui.horizontal(|ui| {
                for g in 0..7u8 {
                    let gi = g as usize;
                    let (r, resp) = ui.allocate_exact_size(Vec2::new(lado, alto), Sense::click_and_drag());
                    let notas = crate::templates::diatonic_chord(&self.p, g, 48, self.pads.septima);
                    let nombre_ac = nombre_acorde_notas(&notas);
                    let dedo = self.pads.dedos.values().any(|p| r.contains(*p));
                    let dedo_nuevo = nuevos.iter().any(|p| r.contains(*p));
                    // con mouse (sin pantalla táctil): el botón apretado sobre el pad
                    let raton = !self.pads.tactil && resp.is_pointer_button_down_on();
                    let raton_nuevo = !self.pads.tactil && ((raton && recien_apretado) || (resp.clicked() && !self.pads.acordes.contains_key(&gi)));
                    let activo = dedo || raton;
                    if (dedo_nuevo || raton_nuevo) && !self.pads.acordes.contains_key(&gi) {
                        // el volumen se reparte entre las notas para que el acorde no sature
                        let vel = (0.8 / (notas.len() as f32).sqrt()).clamp(0.3, 0.8);
                        for &m in &notas {
                            self.cmd(Cmd::NoteOn { ch, m, v: vel });
                        }
                        if let Some(pp) = pos {
                            let paso = pp.round() as u32;
                            if let Some(pat) = self.p.pattern_mut(self.sel_pat) {
                                let len = pat.len_steps();
                                let lista = pat.notes.entry(ch).or_default();
                                for &m in &notas {
                                    lista.push(Note { s: paso % len, l: 4, m, v: vel, o: 0.0 });
                                }
                            }
                            self.cambios = true;
                            self.send();
                        }
                        self.pads.acordes.insert(gi, (ch, notas.clone(), ahora));
                    }
                    // al soltar (dedo o botón) se apaga ese acorde
                    if !activo {
                        if let Some((_, _, desde)) = self.pads.acordes.get(&gi) {
                            let falta = MINIMO - (ahora - desde);
                            if falta <= 0.0 {
                                soltar.push(gi);
                            } else {
                                ui.ctx().request_repaint_after(Duration::from_secs_f64(falta));
                            }
                        }
                    } else {
                        ui.ctx().request_repaint_after(Duration::from_millis(30));
                    }
                    // brilla solo mientras lo aprietas
                    let t = ui.ctx().animate_bool_with_time(resp.id.with("brillo"), activo, 0.08);
                    let p = ui.painter();
                    p.rect_filled(r.shrink(3.0), 14, lerp_col(Color32::from_gray(28), Color32::from_gray(200), 0.2 + 0.6 * t));
                    p.rect_stroke(r.shrink(3.0), 14, Stroke::new(1.5, lerp_col(Color32::from_gray(80), Color32::WHITE, t)), StrokeKind::Inside);
                    p.text(r.center() - Vec2::new(0.0, 10.0), Align2::CENTER_CENTER, GRADOS[gi], FontId::proportional(20.0), lerp_col(Color32::WHITE, Color32::from_gray(15), t));
                    p.text(r.center() + Vec2::new(0.0, 16.0), Align2::CENTER_CENTER, nombre_ac, FontId::proportional(12.0), lerp_col(Color32::from_gray(180), Color32::from_gray(30), t));
                }
            });
            for gi in soltar {
                if let Some((c0, viejas, _)) = self.pads.acordes.remove(&gi) {
                    for m in viejas {
                        self.cmd(Cmd::NoteOff { ch: c0, m });
                    }
                }
            }
        }
    }
}

impl StudioApp {
    /// Sigue cada dedo apoyado (para tocar varios pads a la vez). Devuelve dónde empezaron toques nuevos.
    /// Si saliste de Pads, los dedos y acordes que quedaron se sueltan.
    fn leer_dedos(&mut self, ui: &mut Ui) -> Vec<Pos2> {
        let mut nuevos = vec![];
        let cuadro = ui.ctx().cumulative_pass_nr();
        if cuadro > self.pads.cuadro + 1 {
            self.pads.dedos.clear();
            let viejos: Vec<(u32, Vec<u8>, f64)> = self.pads.acordes.drain().map(|(_, v)| v).collect();
            for (c0, notas, _) in viejos {
                for m in notas {
                    self.cmd(Cmd::NoteOff { ch: c0, m });
                }
            }
        }
        self.pads.cuadro = cuadro;
        ui.input(|i| {
            for e in &i.events {
                if let egui::Event::Touch { id, phase, pos, .. } = e {
                    self.pads.tactil = true;
                    match phase {
                        egui::TouchPhase::Start => {
                            self.pads.dedos.insert(id.0, *pos);
                            nuevos.push(*pos);
                        }
                        egui::TouchPhase::Move => {
                            self.pads.dedos.insert(id.0, *pos);
                        }
                        egui::TouchPhase::End | egui::TouchPhase::Cancel => {
                            self.pads.dedos.remove(&id.0);
                        }
                    }
                }
            }
        });
        if !self.pads.dedos.is_empty() {
            ui.ctx().request_repaint();
        }
        nuevos
    }

    /// Pone el kit del estilo `kit`: los pads quedan solo con sus sonidos.
    fn elegir_kit(&mut self, kit: usize) {
        self.snap();
        if self.pads.antes_kit.is_none() {
            // se guarda cómo estaba la batería, para «Reiniciar»
            let canales: Vec<Channel> = self.p.channels.iter().filter(|c| c.inst.is_drum()).cloned().collect();
            let ids: Vec<u32> = canales.iter().map(|c| c.id).collect();
            let pasos = self
                .p
                .patterns
                .iter()
                .map(|pat| {
                    let st = pat.steps.iter().filter(|(k, _)| ids.contains(k)).map(|(k, v)| (*k, v.clone())).collect();
                    let no = pat.notes.iter().filter(|(k, _)| ids.contains(k)).map(|(k, v)| (*k, v.clone())).collect();
                    (pat.id, st, no)
                })
                .collect();
            self.pads.antes_kit = Some(AntesKit { canales, pasos });
        }
        crate::templates::poner_kit(&mut self.p, kit);
        self.pads.kit = Some(kit);
        self.fix_selection();
        self.cambios = true;
        self.send();
        let n = self.p.channels.iter().filter(|c| c.inst.is_drum()).count();
        self.msg = format!("Kit «{}»: {n} sonidos en los pads.", crate::templates::KITS[kit]);
    }

    /// Vuelve la batería a como estaba antes de elegir un kit.
    fn reiniciar_pads(&mut self) {
        let Some(antes) = self.pads.antes_kit.take() else { return };
        self.snap();
        let ahora: Vec<u32> = self.p.channels.iter().filter(|c| c.inst.is_drum()).map(|c| c.id).collect();
        for pat in &mut self.p.patterns {
            for id in &ahora {
                pat.steps.remove(id);
                pat.notes.remove(id);
            }
        }
        self.p.autos.retain(|a| a.target.channel().is_none_or(|c| !ahora.contains(&c) || antes.canales.iter().any(|x| x.id == c)));
        let otros: Vec<Channel> = self.p.channels.drain(..).filter(|c| !c.inst.is_drum()).collect();
        self.p.channels = antes.canales;
        self.p.channels.extend(otros);
        for (pid, st, no) in antes.pasos {
            if let Some(pat) = self.p.pattern_mut(pid) {
                pat.steps.extend(st);
                pat.notes.extend(no);
            }
        }
        if self.p.channels.is_empty() {
            let id = self.p.new_id();
            self.p.channels.push(Channel::new(id, Inst::Kick));
        }
        self.pads.kit = None;
        self.fix_selection();
        self.cambios = true;
        self.send();
        self.msg = "Pads como estaban antes de elegir el kit.".into();
    }

    /// Pad «+»: carga un audio tuyo (de fuera de la app) como pad nuevo.
    fn pad_mas(&mut self, ui: &mut Ui, lado: Vec2) {
        let (r, resp) = ui.allocate_exact_size(lado, Sense::click());
        let lado = lado.y;
        let encima = resp.hovered();
        let p = ui.painter();
        p.rect_stroke(r.shrink(3.0), 14, Stroke::new(1.5, Color32::from_gray(if encima { 150 } else { 70 })), StrokeKind::Inside);
        p.text(r.center() - Vec2::new(0.0, 8.0), Align2::CENTER_CENTER, "+", FontId::proportional(lado * 0.3), Color32::from_gray(if encima { 230 } else { 150 }));
        p.text(r.center() + Vec2::new(0.0, lado * 0.2), Align2::CENTER_CENTER, "Audio tuyo", FontId::proportional(11.5), Color32::from_gray(130));
        let resp = resp.on_hover_text("Cargar un sonido de fuera de la app (WAV, MP3…) como pad nuevo");
        if resp.clicked() {
            if self.plat.mobile {
                // selector de archivos del teléfono; lo elegido va a un pad nuevo
                self.elegir_al_rack = true;
                self.elegir_audio = Some(None);
            } else if let Some(path) = self.pick_audio() {
                self.import_to_rack(&path);
                self.cambios = true;
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
