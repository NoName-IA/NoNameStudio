//! Tutorial interactivo: resalta la parte real de la interfaz, oscurece el resto, apunta con una
//! tarjeta y pide hacer la acción (poner golpes, reproducir, escribir notas…) para seguir.
use super::*;

/// Qué tiene que hacer el usuario en cada paso.
#[derive(Clone, Copy, PartialEq)]
enum Accion {
    /// solo leer (Siguiente siempre disponible)
    Leer,
    /// poner golpes en la fila de batería
    Golpes,
    Reproducir,
    AbrirPiano,
    Notas,
    ColocarPatron,
    Cancion,
    AbrirNavegador,
    AbrirMixer,
}

struct Paso {
    /// parte de la interfaz que se resalta ("" = tarjeta centrada)
    zona: &'static str,
    titulo: &'static str,
    texto: &'static str,
    accion: Accion,
    /// qué decir cuando ya lo hizo
    bien: &'static str,
    /// en el teléfono (con los dedos se hace distinto); "" = igual que en la PC
    texto_cel: &'static str,
    bien_cel: &'static str,
}

impl Paso {
    fn texto(&self, movil: bool) -> &'static str {
        if movil && !self.texto_cel.is_empty() { self.texto_cel } else { self.texto }
    }
    fn bien(&self, movil: bool) -> &'static str {
        if movil && !self.bien_cel.is_empty() { self.bien_cel } else { self.bien }
    }
}

const PASOS: [Paso; 12] = [
    Paso {
        zona: "",
        titulo: "Bienvenido a NoName Studio",
        texto: "Vamos a hacer tu primer beat juntos, paso a paso. Te voy a señalar cada parte del programa y tú la vas usando. Toma unos dos minutos.",
        accion: Accion::Leer,
        bien: "",
        texto_cel: "",
        bien_cel: "",
    },
    Paso {
        zona: "kick",
        titulo: "Pon golpes de bombo",
        texto: "Esta es la fila del Kick (el bombo). Cada cuadrito es un pedacito del compás. Haz clic en 3 o 4 cuadritos para poner golpes.",
        accion: Accion::Golpes,
        bien: "¡Eso es un ritmo! Clic derecho pone un golpe suave y arrastrar hacia arriba o abajo cambia su fuerza.",
        texto_cel: "Esta es la fila del Kick (el bombo). Cada cuadrito es un pedacito del compás. Toca 3 o 4 cuadritos para poner golpes.",
        bien_cel: "¡Eso es un ritmo! Mantén apretado un cuadrito para un golpe suave. Arrastrar el dedo desliza la vista.",
    },
    Paso {
        zona: "play",
        titulo: "Escúchalo",
        texto: "Dale a reproducir (o pulsa Espacio). El patrón se repite en bucle mientras lo editas.",
        accion: Accion::Reproducir,
        bien: "Mientras suena puedes seguir agregando golpes en la caja y el hi-hat.",
        texto_cel: "Dale a reproducir. El patrón se repite en bucle mientras lo editas.",
        bien_cel: "",
    },
    Paso {
        zona: "melo",
        titulo: "Abre el Piano Roll",
        texto: "Ahora una melodía. Toca el nombre NN Wave, el sintetizador, para abrir su Piano Roll.",
        accion: Accion::AbrirPiano,
        bien: "Este es el Piano Roll: a la izquierda las teclas, a la derecha el tiempo.",
        texto_cel: "",
        bien_cel: "",
    },
    Paso {
        zona: "roll",
        titulo: "Escribe notas",
        texto: "Haz clic en la cuadrícula para poner notas (al menos 3). Arrastra una nota para moverla o desde su borde derecho para alargarla. Clic derecho la borra y doble clic abre su menú.",
        accion: Accion::Notas,
        bien: "¡Ya tienes melodía! Arriba puedes elegir la escala, poner acordes listos o usar el arpegiador.",
        texto_cel: "Toca la cuadrícula para poner notas (al menos 3). Arrastra una nota para moverla o desde su borde derecho para alargarla. Mantenerla apretada abre su menú (fuerza, largo, borrar…). Arrastrar en el vacío desliza la vista.",
        bien_cel: "",
    },
    Paso {
        zona: "playlist",
        titulo: "Arma la canción",
        texto: "Lo que hiciste quedó guardado en el «Patrón 1». En la Playlist haz doble clic en un carril para colocar el patrón. Coloca varios uno detrás de otro.",
        accion: Accion::ColocarPatron,
        bien: "Cada bloque es tu patrón sonando en ese lugar de la canción. Arrástralos para moverlos; clic derecho para copiar, pegar, duplicar y más. Doble clic en un bloque lo edita.",
        texto_cel: "Lo que hiciste quedó guardado en el «Patrón 1». En la Playlist toca dos veces seguidas un carril para colocar el patrón. Coloca varios uno detrás de otro.",
        bien_cel: "Arrastrar con un dedo mueve la vista. Para mover un bloque, mantenlo apretado hasta que se levante y arrástralo; si lo sueltas sin moverlo, salen sus opciones (copiar, pegar, duplicar…). Pellizca para acercar o alejar.",
    },
    Paso {
        zona: "song",
        titulo: "Escucha la canción completa",
        texto: "PAT toca solo el patrón; SONG toca toda la canción de la Playlist. Pulsa SONG y luego reproducir.",
        accion: Accion::Cancion,
        bien: "Así suena tu canción de principio a fin, y la Playlist la va siguiendo. Enter cambia entre PAT y SONG.",
        texto_cel: "",
        bien_cel: "Así suena tu canción de principio a fin, y la Playlist la va siguiendo.",
    },
    Paso {
        zona: "browser",
        titulo: "Tus propios sonidos",
        texto: "Abre la pestaña Navegador. Ahí aparecen tus audios: un clic los escucha, doble clic los convierte en instrumento y puedes arrastrarlos a la Playlist.",
        accion: Accion::AbrirNavegador,
        bien: "También puedes soltar archivos de audio directamente desde una carpeta de Windows.",
        texto_cel: "Abre la pestaña Navegador. Ahí aparecen tus audios: un toque los escucha, dos toques los convierten en instrumento y puedes arrastrarlos a la Playlist.",
        bien_cel: "",
    },
    Paso {
        zona: "mixer",
        titulo: "El Mixer",
        texto: "Abre el Mixer. Ahí cada canal tiene volumen, paneo, ecualizador (EQ), compresor y efectos.",
        accion: Accion::AbrirMixer,
        bien: "El botón Auto de cada canal dibuja cambios en el tiempo; Master deja tu canción a volumen profesional.",
        texto_cel: "",
        bien_cel: "",
    },
    Paso {
        zona: "",
        titulo: "Toca en vivo con los Pads",
        texto: "En la pestaña Pads (F8) tocas la batería y acordes de la escala en vivo. Elige un kit (Trap 808, Reggaetón, Lo-fi…) y toca varios pads a la vez con el teclado: cada pad muestra su tecla.",
        accion: Accion::Leer,
        bien: "",
        texto_cel: "En la pestaña Pads tocas la batería y acordes de la escala en vivo. Elige un kit (Trap 808, Reggaetón, Lo-fi…) y toca varios pads a la vez con varios dedos.",
        bien_cel: "",
    },
    Paso {
        zona: "export",
        titulo: "Exporta tu canción",
        texto: "Cuando termines, este botón abre las opciones para guardar tu canción: MP3 para compartir, WAV en calidad completa, pistas separadas por canal o las notas en MIDI. Se guarda en tu Escritorio.",
        accion: Accion::Leer,
        bien: "",
        texto_cel: "Cuando termines, este botón abre las opciones para guardar tu canción: MP3 para compartir, WAV en calidad completa, pistas separadas por canal o las notas en MIDI. Se guarda en Música > NoName Studio.",
        bien_cel: "",
    },
    Paso {
        zona: "",
        titulo: "¡Listo para producir!",
        texto: "Ya sabes lo básico. Si quieres partir de una base, ve a Archivo > Nuevo desde plantilla y elige un género. Todos los atajos de teclado se cambian en Ajustes > Teclas. Puedes repetir este tutorial en Archivo > Ver tutorial.",
        accion: Accion::Leer,
        bien: "",
        texto_cel: "Ya sabes lo básico. Si quieres partir de una base, ve a Archivo > Nuevo desde plantilla y elige un género. Puedes repetir este tutorial en Archivo > Ver tutorial.",
        bien_cel: "",
    },
];

impl StudioApp {
    /// Registra dónde está una parte de la interfaz este cuadro (para resaltarla).
    pub(super) fn tut_mark(&mut self, zona: &'static str, r: Rect) {
        if self.tut.is_some() && r.is_positive() {
            self.tut_rects.insert(zona, r);
        }
    }

    fn kick_id(&self) -> Option<u32> {
        self.p.channels.iter().find(|c| c.inst == Inst::Kick).or(self.p.channels.iter().find(|c| c.inst.is_drum())).map(|c| c.id)
    }
    pub(super) fn tut_melo_id(&self) -> Option<u32> {
        self.p.channels.iter().find(|c| c.inst == Inst::Synth).or(self.p.channels.iter().find(|c| c.inst.melodic())).map(|c| c.id)
    }
    pub(super) fn tut_kick_id(&self) -> Option<u32> {
        self.kick_id()
    }
    /// Cantidad que se mide en el paso (golpes, notas, clips) para saber si ya lo hizo.
    fn tut_cuenta(&self, a: Accion) -> usize {
        let pat = self.cur_pat();
        match a {
            Accion::Golpes => self.kick_id().and_then(|k| pat.steps.get(&k)).map(|v| v.iter().filter(|x| **x > 0.0).count()).unwrap_or(0),
            Accion::Notas => self.tut_melo_id().and_then(|m| pat.notes.get(&m)).map(|v| v.len()).unwrap_or(0),
            Accion::ColocarPatron => self.p.playlist.len(),
            _ => 0,
        }
    }
    fn tut_hecho(&self, i: usize) -> bool {
        let a = PASOS[i].accion;
        match a {
            Accion::Leer => true,
            Accion::Golpes => self.tut_cuenta(a) >= self.tut_base + 3,
            Accion::Reproducir => self.playing(),
            Accion::AbrirPiano => self.view == View::Roll,
            Accion::Notas => self.tut_cuenta(a) >= self.tut_base + 3,
            Accion::ColocarPatron => self.tut_cuenta(a) >= self.tut_base + 2,
            Accion::Cancion => self.song_mode && self.playing(),
            Accion::AbrirNavegador => self.left_tab == 1,
            Accion::AbrirMixer => self.view == View::Mixer,
        }
    }

    /// Prepara la pantalla para que la parte del paso esté a la vista.
    pub(super) fn tut_mostrar(&mut self, i: usize) {
        let i = i.min(PASOS.len() - 1);
        self.tut = Some(i);
        self.tut_base = self.tut_cuenta(PASOS[i].accion);
        self.tut_ok = false;
        let compact = self.plat.mobile;
        match PASOS[i].accion {
            Accion::Golpes | Accion::Reproducir => {
                self.view = View::Rack;
                if let Some(k) = self.kick_id() {
                    self.sel_ch = k;
                }
            }
            Accion::AbrirPiano => {
                self.view = View::Rack;
            }
            Accion::Notas => {
                if let Some(m) = self.tut_melo_id() {
                    self.sel_ch = m;
                }
                self.view = View::Roll;
                self.roll_center = true;
            }
            Accion::ColocarPatron | Accion::Cancion => {
                self.show_playlist = true;
                self.left_tab = 0;
                if compact {
                    self.view = View::Playlist;
                }
            }
            Accion::AbrirNavegador => {
                self.show_playlist = true;
                if compact {
                    self.view = View::Playlist;
                }
            }
            Accion::AbrirMixer => {
                if self.view == View::Mixer {
                    self.view = View::Rack;
                }
            }
            Accion::Leer => {}
        }
    }

    fn tut_terminar(&mut self) {
        self.tut = None;
        self.left_tab = 0;
        if !self.settings.tutorial_done {
            self.settings.tutorial_done = true;
            self.save_settings();
        }
        self.msg = "¡A producir! Espacio reproduce · Enter cambia entre patrón y canción.".into();
    }

    pub(super) fn tutorial(&mut self, ctx: &egui::Context) {
        let Some(i) = self.tut else {
            self.tut_rects.clear();
            return;
        };
        let i = i.min(PASOS.len() - 1);
        let paso = &PASOS[i];
        if !self.tut_ok && self.tut_hecho(i) {
            self.tut_ok = true;
        }
        let screen = ctx.content_rect();
        let zona = if paso.zona.is_empty() { None } else { self.tut_rects.get(paso.zona).copied().map(|r| r.expand(6.0).intersect(screen)) };
        let t = ctx.input(|inp| inp.time) as f32;
        ctx.request_repaint_after(Duration::from_millis(33));

        // fondo oscuro con un hueco sobre la zona: lo de afuera no se puede tocar, lo de adentro sí
        let dim = Color32::from_black_alpha(175);
        let tiras: Vec<Rect> = match zona {
            Some(r) => vec![
                Rect::from_min_max(screen.min, Pos2::new(screen.max.x, r.top())),
                Rect::from_min_max(Pos2::new(screen.min.x, r.bottom()), screen.max),
                Rect::from_min_max(Pos2::new(screen.min.x, r.top()), Pos2::new(r.left(), r.bottom())),
                Rect::from_min_max(Pos2::new(r.right(), r.top()), Pos2::new(screen.max.x, r.bottom())),
            ],
            None => vec![screen],
        };
        for (k, tira) in tiras.into_iter().enumerate() {
            if tira.width() < 1.0 || tira.height() < 1.0 {
                continue;
            }
            egui::Area::new(egui::Id::new(("tut_dim", k))).order(egui::Order::Foreground).fixed_pos(tira.min).show(ctx, |ui| {
                let (r, _) = ui.allocate_exact_size(tira.size(), Sense::click_and_drag());
                ui.painter().rect_filled(r, 0, dim);
            });
        }
        // borde que late alrededor de la zona
        let capa = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("tut_borde")));
        if let Some(r) = zona {
            let a = 0.55 + 0.45 * (t * 4.0).sin();
            let col = if self.tut_ok { GREEN } else { ACCENT };
            capa.rect_stroke(r, 6, Stroke::new(2.5, col.gamma_multiply(a)), StrokeKind::Outside);
            capa.rect_stroke(r.expand(4.0 + 3.0 * (t * 4.0).sin().abs()), 8, Stroke::new(1.0, col.gamma_multiply(a * 0.4)), StrokeKind::Outside);
        }

        // tarjeta: debajo de la zona si cabe, si no encima, si no en una esquina
        // en el primer cuadro la pantalla puede medir casi cero: nunca un ancho negativo
        let ancho = (screen.width() - 32.0).min(430.0).max(200.0);
        let alto_aprox = 230.0;
        let (pos, pivote, flecha) = match zona {
            None => (screen.center(), Align2::CENTER_CENTER, None),
            Some(r) => {
                let x = (r.center().x - ancho / 2.0).clamp(screen.left() + 16.0, screen.right() - ancho - 16.0);
                if r.bottom() + 18.0 + alto_aprox < screen.bottom() {
                    (Pos2::new(x, r.bottom() + 18.0), Align2::LEFT_TOP, Some((Pos2::new(r.center().x.clamp(x + 20.0, x + ancho - 20.0), r.bottom() + 18.0), true)))
                } else if r.top() - 18.0 - alto_aprox > screen.top() {
                    (Pos2::new(x, r.top() - 18.0), Align2::LEFT_BOTTOM, Some((Pos2::new(r.center().x.clamp(x + 20.0, x + ancho - 20.0), r.top() - 18.0), false)))
                } else {
                    (Pos2::new(screen.right() - 16.0, screen.bottom() - 40.0), Align2::RIGHT_BOTTOM, None)
                }
            }
        };
        if let (Some((p, abajo)), Some(r)) = (flecha, zona) {
            // triángulo que apunta a la zona
            let punta = if abajo { Pos2::new(p.x, r.bottom() + 4.0) } else { Pos2::new(p.x, r.top() - 4.0) };
            let base_y = if abajo { p.y + 1.0 } else { p.y - 1.0 };
            capa.add(egui::Shape::convex_polygon(vec![punta, Pos2::new(p.x - 10.0, base_y), Pos2::new(p.x + 10.0, base_y)], ACCENT, Stroke::NONE));
        }
        let mut ir: Option<usize> = None;
        let mut fin = false;
        let n = PASOS.len();
        let hecho = self.tut_ok;
        egui::Area::new(egui::Id::new("tut_tarjeta")).order(egui::Order::Tooltip).pivot(pivote).fixed_pos(pos).show(ctx, |ui| {
            Frame::new().fill(PANEL).stroke(Stroke::new(1.5, ACCENT)).corner_radius(10).inner_margin(Margin::same(16)).show(ui, |ui| {
                ui.set_width(ancho);
                if paso.zona.is_empty() && i == 0 {
                    ui.vertical_centered(|ui| {
                        if let Some(tx) = &self.logo {
                            ui.add(egui::Image::new(tx).fit_to_exact_size(Vec2::splat(64.0)));
                        }
                        ui.label(RichText::new("NoName Studio").strong().size(15.0).color(Color32::WHITE));
                    });
                }
                if i > 0 && i + 1 < n {
                    ui.label(RichText::new(format!("PASO {} DE {}", i, n - 2)).small().strong().color(ACCENT));
                }
                ui.label(RichText::new(paso.titulo).strong().size(18.0).color(Color32::WHITE));
                ui.add_space(4.0);
                ui.label(RichText::new(paso.texto(self.plat.mobile)).size(13.5).color(Color32::from_gray(221)));
                if paso.accion != Accion::Leer {
                    ui.add_space(8.0);
                    if hecho {
                        Frame::new().fill(GREEN.gamma_multiply(0.15)).corner_radius(6).inner_margin(Margin::same(8)).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(RichText::new("¡Bien hecho!").strong().color(GREEN));
                            ui.label(RichText::new(paso.bien(self.plat.mobile)).size(13.0).color(Color32::from_rgb(200, 230, 200)));
                        });
                    } else {
                        ui.label(RichText::new("Hazlo en la parte resaltada para continuar.").italics().size(12.5).color(ACCENT2));
                    }
                }
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if i + 1 < n && ui.add(egui::Button::new(RichText::new("Saltar tutorial").color(DIM)).frame(false)).clicked() {
                        fin = true;
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let (txt, ultimo) = match i {
                            0 => ("Empezar", false),
                            x if x + 1 == n => ("Empezar a producir", true),
                            _ => ("Siguiente", false),
                        };
                        let listo = hecho || paso.accion == Accion::Leer;
                        // listo: botón de metal líquido; si falta hacer el paso, gris y apagado
                        let clic = if listo {
                            boton_metal(ui, txt, Vec2::new(if ultimo { 180.0 } else { 140.0 }, 34.0)).clicked()
                        } else {
                            ui.add_enabled(false, egui::Button::new(RichText::new(txt).strong().color(DIM)).min_size(Vec2::new(140.0, 32.0)));
                            false
                        };
                        if clic {
                            if ultimo {
                                fin = true;
                            } else {
                                ir = Some(i + 1);
                            }
                        }
                        if i > 0 && ui.add(egui::Button::new("Atrás").min_size(Vec2::new(80.0, 32.0))).clicked() {
                            ir = Some(i - 1);
                        }
                        if paso.accion != Accion::Leer && !listo && ui.add(egui::Button::new(RichText::new("Omitir paso").size(12.0).color(DIM)).frame(false)).clicked() {
                            ir = Some(i + 1);
                        }
                    });
                });
            });
        });
        if ctx.input(|inp| inp.key_pressed(Key::Escape)) {
            fin = true;
        }
        if let Some(k) = ir {
            self.tut_mostrar(k);
        }
        if fin {
            self.tut_terminar();
        }
        self.tut_rects.clear();
    }
}
