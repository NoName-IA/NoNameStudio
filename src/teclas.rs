//! Atajos de teclado configurables: cada acción tiene una tecla (con Ctrl / Shift / Alt) que se puede
//! cambiar en Ajustes > Teclas. Se guardan solo las que cambiaste; el resto usa la de fábrica.
use super::*;

/// Una combinación de teclas.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) struct Atajo {
    pub key: Key,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

impl Atajo {
    /// Lee «Ctrl+Shift+S», «Space», «F5»… (vacío = sin tecla).
    pub fn leer(s: &str) -> Option<Atajo> {
        let (mut ctrl, mut shift, mut alt) = (false, false, false);
        let mut key = None;
        for parte in s.split('+').map(str::trim).filter(|p| !p.is_empty()) {
            match parte {
                "Ctrl" => ctrl = true,
                "Shift" => shift = true,
                "Alt" => alt = true,
                k => key = Key::from_name(k),
            }
        }
        // «Ctrl++» (la tecla más) queda con la parte vacía al final
        if key.is_none() && s.ends_with("++") {
            key = Some(Key::Plus);
        }
        key.map(|key| Atajo { key, ctrl, shift, alt })
    }

    /// Texto para guardar y mostrar: «Ctrl+Shift+S».
    pub fn texto(&self) -> String {
        let mut t = String::new();
        if self.ctrl {
            t.push_str("Ctrl+");
        }
        if self.shift {
            t.push_str("Shift+");
        }
        if self.alt {
            t.push_str("Alt+");
        }
        t.push_str(self.key.name());
        t
    }

    /// Texto corto para mostrar en pantalla (Espacio, Inicio, «+»…).
    pub fn corto(&self) -> String {
        let k = match self.key {
            Key::Space => "Espacio".to_string(),
            Key::Enter => "Enter".to_string(),
            Key::Home => "Inicio".to_string(),
            Key::Backspace => "Retroceso".to_string(),
            Key::Delete => "Supr".to_string(),
            Key::ArrowUp => "Flecha arriba".to_string(),
            Key::ArrowDown => "Flecha abajo".to_string(),
            Key::ArrowLeft => "Flecha izquierda".to_string(),
            Key::ArrowRight => "Flecha derecha".to_string(),
            k => k.symbol_or_name().to_string(),
        };
        let mut t = String::new();
        if self.ctrl {
            t.push_str("Ctrl+");
        }
        if self.shift {
            t.push_str("Shift+");
        }
        if self.alt {
            t.push_str("Alt+");
        }
        t + &k
    }

    fn mods_iguales(&self, i: &egui::InputState) -> bool {
        i.modifiers.command == self.ctrl && i.modifiers.shift == self.shift && i.modifiers.alt == self.alt
    }

    /// Se apretó en este cuadro (sin contar la repetición automática del teclado).
    pub fn apretado(&self, i: &egui::InputState) -> bool {
        if self.mods_iguales(i) && i.events.iter().any(|e| matches!(e, egui::Event::Key { key, pressed: true, repeat: false, .. } if *key == self.key)) {
            return true;
        }
        // egui no manda Ctrl+C, Ctrl+X ni Ctrl+V como teclas sino como «copiar», «cortar» y «pegar»
        if self.ctrl && !self.shift && !self.alt {
            return i.events.iter().any(|e| match (self.key, e) {
                (Key::C, egui::Event::Copy) | (Key::X, egui::Event::Cut) => true,
                (Key::V, egui::Event::Paste(_)) | (Key::V, egui::Event::PasteImage(_)) => true,
                _ => false,
            });
        }
        false
    }

    /// Está apretada ahora (para mantener).
    pub fn abajo(&self, i: &egui::InputState) -> bool {
        i.key_down(self.key) && i.modifiers.command == self.ctrl && i.modifiers.alt == self.alt
    }

    /// Se soltó en este cuadro.
    pub fn soltado(&self, i: &egui::InputState) -> bool {
        i.key_released(self.key)
    }
}

/// Grupo de cada acción (para ordenar la lista en Ajustes).
pub(super) const GRUPOS: [&str; 5] = ["Reproducción", "Proyecto", "Ventanas", "Playlist", "Pads"];

/// Acciones con atajo: (id, nombre, tecla de fábrica, grupo).
pub(super) const ACCIONES: &[(&str, &str, &str, usize)] = &[
    ("play", "Reproducir / pausar (tocar y soltar)", "Space", 0),
    ("detener", "Detener", "Shift+Space", 0),
    ("grabar", "Grabar", "Ctrl+R", 0),
    ("pat_song", "Cambiar PAT / SONG", "Enter", 0),
    ("metronomo", "Metrónomo", "M", 0),
    ("bucle", "Bucle encendido / apagado", "L", 0),
    ("inicio", "Volver al inicio", "Home", 0),
    ("guardar", "Guardar", "Ctrl+S", 1),
    ("guardar_como", "Guardar como…", "Ctrl+Shift+S", 1),
    ("abrir", "Abrir proyecto…", "Ctrl+O", 1),
    ("nuevo", "Proyecto nuevo", "Ctrl+N", 1),
    ("deshacer", "Deshacer", "Ctrl+Z", 1),
    ("exportar_mp3", "Exportar MP3", "Ctrl+E", 1),
    ("exportar_wav", "Exportar WAV", "Ctrl+Shift+E", 1),
    ("rack", "Channel Rack", "F5", 2),
    ("roll", "Piano Roll", "F6", 2),
    ("mixer", "Mixer", "F7", 2),
    ("pads", "Pads", "F8", 2),
    ("microfono", "Panel del micrófono", "Ctrl+M", 2),
    ("letras", "Letras y notas", "Ctrl+L", 2),
    ("ajustes", "Ajustes", "Ctrl+Comma", 2),
    ("mano", "Mover la vista (mantener y arrastrar)", "Space", 3),
    ("acercar", "Acercar", "Plus", 3),
    ("alejar", "Alejar", "Minus", 3),
    ("borrar", "Borrar la pista o el clip elegido", "Delete", 3),
    ("copiar", "Copiar la pista o el clip elegido", "Ctrl+C", 3),
    ("cortar", "Cortar la pista o el clip elegido", "Ctrl+X", 3),
    ("pegar", "Pegar", "Ctrl+V", 3),
    ("duplicar", "Duplicar la pista o el clip elegido", "Ctrl+D", 3),
    ("renombrar", "Renombrar la pista elegida", "F2", 3),
    ("elegir_arriba", "Elegir la pista de arriba", "Up", 3),
    ("elegir_abajo", "Elegir la pista de abajo", "Down", 3),
    ("pista_subir", "Subir la pista elegida", "Alt+Up", 3),
    ("pista_bajar", "Bajar la pista elegida", "Alt+Down", 3),
    ("silenciar_pista", "Silenciar las pistas elegidas", "Shift+M", 3),
    ("solo_pista", "Que suenen solo las pistas elegidas", "Shift+S", 3),
    ("pad1", "Pad 1", "1", 4),
    ("pad2", "Pad 2", "2", 4),
    ("pad3", "Pad 3", "3", 4),
    ("pad4", "Pad 4", "4", 4),
    ("pad5", "Pad 5", "Q", 4),
    ("pad6", "Pad 6", "W", 4),
    ("pad7", "Pad 7", "E", 4),
    ("pad8", "Pad 8", "R", 4),
    ("pad9", "Pad 9", "A", 4),
    ("pad10", "Pad 10", "S", 4),
    ("pad11", "Pad 11", "D", 4),
    ("pad12", "Pad 12", "F", 4),
    ("pad13", "Pad 13", "Z", 4),
    ("pad14", "Pad 14", "X", 4),
    ("pad15", "Pad 15", "C", 4),
    ("pad16", "Pad 16", "V", 4),
];

/// Ids de los 16 pads, en orden.
pub(super) const PADS_IDS: [&str; 16] = ["pad1", "pad2", "pad3", "pad4", "pad5", "pad6", "pad7", "pad8", "pad9", "pad10", "pad11", "pad12", "pad13", "pad14", "pad15", "pad16"];

/// Pares que pueden compartir tecla a propósito: Espacio tocado = play, Espacio mantenido = mano.
fn pueden_compartir(a: &str, b: &str) -> bool {
    matches!((a, b), ("play", "mano") | ("mano", "play"))
}

/// Los pads solo se tocan con los Pads a la vista y las acciones del resto solo fuera de ellos,
/// así una misma letra puede servir para las dos cosas sin chocar.
fn mismo_lugar(a: &str, b: &str) -> bool {
    a.starts_with("pad") == b.starts_with("pad") || !(a.starts_with("pad") || b.starts_with("pad"))
}

impl StudioApp {
    /// El atajo de una acción (el que pusiste o el de fábrica). None = sin tecla.
    pub(super) fn atajo(&self, id: &str) -> Option<Atajo> {
        match self.settings.teclas.get(id) {
            Some(t) => Atajo::leer(t),
            None => ACCIONES.iter().find(|a| a.0 == id).and_then(|a| Atajo::leer(a.2)),
        }
    }

    /// ¿Se apretó el atajo de esta acción en este cuadro?
    pub(super) fn tecla(&self, ctx: &egui::Context, id: &str) -> bool {
        self.atajo(id).is_some_and(|a| ctx.input(|i| a.apretado(i)))
    }

    /// Ajustes > Teclas: la lista de acciones con su tecla; tocar una para cambiarla.
    pub(super) fn ajustes_teclas(&mut self, ui: &mut Ui) {
        // capturando: la próxima tecla que se apriete queda para esa acción
        if let Some(id) = self.tecla_captura {
            let nueva = ui.input(|i| {
                i.events.iter().find_map(|e| match e {
                    egui::Event::Key { key, pressed: true, repeat: false, modifiers, .. } => Some((*key, *modifiers)),
                    _ => None,
                })
            });
            if let Some((key, m)) = nueva {
                self.tecla_captura = None;
                if key != Key::Escape {
                    let a = Atajo { key, ctrl: m.command, shift: m.shift, alt: m.alt };
                    // si otra acción la usaba, se le quita (salvo las que pueden compartirla)
                    let mut quitadas = vec![];
                    for &(otro, nombre, _, _) in ACCIONES {
                        if otro != id && !pueden_compartir(id, otro) && mismo_lugar(id, otro) && self.atajo(otro) == Some(a) {
                            self.settings.teclas.insert(otro.to_string(), String::new());
                            quitadas.push(nombre);
                        }
                    }
                    self.settings.teclas.insert(id.to_string(), a.texto());
                    self.msg = if quitadas.is_empty() { format!("Tecla {} asignada.", a.corto()) } else { format!("Tecla {} asignada; se quitó de: {}.", a.corto(), quitadas.join(", ")) };
                }
            }
            ui.ctx().request_repaint();
        }
        ui.horizontal(|ui| {
            ui.label(RichText::new("Toca una tecla y aprieta la nueva (con Ctrl, Shift o Alt si quieres). Esc cancela.").small().color(DIM));
        });
        ui.add_space(2.0);
        if pestana_metal(ui, false, "Restablecer las teclas", 150.0).clicked() {
            self.settings.teclas.clear();
            self.tecla_captura = None;
        }
        for (g, grupo) in GRUPOS.iter().enumerate() {
            ui.add_space(10.0);
            ui.label(RichText::new(grupo.to_uppercase()).size(10.5).strong().color(DIM));
            ui.add_space(2.0);
            for &(id, nombre, defecto, gr) in ACCIONES {
                if gr != g {
                    continue;
                }
                // fila tipo tarjeta: nombre a la izquierda, la tecla (dibujada como tecla) a la derecha
                Frame::new().fill(Color32::from_gray(22)).stroke(Stroke::new(1.0, Color32::from_gray(34))).corner_radius(10).inner_margin(Margin::symmetric(10, 5)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(nombre).size(13.0).color(Color32::from_gray(215)));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let cambiada = self.settings.teclas.get(id).is_some_and(|t| t != defecto);
                            if self.atajo(id).is_some() && icon_button(ui, Ic::Close, Vec2::new(24.0, 22.0), false, None).on_hover_text("Sin tecla").clicked() {
                                self.settings.teclas.insert(id.to_string(), String::new());
                            }
                            if cambiada && icon_button(ui, Ic::Undo, Vec2::new(24.0, 22.0), false, None).on_hover_text("Volver a la de fábrica").clicked() {
                                self.settings.teclas.remove(id);
                            }
                            let esperando = self.tecla_captura == Some(id);
                            let txt = if esperando { "aprieta una tecla…".to_string() } else { self.atajo(id).map(|a| a.corto()).unwrap_or_else(|| "sin tecla".into()) };
                            if tecla_dibujada(ui, &txt, esperando, self.atajo(id).is_none()).on_hover_text("Cambiar la tecla").clicked() {
                                self.tecla_captura = if esperando { None } else { Some(id) };
                            }
                        });
                    });
                });
                ui.add_space(3.0);
            }
        }
    }
}

/// Una tecla dibujada como las del teclado (caja oscura con borde plateado, como en los Pads).
/// `esperando`: se ilumina en plata mientras espera la tecla nueva.
fn tecla_dibujada(ui: &mut Ui, txt: &str, esperando: bool, vacia: bool) -> Response {
    let f = FontId::monospace(12.5);
    let g = ui.painter().layout_no_wrap(txt.to_string(), f.clone(), Color32::WHITE);
    let tam = Vec2::new((g.size().x + 20.0).max(46.0), 26.0);
    let (r, resp) = ui.allocate_exact_size(tam, Sense::click());
    let t = ui.ctx().animate_bool_with_time(resp.id, resp.hovered() || esperando, 0.15);
    let p = ui.painter();
    // sombra abajo (relieve de tecla) y la tapa
    p.rect_filled(r.translate(Vec2::new(0.0, 2.0)), 6, Color32::from_black_alpha(140));
    p.rect_filled(r, 6, lerp_col(Color32::from_gray(16), Color32::from_gray(30), t));
    let borde = if esperando {
        // pulso plateado mientras espera
        let pulso = 0.5 + 0.5 * (ui.input(|i| i.time) as f32 * 5.0).sin();
        ui.ctx().request_repaint();
        lerp_col(Color32::from_gray(150), Color32::WHITE, pulso)
    } else {
        lerp_col(Color32::from_gray(110), Color32::from_gray(200), t)
    };
    p.rect_stroke(r, 6, Stroke::new(1.2, borde), StrokeKind::Inside);
    // brillo fino arriba
    p.hline((r.left() + 6.0)..=(r.right() - 6.0), r.top() + 1.5, Stroke::new(1.0, Color32::from_white_alpha(25)));
    let col = if vacia { Color32::from_gray(90) } else { Color32::from_gray(230) };
    p.text(r.center(), Align2::CENTER_CENTER, txt, f, col);
    resp
}

