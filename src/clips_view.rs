//! Clips de la playlist: portapapeles (copiar, cortar, pegar), acciones (duplicar, hacer único,
//! dividir, borrar, editar) y, en el teléfono, el gesto de mantener apretado para levantar un clip
//! con su barrita flotante de acciones.
use super::*;

/// Lo que hay en el portapapeles de la playlist.
#[derive(Clone)]
pub(super) enum ClipCopia {
    Pat(u32),
    Audio(AudioClip),
    Auto(AutoClip),
}

/// Un toque en curso sobre la playlist (teléfono): para distinguir arrastrar la vista, mantener
/// apretado para levantar un clip, y tocar.
pub(super) struct Toque {
    pub inicio: f64,
    pub pos0: Pos2,
    /// clip bajo el dedo al empezar: (tipo, índice, id)
    pub sobre: Option<(Kind, usize, u32)>,
    pub track: u32,
    pub step: u32,
    pub movido: bool,
    pub levantado: bool,
    /// el clip levantado ya se empezó a arrastrar (al soltar no se abre la barrita)
    pub arrastrado: bool,
}

/// Barrita flotante de acciones de un clip (o de un lugar vacío, para pegar).
pub(super) struct Barra {
    pub clip: Option<(Kind, usize, u32)>,
    pub ancla: Pos2,
    pub track: u32,
    pub step: u32,
    pub cuadros: u32,
}

/// Tiempo apretando sin mover para levantar un clip (s) y distancia que ya cuenta como arrastre.
pub(super) const LEVANTAR_S: f64 = 0.3;
pub(super) const MOVER_PX: f32 = 10.0;

impl StudioApp {
    /// Id del clip (el patrón para los clips de patrón).
    pub(super) fn id_de_clip(&self, kind: Kind, idx: usize) -> u32 {
        match kind {
            Kind::Pat => self.p.playlist.get(idx).map(|c| c.pat).unwrap_or(0),
            Kind::Audio => self.p.audio_clips.get(idx).map(|c| c.id).unwrap_or(0),
            Kind::Auto => self.p.autos.get(idx).map(|c| c.id).unwrap_or(0),
        }
    }

    /// Pista, inicio y largo (en pasos) de un clip.
    pub(super) fn lugar_de_clip(&self, kind: Kind, idx: usize) -> Option<(u32, u32, u32)> {
        match kind {
            Kind::Pat => self.p.playlist.get(idx).map(|c| (c.track, c.bar * STEPS_PER_BAR, self.p.pattern(c.pat).map(|p| p.bars).unwrap_or(1) * STEPS_PER_BAR)),
            Kind::Audio => self.p.audio_clips.get(idx).map(|c| (c.track, c.start, c.len)),
            Kind::Auto => self.p.autos.get(idx).map(|c| (c.track, c.start, c.len)),
        }
    }

    pub(super) fn copiar_clip(&mut self, kind: Kind, idx: usize) {
        self.pl_copia = match kind {
            Kind::Pat => self.p.playlist.get(idx).map(|c| ClipCopia::Pat(c.pat)),
            Kind::Audio => self.p.audio_clips.get(idx).cloned().map(ClipCopia::Audio),
            Kind::Auto => self.p.autos.get(idx).cloned().map(ClipCopia::Auto),
        };
        if self.pl_copia.is_some() {
            self.msg = "Clip copiado. «Pegar» lo pone donde elijas.".into();
        }
    }

    pub(super) fn borrar_clip(&mut self, kind: Kind, idx: usize) {
        self.snap();
        match kind {
            Kind::Pat if idx < self.p.playlist.len() => {
                self.p.playlist.remove(idx);
            }
            Kind::Audio if idx < self.p.audio_clips.len() => {
                self.p.audio_clips.remove(idx);
            }
            Kind::Auto if idx < self.p.autos.len() => {
                self.p.autos.remove(idx);
            }
            _ => {}
        }
        self.cambios = true;
        self.send();
    }

    pub(super) fn cortar_clip(&mut self, kind: Kind, idx: usize) {
        self.copiar_clip(kind, idx);
        self.borrar_clip(kind, idx);
        self.msg = "Clip cortado. «Pegar» lo pone donde elijas.".into();
    }

    /// Pega lo copiado en esa pista, desde ese paso (los patrones caen al inicio de su compás).
    pub(super) fn pegar_en(&mut self, track: u32, step: u32) {
        let Some(c) = self.pl_copia.clone() else { return };
        self.snap();
        match c {
            ClipCopia::Pat(pat) => {
                if self.p.pattern(pat).is_none() {
                    self.msg = "El patrón copiado ya no existe.".into();
                    self.hist.pop();
                    return;
                }
                self.p.playlist.push(Clip { track, bar: step / STEPS_PER_BAR, pat });
            }
            ClipCopia::Audio(a) => {
                let id = self.p.new_id();
                self.p.audio_clips.push(AudioClip { id, track, start: step, ..a });
            }
            ClipCopia::Auto(a) => {
                let id = self.p.new_id();
                self.p.autos.push(AutoClip { id, track, start: step, ..a });
            }
        }
        self.ensure_tracks(track + 1);
        self.cambios = true;
        self.send();
        self.msg = "Clip pegado.".into();
    }

    pub(super) fn duplicar_clip(&mut self, kind: Kind, idx: usize) {
        let Some((track, start, len)) = self.lugar_de_clip(kind, idx) else { return };
        self.snap();
        match kind {
            Kind::Pat => {
                if let Some(c) = self.p.playlist.get(idx).copied() {
                    self.p.playlist.push(Clip { bar: c.bar + len / STEPS_PER_BAR, ..c });
                }
            }
            Kind::Audio => {
                if let Some(c) = self.p.audio_clips.get(idx).cloned() {
                    let id = self.p.new_id();
                    self.p.audio_clips.push(AudioClip { id, track, start: start + len, ..c });
                }
            }
            Kind::Auto => {
                if let Some(c) = self.p.autos.get(idx).cloned() {
                    let id = self.p.new_id();
                    self.p.autos.push(AutoClip { id, track, start: start + len, ..c });
                }
            }
        }
        self.cambios = true;
        self.send();
    }

    /// El clip pasa a tener su propia copia del patrón: editarlo ya no cambia los otros clips.
    pub(super) fn hacer_unico(&mut self, idx: usize) {
        let Some(c) = self.p.playlist.get(idx).copied() else { return };
        let Some(pat) = self.p.pattern(c.pat).cloned() else { return };
        self.snap();
        let id = self.p.new_id();
        let n = self.p.patterns.iter().filter(|p| p.name.starts_with(&pat.name)).count() + 1;
        let mut nuevo = pat.clone();
        nuevo.id = id;
        nuevo.name = format!("{} ({n})", pat.name);
        self.p.patterns.push(nuevo);
        self.p.playlist[idx].pat = id;
        self.select_pat(id);
        self.cambios = true;
        self.send();
        self.msg = format!("Ahora este clip tiene su propio patrón «{} ({n})».", pat.name);
    }

    /// Abre el editor del clip.
    pub(super) fn editar_clip(&mut self, kind: Kind, idx: usize) {
        let id = self.id_de_clip(kind, idx);
        match kind {
            Kind::Pat => {
                self.select_pat(id);
                // en el teléfono se pasa a ver el patrón; en la PC queda en la zona de abajo
                let tiene_notas = self.p.pattern(id).is_some_and(|p| p.notes.values().any(|v| !v.is_empty()));
                self.view = if tiene_notas { View::Roll } else { View::Rack };
                if self.view == View::Roll {
                    self.roll_center = true;
                }
                self.ver_vista = true;
            }
            Kind::Audio => self.open_win(Win::Clip(id)),
            Kind::Auto => self.open_win(Win::Auto(id)),
        }
    }

    /// Barrita flotante con las acciones del clip levantado (teléfono).
    pub(super) fn barra_clip(&mut self, ctx: &egui::Context) {
        let Some(b) = &mut self.pl_barra else { return };
        b.cuadros += 1;
        let (clip, ancla, track, step, cuadros) = (b.clip, b.ancla, b.track, b.step, b.cuadros);
        // el clip puede haber cambiado (deshacer, borrado…): si ya no está, se cierra
        if let Some((k, i, id)) = clip {
            if self.id_de_clip(k, i) != id {
                self.pl_barra = None;
                return;
            }
        }
        let mut cerrar = false;
        let mut mas = false;
        let pantalla = ctx.content_rect();
        let area = egui::Area::new(egui::Id::new("pl_barra"))
            .order(egui::Order::Foreground)
            .pivot(Align2::LEFT_CENTER)
            .fixed_pos(ancla)
            .constrain(true)
            .show(ctx, |ui| {
                Frame::new()
                    .fill(Color32::from_gray(20))
                    .stroke(Stroke::new(1.0, Color32::from_gray(110)))
                    .corner_radius(14)
                    .inner_margin(Margin::symmetric(8, 6))
                    .shadow(egui::Shadow { offset: [0, 6], blur: 18, spread: 0, color: Color32::from_black_alpha(160) })
                    .show(ui, |ui| {
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(0.0, 3.0);
                            // lista vertical: un botón por fila, todos del mismo ancho, texto a la izquierda
                            let boton = |ui: &mut Ui, t: &str| {
                                ui.add(egui::Button::new(RichText::new(t).size(14.0)).frame(false).min_size(Vec2::new(200.0, 34.0)).right_text("")).clicked()
                            };
                            match clip {
                                Some((k, i, _)) => {
                                    if boton(ui, "Editar") {
                                        self.editar_clip(k, i);
                                        cerrar = true;
                                    }
                                    if boton(ui, "Copiar") {
                                        self.copiar_clip(k, i);
                                        cerrar = true;
                                    }
                                    if boton(ui, "Cortar") {
                                        self.cortar_clip(k, i);
                                        cerrar = true;
                                    }
                                    if self.pl_copia.is_some() && boton(ui, "Pegar después") {
                                        if let Some((t, s, l)) = self.lugar_de_clip(k, i) {
                                            self.pegar_en(t, s + l);
                                        }
                                        cerrar = true;
                                    }
                                    if boton(ui, "Duplicar") {
                                        self.duplicar_clip(k, i);
                                        cerrar = true;
                                    }
                                    if k == Kind::Pat && boton(ui, "Hacer único") {
                                        self.hacer_unico(i);
                                        cerrar = true;
                                    }
                                    if k == Kind::Audio {
                                        if let Some((_, s, l)) = self.lugar_de_clip(k, i) {
                                            let corte = (step / 4 * 4).clamp(s + 1, s + l - 1);
                                            if l > 1 && boton(ui, "Dividir aquí") {
                                                let id = self.id_de_clip(k, i);
                                                self.split_clip(id, corte);
                                                cerrar = true;
                                            }
                                        }
                                    }
                                    if boton(ui, "Borrar") {
                                        self.borrar_clip(k, i);
                                        cerrar = true;
                                    }
                                    if boton(ui, "⋯ Más") {
                                        mas = true;
                                        cerrar = true;
                                    }
                                }
                                None => {
                                    if self.pl_copia.is_some() && boton(ui, "Pegar aquí") {
                                        self.pegar_en(track, step);
                                        cerrar = true;
                                    }
                                    if boton(ui, &format!("Poner «{}» aquí", self.cur_pat().name)) {
                                        self.snap();
                                        let pat = self.sel_pat;
                                        self.p.playlist.push(Clip { track, bar: step / STEPS_PER_BAR, pat });
                                        self.cambios = true;
                                        self.send();
                                        cerrar = true;
                                    }
                                }
                            }
                        });
                    });
            });
        // «Más»: el menú completo de siempre, en el mismo lugar
        if mas {
            if let Some((k, i, id)) = clip {
                self.pl_menu = Some(PlMenu { kind: k, id, idx: i, pos: ancla, step, opened: 0 });
            }
        }
        let fuera = ctx.input(|i| i.pointer.any_pressed()) && ctx.input(|i| i.pointer.interact_pos()).is_some_and(|p| !area.response.rect.contains(p));
        if cerrar || (fuera && cuadros > 1) || ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.pl_barra = None;
        }
    }

    /// Copiar / cortar / pegar en el menú de clic derecho (PC). Devuelve si hay que cerrar el menú.
    pub(super) fn menu_portapapeles(&mut self, ui: &mut Ui, kind: Kind, idx: usize, step: u32) -> bool {
        let mut cerrar = false;
        ui.horizontal(|ui| {
            if ui.button("Copiar").clicked() {
                self.copiar_clip(kind, idx);
                cerrar = true;
            }
            if ui.button("Cortar").clicked() {
                self.cortar_clip(kind, idx);
                cerrar = true;
            }
            if ui.add_enabled(self.pl_copia.is_some(), egui::Button::new("Pegar aquí")).clicked() {
                if let Some((t, _, _)) = self.lugar_de_clip(kind, idx) {
                    self.pegar_en(t, step);
                }
                cerrar = true;
            }
        });
        if kind == Kind::Pat && ui.button("Hacer único (su propio patrón)").clicked() {
            self.hacer_unico(idx);
            cerrar = true;
        }
        ui.separator();
        cerrar
    }
}
