//! Retune: edita la voz nota por nota (como un editor de afinación): mover notas, llevarlas a la
//! escala y quitar el vaivén del tono. Al aplicar se crea una toma nueva; la original queda en «Tomas».
use super::*;
use crate::fx::{NotaRt, RT_CENTRO, RT_HOP};
use crate::media::{self, Buf};

/// Estado del editor Retune de un clip.
pub(super) struct Retune {
    pub clip: u32,
    pub file: String,
    pub sr: f32,
    pub mono: Arc<Vec<f32>>,
    pub cont: Vec<f32>,
    pub notas: Vec<NotaRt>,
    pub orig: Vec<NotaRt>,
    pub picos: Vec<f32>,
    pub aplanar: f32,
    pub sel: Option<usize>,
    pub drag: Option<(usize, f32)>,
    pub zoom: f32,
    /// reproducción: cuándo empezó, desde qué segundo y si suena con los cambios
    pub sonando: Option<(Instant, f32, bool)>,
    /// escuchar «con cambios» (true) u «original» (false)
    pub con_cambios: bool,
    /// segundo desde donde empieza a sonar
    pub desde: f32,
    /// voz ya procesada con los cambios (clave de los cambios, audio) y cálculo en curso
    pub proc: Option<(u64, Arc<Vec<f32>>)>,
    pub calculando: Option<u64>,
    /// pedido de sonar en cuanto termine el cálculo (desde el segundo…)
    pub quiere: Option<f32>,
}

impl Retune {
    /// Clave de los cambios actuales (para saber si hay que volver a calcular la voz).
    fn clave(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for n in &self.notas {
            n.objetivo.to_bits().hash(&mut h);
        }
        self.aplanar.to_bits().hash(&mut h);
        h.finish()
    }
    fn dur(&self) -> f32 {
        self.mono.len() as f32 / self.sr
    }
    /// Segundo que está sonando ahora (o desde donde va a sonar).
    pub fn pos(&self) -> f32 {
        match self.sonando {
            Some((t0, off, _)) => off + t0.elapsed().as_secs_f32(),
            None => self.desde,
        }
    }
}

const NOMBRES: [&str; 12] = ["Do", "Do#", "Re", "Re#", "Mi", "Fa", "Fa#", "Sol", "Sol#", "La", "La#", "Si"];

fn nombre_nota(m: f32) -> String {
    let r = m.round() as i32;
    format!("{}{}", NOMBRES[r.rem_euclid(12) as usize], r.div_euclid(12) - 1)
}

impl StudioApp {
    /// Abre el Retune de un clip: analiza la voz en segundo plano.
    pub(super) fn abrir_retune(&mut self, id: u32) {
        self.open_win(Win::Retune(id));
        if self.retune.as_ref().is_some_and(|r| r.clip == id) || self.retune_cargando == Some(id) {
            return;
        }
        let Some(c) = self.p.audio_clip(id).cloned() else { return };
        let file = c.file().to_string();
        let Some(path) = media::resolve(&self.plat.data_dir, &file) else {
            self.msg = "No se encuentra el audio del clip.".into();
            return;
        };
        self.retune_detener();
        self.retune = None;
        self.retune_cargando = Some(id);
        let tx = self.file_tx.clone();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let r = (|| -> Result<Retune, String> {
                let b = media::decode(&path)?;
                let mono = b.mono();
                let cont = crate::fx::contorno(&mono, b.sr);
                let notas = crate::fx::notas_de(&cont);
                let picos: Vec<f32> = mono.chunks(RT_HOP).map(|c| c.iter().fold(0f32, |m, v| m.max(v.abs()))).collect();
                let zoom = (860.0 / cont.len().max(1) as f32).clamp(0.6, 6.0);
                Ok(Retune { clip: id, file, sr: b.sr, mono: Arc::new(mono), cont, orig: notas.clone(), notas, picos, aplanar: 0.0, sel: None, drag: None, zoom, sonando: None, con_cambios: true, desde: 0.0, proc: None, calculando: None, quiere: None })
            })();
            let _ = tx.send(FileMsg::Retune(id, r));
            if let Some(c) = ctx {
                c.request_repaint();
            }
        });
    }

    /// Nota de la escala del panel NN Tune más cercana a `m`.
    fn nota_en_escala(&self, m: f32) -> f32 {
        let mask = scale_mask(self.tune_tonica(), self.tune_escala());
        let base = m.round() as i32;
        let mut mejor = base;
        let mut bd = f32::MAX;
        for n in base - 6..=base + 6 {
            if (mask >> n.rem_euclid(12)) & 1 == 1 && (n as f32 - m).abs() < bd {
                bd = (n as f32 - m).abs();
                mejor = n;
            }
        }
        mejor as f32
    }

    /// Para lo que esté sonando del Retune.
    pub(super) fn retune_detener(&mut self) {
        let sonaba = self.retune.as_ref().is_some_and(|r| r.sonando.is_some() || r.quiere.is_some());
        if let Some(rt) = &mut self.retune {
            if let Some((t0, off, _)) = rt.sonando.take() {
                rt.desde = (off + t0.elapsed().as_secs_f32()).min(rt.dur());
                if rt.desde >= rt.dur() - 0.05 {
                    rt.desde = 0.0;
                }
            }
            rt.quiere = None;
        }
        if sonaba {
            self.cmd(Cmd::PreviewFile(None));
        }
    }

    /// Hace sonar la voz desde el segundo `desde` (original o con cambios, según el selector).
    fn retune_tocar(&mut self, desde: f32) {
        let Some(rt) = &mut self.retune else { return };
        let desde = desde.clamp(0.0, (rt.dur() - 0.05).max(0.0));
        let audio = if rt.con_cambios {
            let clave = rt.clave();
            match &rt.proc {
                Some((k, a)) if *k == clave => Some(a.clone()),
                _ => {
                    // hay que calcular la voz con los cambios: suena en cuanto esté lista
                    rt.quiere = Some(desde);
                    if rt.calculando != Some(clave) {
                        rt.calculando = Some(clave);
                        let (id, mono, cont, notas, aplanar, sr) = (rt.clip, rt.mono.clone(), rt.cont.clone(), rt.notas.clone(), rt.aplanar, rt.sr);
                        let tx = self.file_tx.clone();
                        let ctx = self.ctx.clone();
                        std::thread::spawn(move || {
                            let data = crate::fx::retune(&mono, sr, &cont, &notas, aplanar);
                            let _ = tx.send(FileMsg::RetuneListo(id, clave, data));
                            if let Some(c) = ctx {
                                c.request_repaint();
                            }
                        });
                    }
                    None
                }
            }
        } else {
            Some(rt.mono.clone())
        };
        let Some(audio) = audio else { return };
        let i0 = ((desde * rt.sr) as usize).min(audio.len());
        let buf = Buf { ch: 1, sr: rt.sr, data: audio[i0..].to_vec() };
        rt.sonando = Some((Instant::now(), desde, rt.con_cambios));
        rt.quiere = None;
        self.cmd(Cmd::PreviewFile(Some(Arc::new(buf))));
    }

    /// Llegó la voz calculada con los cambios.
    pub(super) fn retune_listo(&mut self, id: u32, clave: u64, data: Vec<f32>) {
        let Some(rt) = &mut self.retune else { return };
        if rt.clip != id {
            return;
        }
        if rt.calculando == Some(clave) {
            rt.calculando = None;
        }
        rt.proc = Some((clave, Arc::new(data)));
        if let Some(desde) = rt.quiere {
            if rt.con_cambios && rt.clave() == clave {
                self.retune_tocar(desde);
            }
        }
    }

    /// Después de cambiar algo: si está sonando con cambios, se vuelve a calcular y sigue desde donde iba.
    fn retune_refrescar(&mut self) {
        let Some(rt) = &self.retune else { return };
        if rt.con_cambios && (rt.sonando.is_some() || rt.quiere.is_some()) {
            let pos = rt.pos();
            self.retune_tocar(pos);
        }
    }

    /// Crea la toma nueva con los cambios.
    fn retune_aplicar(&mut self) {
        let Some(rt) = &self.retune else { return };
        let (id, mono, cont, notas, aplanar, sr, file) = (rt.clip, rt.mono.clone(), rt.cont.clone(), rt.notas.clone(), rt.aplanar, rt.sr, rt.file.clone());
        let dir = self.plat.data_dir.join("audio");
        let tx = self.file_tx.clone();
        let ctx = self.ctx.clone();
        self.msg = "Aplicando el Retune…".into();
        std::thread::spawn(move || {
            let r = (|| -> Result<(String, Buf), String> {
                let data = crate::fx::retune(&mono, sr, &cont, &notas, aplanar);
                let _ = std::fs::create_dir_all(&dir);
                let stem = std::path::Path::new(&file).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "voz".into());
                let mut name = format!("{stem} (retune).wav");
                let mut k = 2;
                while dir.join(&name).exists() {
                    name = format!("{stem} (retune {k}).wav");
                    k += 1;
                }
                let out = Buf { ch: 1, sr, data };
                media::write_wav(&dir.join(&name), &out).map_err(|e| e.to_string())?;
                Ok((name, out))
            })();
            let _ = tx.send(FileMsg::Afinado(id, r));
            if let Some(c) = ctx {
                c.request_repaint();
            }
        });
    }

    pub(super) fn retune_window(&mut self, ui: &mut Ui, id: u32) {
        if self.retune.as_ref().map(|r| r.clip) != Some(id) {
            if self.retune_cargando == Some(id) {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Analizando la voz y buscando las notas…");
                });
            } else if ui.button("Analizar la voz de este clip").clicked() {
                self.abrir_retune(id);
            }
            return;
        }
        // ---------- barra de herramientas
        let mut accion: Option<u8> = None;
        let (sonando, con_cambios, calculando, pos, dur) = {
            let rt = self.retune.as_ref().unwrap();
            (rt.sonando.is_some() || rt.quiere.is_some(), rt.con_cambios, rt.quiere.is_some(), rt.pos(), rt.dur())
        };
        // se acabó el audio
        if sonando && !calculando && pos >= dur {
            self.retune_detener();
            if let Some(rt) = &mut self.retune {
                rt.desde = 0.0;
            }
        }
        if sonando {
            ui.ctx().request_repaint();
        }
        ui.horizontal(|ui| {
            // botón redondo: reproducir / detener
            let (r, resp) = ui.allocate_exact_size(Vec2::new(36.0, 36.0), Sense::click());
            let p = ui.painter();
            p.circle_filled(r.center(), 17.0, if resp.hovered() { Color32::from_gray(231) } else { Color32::WHITE });
            let c = r.center();
            let oscuro = Color32::from_gray(17);
            if sonando {
                p.rect_filled(Rect::from_center_size(c, Vec2::new(11.0, 11.0)), 2, oscuro);
            } else {
                p.add(egui::Shape::convex_polygon(vec![c + Vec2::new(-4.0, -7.0), c + Vec2::new(-4.0, 7.0), c + Vec2::new(7.5, 0.0)], oscuro, Stroke::NONE));
            }
            if resp.on_hover_text(if sonando { "Detener" } else { "Escuchar" }).clicked() {
                accion = Some(if sonando { 2 } else { 0 });
            }
            ui.add_space(6.0);
            // original / con cambios (se puede cambiar mientras suena, sigue desde el mismo lugar)
            for (k, n) in [(false, "Original"), (true, "Con cambios")] {
                if ui.add(egui::Button::selectable(con_cambios == k, n)).on_hover_text("Compara: cambia mientras suena y sigue desde el mismo punto").clicked() && con_cambios != k {
                    accion = Some(if k { 9 } else { 8 });
                }
            }
            let mmss = |t: f32| format!("{}:{:04.1}", (t / 60.0) as u32, t % 60.0);
            ui.label(RichText::new(format!("{} / {}", mmss(pos.min(dur)), mmss(dur))).monospace().color(if sonando { ACCENT2 } else { DIM }));
            if calculando {
                ui.spinner();
                ui.label(RichText::new("preparando los cambios…").small().color(DIM));
            }
            ui.separator();
            if ui.button("Afinar todas a la escala").on_hover_text(format!("Lleva cada nota a la nota más cercana de {}", self.tune_key_label())).clicked() {
                accion = Some(3);
            }
            if ui.button("Restablecer").on_hover_text("Deja todas las notas como estaban").clicked() {
                accion = Some(4);
            }
            ui.separator();
            if ui.button("Alejar").clicked() {
                accion = Some(5);
            }
            if ui.button("Acercar").clicked() {
                accion = Some(6);
            }
        });
        let rt_aplanar = self.retune.as_ref().map(|r| r.aplanar).unwrap_or(0.0);
        let mut aplanar = rt_aplanar;
        let mut soltar_aplanar = false;
        ui.horizontal(|ui| {
            ui.label(RichText::new("Quitar vaivén").small().color(DIM));
            ui.spacing_mut().slider_width = 180.0;
            let r = ui.add(egui::Slider::new(&mut aplanar, 0.0..=1.0).custom_formatter(|v, _| format!("{:.0} %", v * 100.0))).on_hover_text("Aplana el tono dentro de cada nota (100 % = nota totalmente recta)");
            soltar_aplanar = r.drag_stopped() || (r.changed() && !r.dragged());
            ui.add_space(16.0);
            if boton_metal(ui, "Aplicar (toma nueva)", Vec2::new(180.0, 34.0)).clicked() {
                accion = Some(7);
            }
        });
        let mut refrescar = soltar_aplanar;
        match accion {
            Some(0) => {
                let desde = self.retune.as_ref().map(|r| r.desde).unwrap_or(0.0);
                self.retune_tocar(desde);
            }
            Some(2) => self.retune_detener(),
            Some(8) | Some(9) => {
                let pos = self.retune.as_ref().map(|r| r.pos()).unwrap_or(0.0);
                if let Some(rt) = &mut self.retune {
                    rt.con_cambios = accion == Some(9);
                }
                if sonando {
                    self.retune_tocar(pos);
                }
            }
            Some(3) => {
                let objetivos: Vec<f32> = self.retune.as_ref().map(|r| r.notas.iter().map(|n| self.nota_en_escala(n.centro)).collect()).unwrap_or_default();
                if let Some(rt) = &mut self.retune {
                    for (n, o) in rt.notas.iter_mut().zip(objetivos) {
                        n.objetivo = o;
                    }
                }
                refrescar = true;
            }
            Some(4) => {
                if let Some(rt) = &mut self.retune {
                    rt.notas = rt.orig.clone();
                    rt.aplanar = 0.0;
                    aplanar = 0.0;
                }
                refrescar = true;
            }
            Some(5) => {
                if let Some(rt) = &mut self.retune {
                    rt.zoom = (rt.zoom / 1.4).max(0.3);
                }
            }
            Some(6) => {
                if let Some(rt) = &mut self.retune {
                    rt.zoom = (rt.zoom * 1.4).min(12.0);
                }
            }
            Some(7) => {
                self.retune_detener();
                self.retune_aplicar();
            }
            _ => {}
        }
        let mascara = scale_mask(self.tune_tonica(), self.tune_escala());
        let a_escala: Vec<f32> = self.retune.as_ref().map(|r| r.notas.iter().map(|n| self.nota_en_escala(n.centro)).collect()).unwrap_or_default();
        let mut ir_a: Option<f32> = None;
        let Some(rt) = &mut self.retune else { return };
        rt.aplanar = aplanar;
        if rt.notas.is_empty() {
            ui.label(RichText::new("No se encontraron notas cantadas en este clip.").color(DIM));
            return;
        }
        // ---------- rango de notas a la vista
        let lo = rt.notas.iter().map(|n| n.centro.min(n.objetivo)).fold(f32::MAX, f32::min).floor() - 3.0;
        let hi = rt.notas.iter().map(|n| n.centro.max(n.objetivo)).fold(f32::MIN, f32::max).ceil() + 3.0;
        let (lo, hi) = if hi - lo < 14.0 { let c = (lo + hi) * 0.5; ((c - 7.0).floor(), (c + 7.0).ceil()) } else { (lo, hi) };
        let alto = 380.0f32;
        let fila = alto / (hi - lo);
        let ancho_vista = 880.0f32;
        let n_h = rt.cont.len();
        let z = rt.zoom;
        let shift = ui.input(|i| i.modifiers.shift);
        ui.add_space(4.0);
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            // nombres de las notas a la izquierda
            let (rl, _) = ui.allocate_exact_size(Vec2::new(44.0, alto), Sense::hover());
            let p = ui.painter();
            p.rect_filled(rl, 0, Color32::from_gray(20));
            for m in (lo as i32)..(hi as i32) {
                let y0 = rl.top() + (hi - m as f32 - 1.0) * fila;
                let negra = [1, 3, 6, 8, 10].contains(&m.rem_euclid(12));
                if negra {
                    p.rect_filled(Rect::from_min_size(Pos2::new(rl.left(), y0), Vec2::new(44.0, fila)), 0, Color32::from_gray(14));
                }
                if fila >= 11.0 || m.rem_euclid(12) == 0 {
                    p.text(Pos2::new(rl.right() - 5.0, y0 + fila * 0.5), Align2::RIGHT_CENTER, nombre_nota(m as f32), FontId::proportional(10.0), if m.rem_euclid(12) == 0 { Color32::WHITE } else { DIM });
                }
            }
            egui::ScrollArea::horizontal().id_salt("retune_scroll").max_width(ancho_vista).show(ui, |ui| {
                let w = (n_h as f32 * z).max(ancho_vista);
                let (r, resp) = ui.allocate_exact_size(Vec2::new(w, alto), Sense::click_and_drag());
                let p = ui.painter_at(r);
                p.rect_filled(r, 0, Color32::from_gray(22));
                let y_de = |m: f32| r.top() + (hi - m) * fila;
                // filas: negras más oscuras, notas de la escala marcadas
                for m in (lo as i32)..(hi as i32) {
                    let y0 = y_de(m as f32 + 1.0);
                    let fr = Rect::from_min_size(Pos2::new(r.left(), y0), Vec2::new(w, fila));
                    let en = (mascara >> m.rem_euclid(12)) & 1 == 1;
                    if [1, 3, 6, 8, 10].contains(&m.rem_euclid(12)) {
                        p.rect_filled(fr, 0, Color32::from_gray(18));
                    }
                    if en {
                        p.rect_filled(fr, 0, Color32::from_rgba_unmultiplied(61, 139, 255, 10));
                    }
                    p.hline(r.x_range(), y0, Stroke::new(1.0, Color32::from_gray(31)));
                }
                // segundos
                let x_de = |h: f32| r.left() + h * z;
                let segs = (n_h * RT_HOP) as f32 / rt.sr;
                let paso_s = if z < 1.0 { 5.0 } else { 1.0 };
                let mut s = 0.0;
                while s < segs {
                    let x = x_de((s * rt.sr - RT_CENTRO as f32).max(0.0) / RT_HOP as f32);
                    p.vline(x, r.y_range(), Stroke::new(1.0, Color32::from_gray(35)));
                    p.text(Pos2::new(x + 3.0, r.top() + 2.0), Align2::LEFT_TOP, format!("{s:.0} s"), FontId::proportional(10.0), DIM);
                    s += paso_s;
                }
                // forma de onda tenue abajo
                for (h, pk) in rt.picos.iter().enumerate() {
                    let x = x_de(h as f32 - RT_CENTRO as f32 / RT_HOP as f32);
                    if x < r.left() || x > r.right() {
                        continue;
                    }
                    let a = pk.min(1.0) * 36.0;
                    p.vline(x, (r.bottom() - 2.0 - a)..=(r.bottom() - 2.0), Stroke::new(z.max(1.0), Color32::from_gray(46)));
                }
                // tono de la voz fuera de las notas (gris)
                let mut tramo: Vec<Pos2> = vec![];
                let mut en_nota = vec![false; n_h];
                for n in &rt.notas {
                    for h in n.a..n.b.min(n_h) {
                        en_nota[h] = true;
                    }
                }
                for h in 0..n_h {
                    let c = rt.cont[h];
                    if c > 0.0 && !en_nota[h] && c > lo && c < hi {
                        tramo.push(Pos2::new(x_de(h as f32), y_de(c)));
                    } else if tramo.len() > 1 {
                        p.add(egui::Shape::line(std::mem::take(&mut tramo), Stroke::new(1.0, Color32::from_rgb(90, 96, 108))));
                    } else {
                        tramo.clear();
                    }
                }
                if tramo.len() > 1 {
                    p.add(egui::Shape::line(tramo, Stroke::new(1.0, Color32::from_rgb(90, 96, 108))));
                }
                // ---------- interacción
                let puntero = resp.interact_pointer_pos().or(resp.hover_pos());
                let bajo = |pos: Pos2, notas: &[NotaRt]| -> Option<usize> {
                    notas.iter().position(|n| {
                        let (x0, x1) = (x_de(n.a as f32), x_de(n.b as f32));
                        pos.x >= x0 && pos.x <= x1 && (pos.y - y_de(n.objetivo)).abs() <= (fila * 0.6).max(9.0)
                    })
                };
                if resp.drag_started() {
                    if let Some(pos) = resp.interact_pointer_pos() {
                        rt.drag = bajo(pos, &rt.notas).map(|i| (i, rt.notas[i].objetivo));
                        rt.sel = rt.drag.map(|d| d.0);
                    }
                }
                if let (Some((i, inicio)), true) = (rt.drag, resp.dragged()) {
                    let total = resp.total_drag_delta().map(|d| d.y).unwrap_or(0.0);
                    let nuevo = inicio - total / fila;
                    rt.notas[i].objetivo = if shift { nuevo } else { nuevo.round() };
                }
                if resp.drag_stopped() {
                    if rt.drag.is_some() {
                        refrescar = true;
                    }
                    rt.drag = None;
                }
                // clic en la regla de arriba (o en un espacio vacío): escuchar desde ahí
                if resp.clicked() {
                    if let Some(pos) = puntero {
                        if pos.y < r.top() + 16.0 || bajo(pos, &rt.notas).is_none() {
                            let h = (pos.x - r.left()) / z;
                            ir_a = Some(((h * RT_HOP as f32 + RT_CENTRO as f32) / rt.sr).max(0.0));
                        }
                    }
                }
                if resp.clicked() {
                    rt.sel = puntero.and_then(|pos| bajo(pos, &rt.notas));
                }
                if resp.double_clicked() {
                    if let Some(i) = puntero.and_then(|pos| bajo(pos, &rt.notas)) {
                        rt.notas[i].objetivo = a_escala[i];
                        rt.sel = Some(i);
                        refrescar = true;
                    }
                }
                // flechas: mover la nota elegida un semitono
                if let Some(i) = rt.sel {
                    let (arriba, abajo) = ui.input(|inp| (inp.key_pressed(Key::ArrowUp), inp.key_pressed(Key::ArrowDown)));
                    if arriba {
                        rt.notas[i].objetivo = (rt.notas[i].objetivo + 1.0).round();
                        refrescar = true;
                    }
                    if abajo {
                        rt.notas[i].objetivo = (rt.notas[i].objetivo - 1.0).round();
                        refrescar = true;
                    }
                }
                // ---------- notas
                let hover = puntero.and_then(|pos| bajo(pos, &rt.notas));
                for (i, n) in rt.notas.iter().enumerate() {
                    let (x0, x1) = (x_de(n.a as f32), x_de(n.b as f32));
                    if x1 < r.left() || x0 > r.right() {
                        continue;
                    }
                    let h_n = (fila * 0.85).max(8.0);
                    let movida = (n.objetivo - n.centro).abs() > 0.01;
                    if movida {
                        // dónde estaba la nota
                        let g = Rect::from_center_size(Pos2::new((x0 + x1) * 0.5, y_de(n.centro)), Vec2::new(x1 - x0, h_n));
                        p.rect_stroke(g, 5, Stroke::new(1.0, Color32::from_rgb(80, 86, 98)), StrokeKind::Inside);
                    }
                    let caja = Rect::from_center_size(Pos2::new((x0 + x1) * 0.5, y_de(n.objetivo)), Vec2::new((x1 - x0).max(3.0), h_n));
                    let sel = rt.sel == Some(i);
                    let base = if movida { Color32::from_rgb(58, 214, 140) } else { ACCENT };
                    let fondo = lerp_col(base, BG, if sel || hover == Some(i) { 0.25 } else { 0.45 });
                    p.rect(caja, 5, fondo, Stroke::new(if sel { 2.0 } else { 1.0 }, if sel { Color32::WHITE } else { lerp_col(base, Color32::WHITE, 0.2) }), StrokeKind::Inside);
                    // tono real dentro de la nota (cómo va a sonar)
                    let pts: Vec<Pos2> = (n.a..n.b.min(n_h))
                        .filter(|h| rt.cont[*h] > 0.0)
                        .map(|h| Pos2::new(x_de(h as f32), y_de(n.objetivo + (rt.cont[h] - n.centro) * (1.0 - rt.aplanar))))
                        .collect();
                    if pts.len() > 1 {
                        p.add(egui::Shape::line(pts, Stroke::new(1.4, Color32::from_rgba_unmultiplied(255, 255, 255, 200))));
                    }
                    if x1 - x0 > 34.0 {
                        p.text(Pos2::new(x0 + 4.0, caja.top() - 2.0), Align2::LEFT_BOTTOM, nombre_nota(n.objetivo), FontId::proportional(10.5), Color32::WHITE);
                    }
                }
                // cabezal: por dónde va (blanco con punto arriba, como en la playlist)
                let pos_s = rt.pos();
                let xc = x_de((pos_s * rt.sr - RT_CENTRO as f32) / RT_HOP as f32);
                if xc >= r.left() && xc <= r.right() {
                    p.vline(xc, (r.top() + 4.0)..=r.bottom(), Stroke::new(1.5, if rt.sonando.is_some() { Color32::WHITE } else { Color32::from_rgb(150, 156, 168) }));
                    p.circle_filled(Pos2::new(xc, r.top() + 4.0), 4.0, Color32::WHITE);
                }
                if let Some(i) = hover {
                    let n = rt.notas[i];
                    let cents = ((n.objetivo - n.objetivo.round()) * 100.0).round();
                    let desde = if (n.objetivo - n.centro).abs() > 0.01 { format!(" · antes {}", nombre_nota(n.centro)) } else { format!(" · {:+.0} ¢ de {}", (n.centro - n.centro.round()) * 100.0, nombre_nota(n.centro)) };
                    resp.on_hover_text(format!("{} {:+.0} ¢{}", nombre_nota(n.objetivo), cents, desde));
                }
            });
        });
        if let Some(t) = ir_a {
            let sonando = self.retune.as_ref().is_some_and(|r| r.sonando.is_some() || r.quiere.is_some());
            if sonando {
                self.retune_tocar(t);
            } else if let Some(rt) = &mut self.retune {
                rt.desde = t;
            }
        }
        if refrescar {
            self.retune_refrescar();
        }
        ui.label(RichText::new("Clic en un espacio vacío: escuchar desde ahí · arrastra una nota arriba o abajo (Mayús: sin pegarse a los semitonos) · doble clic: a la nota de la escala · flechas: un semitono. Verde = nota cambiada; el contorno gris es donde estaba.").small().color(DIM));
    }
}
