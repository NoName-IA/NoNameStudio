//! Pistas de la playlist como en FL Studio: se elige una pista (clic en su cabecera o en un lugar vacío
//! de su carril) o un clip, y los atajos actúan sobre lo elegido: borrar (Supr), copiar, cortar, pegar,
//! duplicar, renombrar (F2), subir y bajar la pista, elegir la de arriba o abajo, silenciar y solo.
use super::*;

/// Lo elegido en la playlist.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum PlSel {
    Pista(u32),
    /// clip: tipo, índice y dónde estaba (para reconocerlo aunque la lista cambie)
    Clip { kind: Kind, idx: usize, track: u32, start: u32 },
}

/// Una pista copiada entera (nombre, volumen/paneo y todos sus clips).
#[derive(Clone)]
pub(super) struct PistaCopia {
    pub nombre: String,
    pub ajustes: PistaP,
    pub clips: Vec<Clip>,
    pub audio: Vec<AudioClip>,
    pub autos: Vec<AutoClip>,
}

impl StudioApp {
    /// Elegir una pista (sola).
    pub(super) fn elegir_pista(&mut self, t: u32) {
        if (t as usize) < self.p.tracks.len() {
            self.pl_sel = Some(PlSel::Pista(t));
            self.pl_multi.clear();
            self.pl_clips.clear();
        }
    }

    /// Elegir con el teclado apretado: Ctrl suma o quita la pista; Shift elige desde la elegida hasta esta.
    pub(super) fn elegir_pista_con(&mut self, t: u32, ctrl: bool, shift: bool) {
        if t as usize >= self.p.tracks.len() {
            return;
        }
        let actual = match self.pl_sel {
            Some(PlSel::Pista(p)) => Some(p),
            _ => None,
        };
        match (actual, ctrl, shift) {
            (Some(p), _, true) => self.elegir_rango(p, t),
            (Some(p), true, false) => {
                let mut todas = self.pistas_elegidas();
                if let Some(k) = todas.iter().position(|&x| x == t) {
                    todas.remove(k);
                } else {
                    todas.push(t);
                }
                let _ = p;
                match todas.last().copied() {
                    Some(ultima) => {
                        self.pl_sel = Some(PlSel::Pista(ultima));
                        self.pl_multi = todas;
                    }
                    None => {
                        self.pl_sel = None;
                        self.pl_multi.clear();
                    }
                }
            }
            _ => self.elegir_pista(t),
        }
    }

    /// Elige todas las pistas de `a` a `b` (la elegida queda en `b`).
    pub(super) fn elegir_rango(&mut self, a: u32, b: u32) {
        let n = self.p.tracks.len() as u32;
        if n == 0 {
            return;
        }
        let (a, b) = (a.min(n - 1), b.min(n - 1));
        self.pl_multi = (a.min(b)..=a.max(b)).collect();
        self.pl_sel = Some(PlSel::Pista(b));
    }

    /// Todas las pistas elegidas, en orden (vacío si lo elegido es un clip).
    pub(super) fn pistas_elegidas(&self) -> Vec<u32> {
        let Some(PlSel::Pista(p)) = self.pl_sel else { return vec![] };
        let n = self.p.tracks.len() as u32;
        let mut v: Vec<u32> = self.pl_multi.iter().copied().chain(std::iter::once(p)).filter(|&t| t < n).collect();
        v.sort_unstable();
        v.dedup();
        v
    }

    /// «Solo estas»: suenan solo las pistas elegidas. Si ya era así, vuelven a sonar todas.
    pub(super) fn solo_elegidas(&mut self) {
        let elegidas = self.pistas_elegidas();
        if elegidas.is_empty() {
            return;
        }
        let n = self.p.tracks.len();
        let ya = (0..n).all(|t| self.p.pista(t).solo == elegidas.contains(&(t as u32)));
        self.snap();
        for t in 0..n {
            self.p.pista_mut(t).solo = !ya && elegidas.contains(&(t as u32));
        }
        self.cambios = true;
        self.send();
        self.msg = if ya {
            "Vuelven a sonar todas las pistas.".into()
        } else if elegidas.len() == 1 {
            "Solo suena la pista elegida.".into()
        } else {
            format!("Solo suenan las {} pistas elegidas.", elegidas.len())
        };
    }

    /// ¿Ahora suenan exactamente las pistas elegidas (con solo)?
    pub(super) fn solo_de_elegidas(&self) -> bool {
        let elegidas = self.pistas_elegidas();
        !elegidas.is_empty() && (0..self.p.tracks.len()).all(|t| self.p.pista(t).solo == elegidas.contains(&(t as u32)))
    }

    /// Elegir un clip.
    pub(super) fn elegir_clip(&mut self, kind: Kind, idx: usize) {
        if let Some((track, start, _)) = self.lugar_de_clip(kind, idx) {
            self.pl_sel = Some(PlSel::Clip { kind, idx, track, start });
            self.pl_multi.clear();
            self.pl_clips.clear();
        }
    }

    /// El clip elegido, si sigue existiendo (si la lista cambió, se lo busca por su lugar).
    pub(super) fn clip_elegido(&self) -> Option<(Kind, usize)> {
        let Some(PlSel::Clip { kind, idx, track, start }) = self.pl_sel else { return None };
        let en = |i: usize| self.lugar_de_clip(kind, i).is_some_and(|(t, s, _)| t == track && s == start);
        if en(idx) {
            return Some((kind, idx));
        }
        let n = match kind {
            Kind::Pat => self.p.playlist.len(),
            Kind::Audio => self.p.audio_clips.len(),
            Kind::Auto => self.p.autos.len(),
        };
        (0..n).find(|&i| en(i)).map(|i| (kind, i))
    }

    /// La pista elegida (o la del clip elegido).
    pub(super) fn pista_elegida(&self) -> Option<u32> {
        match self.pl_sel? {
            PlSel::Pista(t) => Some(t).filter(|&t| (t as usize) < self.p.tracks.len()),
            PlSel::Clip { .. } => self.clip_elegido().and_then(|(k, i)| self.lugar_de_clip(k, i)).map(|l| l.0),
        }
    }

    fn grabando_pistas(&mut self) -> bool {
        if self.rec_track.is_some() || self.rec.is_some() {
            self.msg = "Termina de grabar antes de cambiar las pistas.".into();
            return true;
        }
        false
    }

    /// Después de cambiar pistas: lo que apuntaba a índices viejos ya no sirve.
    fn pistas_cambiaron(&mut self) {
        self.pl_multi.clear();
        // las filas cambiaron: el cuadro elegido para pegar ya no es el mismo
        self.pl_destino = None;
        self.pl_menu = None;
        self.pl_barra = None;
        self.pl_drag = None;
        self.cambios = true;
        self.send();
    }

    fn copia_de_pista(&self, t: u32) -> PistaCopia {
        PistaCopia {
            nombre: self.p.tracks.get(t as usize).cloned().unwrap_or_default(),
            ajustes: self.p.pistas.get(t as usize).cloned().unwrap_or_default(),
            clips: self.p.playlist.iter().filter(|c| c.track == t).copied().collect(),
            audio: self.p.audio_clips.iter().filter(|c| c.track == t).cloned().collect(),
            autos: self.p.autos.iter().filter(|c| c.track == t).cloned().collect(),
        }
    }

    /// Borra la pista con todos sus clips (las de abajo suben un lugar).
    pub(super) fn borrar_pista(&mut self, t: u32) {
        if self.grabando_pistas() || t as usize >= self.p.tracks.len() {
            return;
        }
        self.snap();
        let nombre = self.quitar_pista(t);
        // queda elegida la que ocupó su lugar (o la última)
        let n = self.p.tracks.len() as u32;
        self.pl_sel = Some(PlSel::Pista(t.min(n - 1)));
        self.pistas_cambiaron();
        self.msg = format!("Pista «{nombre}» borrada (Ctrl+Z la devuelve).");
    }

    /// Borra varias pistas de una vez (un solo Ctrl+Z las devuelve todas).
    pub(super) fn borrar_pistas(&mut self, mut ts: Vec<u32>) {
        if ts.len() <= 1 {
            if let Some(&t) = ts.first() {
                self.borrar_pista(t);
            }
            return;
        }
        if self.grabando_pistas() {
            return;
        }
        self.snap();
        ts.sort_unstable();
        let primera = ts[0];
        // de abajo hacia arriba, así los números de las que faltan no cambian
        for &t in ts.iter().rev() {
            if (t as usize) < self.p.tracks.len() {
                self.quitar_pista(t);
            }
        }
        let n = self.p.tracks.len() as u32;
        self.pl_sel = Some(PlSel::Pista(primera.min(n - 1)));
        self.pistas_cambiaron();
        self.msg = format!("{} pistas borradas (Ctrl+Z las devuelve).", ts.len());
    }

    /// Quita la pista y sus clips (sin guardar para deshacer). Devuelve su nombre.
    fn quitar_pista(&mut self, t: u32) -> String {
        let nombre = self.p.tracks.remove(t as usize);
        if (t as usize) < self.p.pistas.len() {
            self.p.pistas.remove(t as usize);
        }
        self.p.playlist.retain(|c| c.track != t);
        self.p.audio_clips.retain(|c| c.track != t);
        self.p.autos.retain(|c| c.track != t);
        for c in self.p.playlist.iter_mut().filter(|c| c.track > t) {
            c.track -= 1;
        }
        for c in self.p.audio_clips.iter_mut().filter(|c| c.track > t) {
            c.track -= 1;
        }
        for c in self.p.autos.iter_mut().filter(|c| c.track > t) {
            c.track -= 1;
        }
        if self.p.tracks.is_empty() {
            self.ensure_tracks(1);
        }
        nombre
    }

    /// Pone una pista copiada en el lugar `pos` (las de abajo bajan un lugar).
    fn insertar_pista(&mut self, pos: u32, c: PistaCopia) {
        let pos = pos.min(self.p.tracks.len() as u32);
        for k in self.p.playlist.iter_mut().filter(|k| k.track >= pos) {
            k.track += 1;
        }
        for k in self.p.audio_clips.iter_mut().filter(|k| k.track >= pos) {
            k.track += 1;
        }
        for k in self.p.autos.iter_mut().filter(|k| k.track >= pos) {
            k.track += 1;
        }
        self.p.tracks.insert(pos as usize, c.nombre);
        while self.p.pistas.len() < pos as usize {
            self.p.pistas.push(PistaP::default());
        }
        self.p.pistas.insert(pos as usize, c.ajustes);
        for k in c.clips {
            // un patrón que ya no existe no se pega
            if self.p.pattern(k.pat).is_some() {
                self.p.playlist.push(Clip { track: pos, ..k });
            }
        }
        for a in c.audio {
            let id = self.p.new_id();
            self.p.audio_clips.push(AudioClip { id, track: pos, ..a });
        }
        for a in c.autos {
            let id = self.p.new_id();
            self.p.autos.push(AutoClip { id, track: pos, ..a });
        }
        self.pl_sel = Some(PlSel::Pista(pos));
    }

    pub(super) fn copiar_pista(&mut self, t: u32) {
        if (t as usize) < self.p.tracks.len() {
            let c = self.copia_de_pista(t);
            self.msg = format!("Pista «{}» copiada. Ctrl+V la pega debajo de la pista elegida.", c.nombre);
            self.pl_copia = Some(ClipCopia::Pista(Box::new(c)));
            self.copia_sistema = false;
        }
    }

    pub(super) fn cortar_pista(&mut self, t: u32) {
        if self.grabando_pistas() {
            return;
        }
        self.copiar_pista(t);
        self.borrar_pista(t);
        self.msg = "Pista cortada. Ctrl+V la pega debajo de la pista elegida.".into();
    }

    pub(super) fn duplicar_pista(&mut self, t: u32) {
        if self.grabando_pistas() || t as usize >= self.p.tracks.len() {
            return;
        }
        self.snap();
        let mut c = self.copia_de_pista(t);
        c.nombre = format!("{} (copia)", c.nombre);
        let nombre = c.nombre.clone();
        self.insertar_pista(t + 1, c);
        self.pistas_cambiaron();
        self.msg = format!("Pista duplicada: «{nombre}».");
    }

    /// Pega una pista copiada debajo de `t`.
    pub(super) fn pegar_pista_bajo(&mut self, t: u32, c: PistaCopia) {
        self.pegar_pista_en(t + 1, c);
    }

    /// Pega una pista copiada como pista nueva en el lugar `pos` (las de abajo bajan un lugar).
    pub(super) fn pegar_pista_en(&mut self, pos: u32, c: PistaCopia) {
        if self.grabando_pistas() {
            return;
        }
        self.snap();
        let nombre = c.nombre.clone();
        self.insertar_pista(pos, c);
        self.pistas_cambiaron();
        self.msg = format!("Pista «{nombre}» pegada en el lugar {}.", pos + 1);
    }

    /// Cambia la pista `t` de lugar con la de arriba (-1) o la de abajo (+1).
    pub(super) fn mover_pista(&mut self, t: u32, dir: i32) {
        let n = self.p.tracks.len() as i32;
        let o = t as i32 + dir;
        if o < 0 || o >= n || self.grabando_pistas() {
            return;
        }
        let o = o as u32;
        self.snap();
        self.p.tracks.swap(t as usize, o as usize);
        while self.p.pistas.len() <= t.max(o) as usize {
            self.p.pistas.push(PistaP::default());
        }
        self.p.pistas.swap(t as usize, o as usize);
        let cambiar = |tr: &mut u32| {
            if *tr == t {
                *tr = o;
            } else if *tr == o {
                *tr = t;
            }
        };
        self.p.playlist.iter_mut().for_each(|c| cambiar(&mut c.track));
        self.p.audio_clips.iter_mut().for_each(|c| cambiar(&mut c.track));
        self.p.autos.iter_mut().for_each(|c| cambiar(&mut c.track));
        self.pl_sel = Some(PlSel::Pista(o));
        self.pistas_cambiaron();
    }

    /// Atajos de la playlist sobre lo elegido (se llaman con el teclado libre).
    pub(super) fn atajos_playlist(&mut self, ctx: &egui::Context) {
        if self.pl_renombrar.is_some() || self.roll_es_completo() {
            return;
        }
        let t = |this: &Self, id: &str| this.tecla(ctx, id);
        if self.pl_copia.is_some() && !self.copia_sistema {
            ctx.copy_text("NoName Studio · copia de la playlist".into());
            self.copia_sistema = true;
        }
        // pegar sirve aunque no haya nada elegido (en la pista 1, en el cursor)
        if t(self, "pegar") {
            self.pegar_atajo();
        }
        let Some(sel) = self.pl_sel else { return };
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.pl_sel = None;
            self.pl_multi.clear();
            self.pl_clips.clear();
            self.pl_destino = None;
            return;
        }
        // varios clips elegidos: las acciones van sobre todos
        let grupo = self.clips_elegidos();
        if grupo.len() > 1 {
            if t(self, "borrar") {
                self.borrar_clips(&grupo);
            } else if t(self, "copiar") {
                self.copiar_clips(&grupo);
            } else if t(self, "cortar") {
                self.cortar_clips(&grupo);
            } else if t(self, "duplicar") {
                self.duplicar_clips(&grupo);
            }
            return;
        }
        let varias = self.pistas_elegidas();
        let clip = self.clip_elegido();
        let pista = self.pista_elegida();
        if matches!(sel, PlSel::Clip { .. }) && clip.is_none() {
            // el clip elegido ya no existe
            self.pl_sel = pista.map(PlSel::Pista);
            return;
        }
        if t(self, "borrar") {
            match (clip, pista) {
                (Some((k, i)), _) => {
                    self.borrar_clip(k, i);
                    self.pl_sel = pista.map(PlSel::Pista);
                    self.msg = "Clip borrado (Ctrl+Z lo devuelve).".into();
                }
                (None, Some(_)) => self.borrar_pistas(varias.clone()),
                _ => {}
            }
        }
        if t(self, "copiar") {
            match (clip, pista) {
                (Some((k, i)), _) => self.copiar_clip(k, i),
                (None, Some(p)) => self.copiar_pista(p),
                _ => {}
            }
        }
        if t(self, "cortar") {
            match (clip, pista) {
                (Some((k, i)), _) => {
                    self.cortar_clip(k, i);
                    self.pl_sel = pista.map(PlSel::Pista);
                }
                (None, Some(p)) => self.cortar_pista(p),
                _ => {}
            }
        }
        if t(self, "duplicar") {
            match (clip, pista) {
                (Some((k, i)), _) => {
                    self.duplicar_clip(k, i);
                    // queda elegida la copia (la última de su lista), así se puede repetir
                    let n = match k {
                        Kind::Pat => self.p.playlist.len(),
                        Kind::Audio => self.p.audio_clips.len(),
                        Kind::Auto => self.p.autos.len(),
                    };
                    if n > 0 {
                        self.elegir_clip(k, n - 1);
                    }
                    self.msg = "Clip duplicado a la derecha.".into();
                }
                (None, Some(p)) => self.duplicar_pista(p),
                _ => {}
            }
        }
        let Some(p) = pista else { return };
        if t(self, "renombrar") {
            self.pl_renombrar = Some((p, self.p.tracks[p as usize].clone(), true));
        }
        if t(self, "pista_subir") {
            self.mover_pista(p, -1);
        }
        if t(self, "pista_bajar") {
            self.mover_pista(p, 1);
        }
        if t(self, "elegir_arriba") && p > 0 {
            self.elegir_pista(p - 1);
        }
        if t(self, "elegir_abajo") && (p as usize) + 1 < self.p.tracks.len() {
            self.elegir_pista(p + 1);
        }
        if t(self, "solo_pista") {
            self.solo_elegidas();
        }
        if t(self, "silenciar_pista") {
            // todas las elegidas: si alguna suena, se silencian todas; si no, vuelven a sonar
            let lista = if varias.is_empty() { vec![p] } else { varias };
            let silenciar = lista.iter().any(|&x| !self.p.pista(x as usize).mute);
            self.snap();
            for x in lista {
                self.p.pista_mut(x as usize).mute = silenciar;
            }
            self.cambios = true;
            self.send();
        }
    }

    /// Ctrl+V: una pista copiada va debajo de la elegida; un clip, en la pista elegida (después del clip
    /// elegido, o donde está el cursor de reproducción).
    pub(super) fn pegar_atajo(&mut self) {
        let Some(c) = self.pl_copia.clone() else {
            self.msg = "No hay nada copiado.".into();
            return;
        };
        // hay un cuadro elegido: se pega justo ahí, y el cuadro avanza al final de lo pegado
        // (así otro Ctrl+V pega a continuación)
        if let Some((t, s)) = self.pl_destino {
            match c {
                ClipCopia::Pista(pc) => self.pegar_pista_en(t, *pc),
                ClipCopia::Varios(v) => {
                    self.pegar_varios(t, s, &v);
                    self.pl_destino = Some((t, s + v.largo.max(1)));
                }
                _ => {
                    self.pegar_en(t, s);
                    let (k, n) = match self.pl_copia {
                        Some(ClipCopia::Pat(_)) => (Kind::Pat, self.p.playlist.len()),
                        Some(ClipCopia::Audio(_)) => (Kind::Audio, self.p.audio_clips.len()),
                        _ => (Kind::Auto, self.p.autos.len()),
                    };
                    if n > 0 {
                        self.elegir_clip(k, n - 1);
                        if let Some((tr, st, l)) = self.lugar_de_clip(k, n - 1) {
                            self.pl_destino = Some((tr, st + l.max(1)));
                        }
                    }
                }
            }
            return;
        }
        let pista = self.pista_elegida();
        match c {
            ClipCopia::Pista(pc) => {
                let t = pista.unwrap_or(self.p.tracks.len().saturating_sub(1) as u32);
                self.pegar_pista_bajo(t, *pc);
            }
            ClipCopia::Varios(v) => {
                let (t, s) = self.lugar_para_pegar();
                self.pegar_varios(t, s, &v);
            }
            _ => {
                let (track, step) = match self.clip_elegido().and_then(|(k, i)| self.lugar_de_clip(k, i)) {
                    Some((tr, s, l)) => (tr, s + l),
                    None => (pista.unwrap_or(0), self.play_from),
                };
                self.pegar_en(track, step);
                // queda elegido lo pegado
                let (k, n) = match self.pl_copia {
                    Some(ClipCopia::Pat(_)) => (Kind::Pat, self.p.playlist.len()),
                    Some(ClipCopia::Audio(_)) => (Kind::Audio, self.p.audio_clips.len()),
                    _ => (Kind::Auto, self.p.autos.len()),
                };
                if n > 0 {
                    self.elegir_clip(k, n - 1);
                }
            }
        }
    }

    /// Cajita para renombrar la pista (F2 o doble clic en su nombre).
    pub(super) fn ventana_renombrar(&mut self, ctx: &egui::Context) {
        let Some((t, mut texto, primera)) = self.pl_renombrar.clone() else { return };
        if t as usize >= self.p.tracks.len() {
            self.pl_renombrar = None;
            return;
        }
        let mut cerrar = false;
        let mut aceptar = false;
        let pos = self.pl_rect.map(|r| Pos2::new(r.left() + 30.0, self.pl_org.y + t as f32 * TH + 6.0)).unwrap_or(Pos2::new(200.0, 200.0));
        egui::Area::new(egui::Id::new("renombrar_pista")).order(egui::Order::Foreground).fixed_pos(pos).show(ctx, |ui| {
            Frame::new().fill(Color32::from_gray(30)).stroke(Stroke::new(1.0, Color32::from_gray(90))).corner_radius(10).inner_margin(Margin::same(8)).show(ui, |ui| {
                ui.label(RichText::new(format!("Nombre de la pista {}", t + 1)).small().color(DIM));
                let r = ui.add(egui::TextEdit::singleline(&mut texto).desired_width(190.0));
                if primera {
                    r.request_focus();
                    // todo el texto queda marcado para escribir encima
                    if let Some(mut st) = egui::TextEdit::load_state(ctx, r.id) {
                        st.cursor.set_char_range(Some(egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(texto.chars().count()))));
                        st.store(ctx, r.id);
                    }
                }
                if r.lost_focus() {
                    if ui.input(|i| i.key_pressed(Key::Escape)) {
                        cerrar = true;
                    } else {
                        aceptar = true;
                    }
                }
                ui.horizontal(|ui| {
                    if ui.button("Listo").clicked() {
                        aceptar = true;
                    }
                    if ui.button("Cancelar").clicked() {
                        cerrar = true;
                    }
                });
            });
        });
        if aceptar {
            let nuevo = texto.trim().to_string();
            if !nuevo.is_empty() && nuevo != self.p.tracks[t as usize] {
                self.snap();
                self.p.tracks[t as usize] = nuevo;
                self.cambios = true;
                self.send();
            }
            self.pl_renombrar = None;
        } else if cerrar {
            self.pl_renombrar = None;
        } else {
            self.pl_renombrar = Some((t, texto, false));
        }
    }
}
