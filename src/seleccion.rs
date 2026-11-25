//! Varios clips elegidos a la vez (recuadro de selección o Ctrl+clic): borrar, copiar, cortar, pegar,
//! duplicar y mover todos juntos.
use super::*;

/// Un clip recordado por su tipo, índice y lugar (para reconocerlo aunque la lista cambie).
pub(super) type ClipRef = (Kind, usize, u32, u32);

/// Varios clips copiados: cada uno con su distancia a la primera pista y al primer paso de la copia.
#[derive(Clone)]
pub(super) struct VariosCopia {
    pub items: Vec<(ClipCopia, u32, u32)>,
    /// largo total (en pasos) de lo copiado
    pub largo: u32,
}

impl StudioApp {
    /// Busca un clip recordado: primero en su índice; si la lista cambió, por su lugar.
    pub(super) fn resolver_clip(&self, r: ClipRef) -> Option<(Kind, usize)> {
        let (kind, idx, track, start) = r;
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

    fn referencia(&self, kind: Kind, idx: usize) -> Option<ClipRef> {
        self.lugar_de_clip(kind, idx).map(|(t, s, _)| (kind, idx, t, s))
    }

    /// Todos los clips elegidos (el principal y los demás), sin repetir.
    pub(super) fn clips_elegidos(&self) -> Vec<(Kind, usize)> {
        let mut v: Vec<(Kind, usize)> = vec![];
        if let Some(c) = self.clip_elegido() {
            v.push(c);
        }
        for &r in &self.pl_clips {
            if let Some(c) = self.resolver_clip(r) {
                if !v.contains(&c) {
                    v.push(c);
                }
            }
        }
        v
    }

    /// Deja elegidos exactamente estos clips (el primero es el principal).
    pub(super) fn elegir_clips(&mut self, lista: &[(Kind, usize)]) {
        let refs: Vec<ClipRef> = lista.iter().filter_map(|&(k, i)| self.referencia(k, i)).collect();
        match refs.first() {
            Some(&(kind, idx, track, start)) => {
                self.pl_sel = Some(PlSel::Clip { kind, idx, track, start });
                self.pl_multi.clear();
                self.pl_clips = refs[1..].to_vec();
            }
            None => {
                self.pl_sel = None;
                self.pl_clips.clear();
            }
        }
    }

    /// Ctrl+clic en un clip: lo suma a la selección o lo quita.
    pub(super) fn alternar_clip(&mut self, kind: Kind, idx: usize) {
        let mut v = self.clips_elegidos();
        if let Some(k) = v.iter().position(|&c| c == (kind, idx)) {
            v.remove(k);
        } else {
            v.push((kind, idx));
        }
        self.elegir_clips(&v);
    }

    /// Quita los clips (sin guardar para deshacer): de cada lista, del índice más alto al más bajo.
    fn quitar_clips(&mut self, lista: &[(Kind, usize)]) {
        for kind in [Kind::Pat, Kind::Audio, Kind::Auto] {
            let mut idx: Vec<usize> = lista.iter().filter(|c| c.0 == kind).map(|c| c.1).collect();
            idx.sort_unstable();
            idx.dedup();
            for i in idx.into_iter().rev() {
                match kind {
                    Kind::Pat if i < self.p.playlist.len() => {
                        self.p.playlist.remove(i);
                    }
                    Kind::Audio if i < self.p.audio_clips.len() => {
                        self.p.audio_clips.remove(i);
                    }
                    Kind::Auto if i < self.p.autos.len() => {
                        self.p.autos.remove(i);
                    }
                    _ => {}
                }
            }
        }
    }

    pub(super) fn borrar_clips(&mut self, lista: &[(Kind, usize)]) {
        if lista.is_empty() {
            return;
        }
        self.snap();
        self.quitar_clips(lista);
        self.pl_sel = None;
        self.pl_clips.clear();
        self.pl_menu = None;
        self.pl_barra = None;
        self.cambios = true;
        self.send();
        self.msg = format!("{} clips borrados (Ctrl+Z los devuelve).", lista.len());
    }

    fn copia_de_clips(&self, lista: &[(Kind, usize)]) -> Option<VariosCopia> {
        let lugares: Vec<(Kind, usize, u32, u32, u32)> = lista.iter().filter_map(|&(k, i)| self.lugar_de_clip(k, i).map(|(t, s, l)| (k, i, t, s, l))).collect();
        let t0 = lugares.iter().map(|l| l.2).min()?;
        let s0 = lugares.iter().map(|l| l.3).min()?;
        let fin = lugares.iter().map(|l| l.3 + l.4).max()?;
        let mut items = vec![];
        for (k, i, t, s, _) in lugares {
            let c = match k {
                Kind::Pat => self.p.playlist.get(i).map(|c| ClipCopia::Pat(c.pat)),
                Kind::Audio => self.p.audio_clips.get(i).cloned().map(ClipCopia::Audio),
                Kind::Auto => self.p.autos.get(i).cloned().map(ClipCopia::Auto),
            };
            if let Some(c) = c {
                items.push((c, t - t0, s - s0));
            }
        }
        Some(VariosCopia { items, largo: fin - s0 })
    }

    pub(super) fn copiar_clips(&mut self, lista: &[(Kind, usize)]) {
        if let Some(c) = self.copia_de_clips(lista) {
            self.msg = format!("{} clips copiados. Ctrl+V los pega.", c.items.len());
            self.pl_copia = Some(ClipCopia::Varios(Box::new(c)));
            self.copia_sistema = false;
        }
    }

    pub(super) fn cortar_clips(&mut self, lista: &[(Kind, usize)]) {
        self.copiar_clips(lista);
        self.borrar_clips(lista);
        self.msg = "Clips cortados. Ctrl+V los pega.".into();
    }

    /// Pone los clips copiados desde esa pista y ese paso (respetando cómo estaban entre sí);
    /// los pegados quedan elegidos.
    pub(super) fn pegar_varios(&mut self, track: u32, step: u32, c: &VariosCopia) {
        self.snap();
        let mut nuevos = vec![];
        for (item, dt, ds) in &c.items {
            let (t, s) = (track + dt, step + ds);
            self.ensure_tracks(t + 1);
            match item {
                ClipCopia::Pat(pat) => {
                    if self.p.pattern(*pat).is_some() {
                        self.p.playlist.push(Clip { track: t, bar: s / STEPS_PER_BAR, pat: *pat, off: s % STEPS_PER_BAR });
                        nuevos.push((Kind::Pat, self.p.playlist.len() - 1));
                    }
                }
                ClipCopia::Audio(a) => {
                    let id = self.p.new_id();
                    self.p.audio_clips.push(AudioClip { id, track: t, start: s, ..a.clone() });
                    nuevos.push((Kind::Audio, self.p.audio_clips.len() - 1));
                }
                ClipCopia::Auto(a) => {
                    let id = self.p.new_id();
                    self.p.autos.push(AutoClip { id, track: t, start: s, ..a.clone() });
                    nuevos.push((Kind::Auto, self.p.autos.len() - 1));
                }
                _ => {}
            }
        }
        self.cambios = true;
        self.send();
        self.elegir_clips(&nuevos);
        self.msg = format!("{} clips pegados.", nuevos.len());
    }

    /// Duplica los clips elegidos justo a la derecha de todo el grupo; quedan elegidas las copias.
    pub(super) fn duplicar_clips(&mut self, lista: &[(Kind, usize)]) {
        let Some(c) = self.copia_de_clips(lista) else { return };
        let t0 = lista.iter().filter_map(|&(k, i)| self.lugar_de_clip(k, i)).map(|l| l.0).min().unwrap_or(0);
        let s0 = lista.iter().filter_map(|&(k, i)| self.lugar_de_clip(k, i)).map(|l| l.1).min().unwrap_or(0);
        self.pegar_varios(t0, s0 + c.largo, &c);
        self.msg = format!("{} clips duplicados a la derecha.", c.items.len());
    }

    /// Lugar donde pegar varios: después de lo elegido; si no, en la pista elegida y en el cabezal.
    pub(super) fn lugar_para_pegar(&self) -> (u32, u32) {
        let elegidos = self.clips_elegidos();
        let lugares: Vec<(u32, u32, u32)> = elegidos.iter().filter_map(|&(k, i)| self.lugar_de_clip(k, i)).collect();
        if !lugares.is_empty() {
            let t = lugares.iter().map(|l| l.0).min().unwrap_or(0);
            let fin = lugares.iter().map(|l| l.1 + l.2).max().unwrap_or(0);
            return (t, fin);
        }
        (self.pista_elegida().unwrap_or(0), self.play_from)
    }

    /// Clips que toca un recuadro (en coordenadas de la pantalla).
    pub(super) fn clips_en_recuadro(&self, caja: Rect, org: Pos2, spx: f32) -> Vec<(Kind, usize)> {
        let rect_de = |t: u32, s: u32, l: u32| Rect::from_min_size(Pos2::new(org.x + s as f32 * spx, org.y + t as f32 * TH + 2.0), Vec2::new(l as f32 * spx, TH - 4.0));
        let mut v = vec![];
        for kind in [Kind::Pat, Kind::Audio, Kind::Auto] {
            let n = match kind {
                Kind::Pat => self.p.playlist.len(),
                Kind::Audio => self.p.audio_clips.len(),
                Kind::Auto => self.p.autos.len(),
            };
            for i in 0..n {
                if let Some((t, s, l)) = self.lugar_de_clip(kind, i) {
                    if rect_de(t, s, l).intersects(caja) {
                        v.push((kind, i));
                    }
                }
            }
        }
        v
    }
}
