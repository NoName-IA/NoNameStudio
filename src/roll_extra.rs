//! Piano Roll: pantalla completa con «Volver», selección (recuadro) y sus acciones, menú de cada nota,
//! zoom, imán ajustable y adivinar la escala.
use super::*;

/// Tamaños del imán (en pasos de 1/16).
pub(super) const IMANES: [(u32, &str); 5] = [(1, "1/16"), (2, "1/8"), (4, "1/4"), (8, "1/2"), (16, "1 compás")];

impl StudioApp {
    /// Ancho de un paso y alto de una fila del Piano Roll, con el zoom aplicado.
    pub(super) fn cw(&self) -> f32 {
        CW * self.roll_zx
    }
    pub(super) fn rh(&self) -> f32 {
        RH * self.roll_zy
    }

    /// El Piano Roll ocupa toda la pantalla, como en FL Studio (en el teléfono y en la PC). En la PC,
    /// cerrar la zona de abajo (F6 otra vez) también lo cierra.
    pub(super) fn roll_es_completo(&self) -> bool {
        self.view == View::Roll && (self.plat.mobile || self.ver_vista)
    }

    /// Vuelve a donde estabas antes de abrir el Piano Roll.
    pub(super) fn roll_volver(&mut self) {
        self.roll_menu = None;
        if let Some(ch) = self.roll_ch() {
            self.roll_teclado_soltar(ch);
        }
        let v = match self.vista_antes_roll {
            View::Roll if self.plat.mobile => View::Playlist,
            View::Roll => View::Rack,
            v => v,
        };
        self.view = v;
        self.vista_prev = v;
        self.ver_vista = true;
    }

    /// Barra de arriba del Piano Roll a pantalla completa: volver, reproducir/parar y PAT/SONG.
    pub(super) fn roll_cabecera(&mut self, ui: &mut Ui) {
        Frame::new().fill(Color32::from_gray(16)).inner_margin(Margin::symmetric(10, 6)).show(ui, |ui| {
            ui.horizontal(|ui| {
                // «Volver» con una flecha dibujada (la fuente no trae ←)
                let r = pestana_metal(ui, false, "   Volver", 104.0).on_hover_text("Volver a donde estabas (Esc)");
                let c = Pos2::new(r.rect.left() + 20.0, r.rect.center().y);
                let trazo = Stroke::new(2.0, Color32::from_gray(225));
                ui.painter().line_segment([c + Vec2::new(4.0, -6.0), c + Vec2::new(-2.0, 0.0)], trazo);
                ui.painter().line_segment([c + Vec2::new(-2.0, 0.0), c + Vec2::new(4.0, 6.0)], trazo);
                if r.clicked() {
                    self.roll_volver();
                }
                ui.add_space(8.0);
                let sonando = self.playing();
                if icon_button(ui, if sonando { Ic::Pause } else { Ic::Play }, Vec2::new(40.0, 30.0), sonando, None).clicked() {
                    self.toggle_play();
                }
                if icon_button(ui, Ic::Stop, Vec2::new(40.0, 30.0), false, None).clicked() {
                    self.stop();
                }
                for (modo, t) in [(false, "PAT"), (true, "SONG")] {
                    if ui.add(egui::Button::selectable(self.song_mode == modo, t)).clicked() {
                        self.set_song_mode(modo);
                    }
                }
                ui.add_space(8.0);
                ui.label(RichText::new(format!("Piano Roll · {}", self.cur_pat().name)).strong().size(14.0));
            });
        });
        if !self.plat.mobile && ui.input(|i| i.key_pressed(Key::Escape)) && self.roll_menu.is_none() {
            self.roll_volver();
        }
    }

    /// Notas del canal en el patrón elegido (copia).
    fn notas_canal(&self, ch: u32) -> Vec<Note> {
        self.p.pattern(self.sel_pat).and_then(|p| p.notes.get(&ch)).cloned().unwrap_or_default()
    }

    /// Índices seleccionados que siguen existiendo, en orden.
    pub(super) fn roll_sel_validos(&self, ch: u32) -> Vec<usize> {
        let n = self.notas_canal(ch).len();
        let mut v: Vec<usize> = self.roll_sel.iter().copied().filter(|i| *i < n).collect();
        v.sort();
        v
    }

    /// Cambia las notas seleccionadas con `f` (recibe las notas, el largo del patrón).
    fn roll_sel_editar(&mut self, ch: u32, f: impl Fn(&mut [Note], u32)) {
        let idx = self.roll_sel_validos(ch);
        if idx.is_empty() {
            return;
        }
        self.snap();
        let pid = self.sel_pat;
        if let Some(p) = self.p.pattern_mut(pid) {
            let len = p.len_steps();
            if let Some(ns) = p.notes.get_mut(&ch) {
                let mut sub: Vec<Note> = idx.iter().map(|&i| ns[i]).collect();
                f(&mut sub, len);
                for (k, &i) in idx.iter().enumerate() {
                    let mut x = sub[k];
                    x.m = x.m.clamp(LOW, HIGH);
                    x.s = x.s.min(len - 1);
                    x.l = x.l.clamp(1, len - x.s);
                    ns[i] = x;
                }
            }
        }
        self.cambios = true;
        self.send();
    }

    /// Copia la selección (posiciones relativas a la primera nota).
    fn roll_copiar(&mut self, ch: u32) {
        let ns = self.notas_canal(ch);
        let sel: Vec<Note> = self.roll_sel_validos(ch).iter().map(|&i| ns[i]).collect();
        if let Some(s0) = sel.iter().map(|n| n.s).min() {
            self.roll_copia = sel.iter().map(|n| Note { s: n.s - s0, ..*n }).collect();
            self.msg = format!("{} notas copiadas.", self.roll_copia.len());
        }
    }

    fn roll_borrar_sel(&mut self, ch: u32) {
        let idx = self.roll_sel_validos(ch);
        if idx.is_empty() {
            return;
        }
        self.snap();
        let pid = self.sel_pat;
        if let Some(ns) = self.p.pattern_mut(pid).and_then(|p| p.notes.get_mut(&ch)) {
            for &i in idx.iter().rev() {
                ns.remove(i);
            }
        }
        self.roll_sel.clear();
        self.cambios = true;
        self.send();
    }

    /// Pega lo copiado desde el paso `desde`; lo pegado queda seleccionado.
    fn roll_pegar(&mut self, ch: u32, desde: u32) {
        if self.roll_copia.is_empty() {
            return;
        }
        self.snap();
        let pid = self.sel_pat;
        let copia = self.roll_copia.clone();
        if let Some(p) = self.p.pattern_mut(pid) {
            let len = p.len_steps();
            let ns = p.notes.entry(ch).or_default();
            let inicio = ns.len();
            for n in copia {
                let s = desde + n.s;
                if s < len {
                    ns.push(Note { s, l: n.l.min(len - s).max(1), ..n });
                }
            }
            self.roll_sel = (inicio..ns.len()).collect();
        }
        self.cambios = true;
        self.send();
    }

    /// Duplica la selección justo a la derecha; la copia queda seleccionada.
    fn roll_duplicar(&mut self, ch: u32) {
        let ns = self.notas_canal(ch);
        let idx = self.roll_sel_validos(ch);
        let (Some(a), Some(b)) = (idx.iter().map(|&i| ns[i].s).min(), idx.iter().map(|&i| ns[i].s + ns[i].l).max()) else { return };
        self.roll_copiar(ch);
        self.roll_pegar(ch, b.max(a + 1));
    }

    /// Adivina la tonalidad y la escala por las notas del patrón (todas las melódicas).
    pub(super) fn adivinar_escala(&mut self) {
        let mut cuenta = [0f32; 12];
        if let Some(p) = self.p.pattern(self.sel_pat) {
            for ns in p.notes.values() {
                for n in ns {
                    cuenta[(n.m % 12) as usize] += n.l as f32;
                }
            }
        }
        let total: f32 = cuenta.iter().sum();
        if total <= 0.0 {
            self.msg = "Escribe unas notas primero para adivinar la escala.".into();
            return;
        }
        let mut mejor = (f32::MIN, 0u8, 0u8);
        for (si, (_, mask)) in SCALES.iter().enumerate() {
            for key in 0..12u8 {
                let mut dentro = 0.0;
                for pc in 0..12 {
                    if mask[(pc + 12 - key as usize) % 12] {
                        dentro += cuenta[pc];
                    }
                }
                // la tónica suena más: pequeño premio para desempatar
                let puntaje = dentro - (total - dentro) * 2.0 + cuenta[key as usize] * 0.3;
                if puntaje > mejor.0 {
                    mejor = (puntaje, key, si as u8);
                }
            }
        }
        self.snap();
        self.p.key = mejor.1;
        self.p.scale = mejor.2;
        self.cambios = true;
        self.send();
        self.msg = format!("La escala parece {}.", self.p.key_label());
    }

    /// Barra de acciones de la selección (aparece si hay notas seleccionadas o algo copiado).
    pub(super) fn roll_barra_seleccion(&mut self, ui: &mut Ui, ch: u32) {
        let n = self.roll_sel_validos(ch).len();
        if n == 0 && self.roll_copia.is_empty() {
            return;
        }
        Frame::new().fill(Color32::from_gray(22)).inner_margin(Margin::symmetric(10, 4)).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                if n > 0 {
                    ui.label(RichText::new(format!("{n} seleccionadas")).small().color(ACCENT2));
                    if ui.button("Duplicar").clicked() {
                        self.roll_duplicar(ch);
                    }
                    if ui.button("Copiar").clicked() {
                        self.roll_copiar(ch);
                    }
                    if ui.button("Cortar").clicked() {
                        self.roll_copiar(ch);
                        self.roll_borrar_sel(ch);
                    }
                    if ui.button("Borrar").clicked() {
                        self.roll_borrar_sel(ch);
                    }
                    ui.separator();
                    for (t, d) in [("−8va", -12i32), ("−1", -1), ("+1", 1), ("+8va", 12)] {
                        if ui.button(t).on_hover_text("Transponer").clicked() {
                            self.roll_sel_editar(ch, |ns, _| {
                                for x in ns.iter_mut() {
                                    x.m = (x.m as i32 + d).clamp(0, 127) as u8;
                                }
                            });
                        }
                    }
                    ui.separator();
                    if ui.button("Invertir").on_hover_text("Da vuelta el orden en el tiempo (lo último queda primero)").clicked() {
                        self.roll_sel_editar(ch, |ns, _| {
                            let a = ns.iter().map(|x| x.s).min().unwrap_or(0);
                            let b = ns.iter().map(|x| x.s + x.l).max().unwrap_or(0);
                            for x in ns.iter_mut() {
                                x.s = a + b - (x.s + x.l);
                            }
                        });
                    }
                    if ui.button("Espejar").on_hover_text("Da vuelta las alturas: lo agudo queda grave y al revés").clicked() {
                        self.roll_sel_editar(ch, |ns, _| {
                            let lo = ns.iter().map(|x| x.m).min().unwrap_or(60) as i32;
                            let hi = ns.iter().map(|x| x.m).max().unwrap_or(60) as i32;
                            for x in ns.iter_mut() {
                                x.m = (lo + hi - x.m as i32).clamp(0, 127) as u8;
                            }
                        });
                    }
                    if ui.button("Legato").on_hover_text("Cada nota dura hasta la siguiente").clicked() {
                        self.roll_sel_editar(ch, |ns, len| {
                            let mut inicios: Vec<u32> = ns.iter().map(|x| x.s).collect();
                            inicios.sort();
                            inicios.dedup();
                            for x in ns.iter_mut() {
                                let sig = inicios.iter().copied().find(|s| *s > x.s).unwrap_or(len);
                                x.l = sig - x.s;
                            }
                        });
                    }
                    if ui.button("Humanizar").on_hover_text("Pequeñas variaciones de tiempo y fuerza").clicked() {
                        let semilla = now_secs() as u32;
                        self.roll_sel_editar(ch, |ns, _| {
                            for (i, x) in ns.iter_mut().enumerate() {
                                let r1 = (crate::dsp::hash(semilla, i as u32 * 3) >> 8) as f32 / (1u32 << 24) as f32;
                                let r2 = (crate::dsp::hash(semilla, i as u32 * 3 + 1) >> 8) as f32 / (1u32 << 24) as f32;
                                x.o = (r1 * 0.14).min(0.99);
                                x.v = (x.v * (0.88 + r2 * 0.24)).clamp(0.05, 1.0);
                            }
                        });
                    }
                    let iman = self.roll_iman.max(1);
                    if ui.button("Cuantizar").on_hover_text("Lleva el inicio de cada nota al imán").clicked() {
                        self.roll_sel_editar(ch, |ns, len| {
                            for x in ns.iter_mut() {
                                let s = ((x.s as f32 + x.o) / iman as f32).round() as u32 * iman;
                                x.s = s.min(len - 1);
                                x.o = 0.0;
                            }
                        });
                    }
                    ui.separator();
                    if ui.button("Nada").on_hover_text("Quitar la selección").clicked() {
                        self.roll_sel.clear();
                    }
                }
                if !self.roll_copia.is_empty() {
                    // pega donde empieza lo que se ve (o en el cabezal si suena)
                    let desde = self.local_step().map(|s| s as u32).unwrap_or((self.roll_x / self.cw()).max(0.0) as u32);
                    let iman = self.roll_iman.max(1);
                    let desde = desde / iman * iman;
                    if ui.button(format!("Pegar ({})", self.roll_copia.len())).on_hover_text("Pega lo copiado donde empieza la vista (o en el cabezal mientras suena)").clicked() {
                        self.roll_pegar(ch, desde);
                    }
                }
                if n == 0 {
                    if ui.button("Seleccionar todo").clicked() {
                        self.roll_sel = (0..self.notas_canal(ch).len()).collect();
                    }
                }
            });
        });
    }

    /// Menú de una nota (doble clic, o mantener apretada en el teléfono): fuerza, largo, octava…
    pub(super) fn roll_menu_nota(&mut self, ctx: &egui::Context) {
        let Some((i, pos, ch, cuadros)) = self.roll_menu else { return };
        let ns = self.notas_canal(ch);
        let Some(nota) = ns.get(i).copied() else {
            self.roll_menu = None;
            return;
        };
        self.roll_menu = Some((i, pos, ch, cuadros + 1));
        let pid = self.sel_pat;
        let iman = self.roll_iman.max(1);
        let mut cerrar = false;
        let area = egui::Area::new(egui::Id::new("roll_menu_nota")).order(egui::Order::Foreground).fixed_pos(pos + Vec2::new(14.0, 0.0)).constrain(true).show(ctx, |ui| {
            Frame::menu(ui.style()).show(ui, |ui| {
                ui.set_min_width(210.0);
                ui.label(RichText::new(format!("Nota {}", nname(nota.m))).strong());
                let mut v = nota.v;
                ui.horizontal(|ui| {
                    ui.label("Fuerza");
                    ui.spacing_mut().slider_width = 130.0;
                    if ui.add(egui::Slider::new(&mut v, 0.05..=1.0).custom_formatter(|x, _| format!("{:.0} %", x * 100.0))).changed() {
                        if let Some(n) = self.p.pattern_mut(pid).and_then(|p| p.notes.get_mut(&ch)).and_then(|ns| ns.get_mut(i)) {
                            n.v = v;
                        }
                        self.cambios = true;
                        self.cmd(Cmd::Project(Box::new(self.p.clone())));
                    }
                });
                let mut cambio = |this: &mut Self, f: &dyn Fn(&mut Note, u32)| {
                    this.snap();
                    if let Some(p) = this.p.pattern_mut(pid) {
                        let len = p.len_steps();
                        if let Some(n) = p.notes.get_mut(&ch).and_then(|ns| ns.get_mut(i)) {
                            f(n, len);
                            n.m = n.m.clamp(LOW, HIGH);
                            n.l = n.l.clamp(1, len - n.s.min(len - 1));
                        }
                    }
                    this.cambios = true;
                    this.send();
                };
                ui.horizontal(|ui| {
                    ui.label("Largo");
                    if ui.button("−").clicked() {
                        cambio(self, &|n, _| n.l = n.l.saturating_sub(iman).max(1));
                    }
                    if ui.button("+").clicked() {
                        cambio(self, &|n, len| n.l = (n.l + iman).min(len - n.s));
                    }
                    ui.label("Octava");
                    if ui.button("↓").clicked() {
                        cambio(self, &|n, _| n.m = n.m.saturating_sub(12));
                    }
                    if ui.button("↑").clicked() {
                        cambio(self, &|n, _| n.m = (n.m + 12).min(127));
                    }
                });
                ui.separator();
                if ui.button("Duplicar a la derecha").clicked() {
                    self.roll_sel = [i].into_iter().collect();
                    self.roll_duplicar(ch);
                    cerrar = true;
                }
                if ui.button("Copiar").clicked() {
                    self.roll_sel = [i].into_iter().collect();
                    self.roll_copiar(ch);
                    cerrar = true;
                }
                if ui.button("Seleccionar").clicked() {
                    self.roll_sel.insert(i);
                    cerrar = true;
                }
                if ui.button("Borrar").clicked() {
                    self.roll_sel = [i].into_iter().collect();
                    self.roll_borrar_sel(ch);
                    cerrar = true;
                }
            });
        });
        let fuera = ctx.input(|i| i.pointer.any_pressed()) && ctx.input(|i| i.pointer.interact_pos()).is_some_and(|p| !area.response.rect.contains(p));
        if cerrar || (fuera && cuadros > 1) || ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.roll_menu = None;
        }
    }
}
