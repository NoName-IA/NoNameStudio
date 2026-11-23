//! Ventana de los efectos NN de un canal (o del master): agregar, ordenar, apagar y ajustar.
use super::*;
use crate::efectos::{EfectoP, TipoEfecto};

impl StudioApp {
    fn lista_efectos(&self, ch: u32) -> Vec<EfectoP> {
        if ch == 0 {
            self.p.master_efectos.clone()
        } else {
            self.p.channel(ch).map(|c| c.efectos.clone()).unwrap_or_default()
        }
    }
    fn poner_efectos(&mut self, ch: u32, v: Vec<EfectoP>) {
        if ch == 0 {
            self.p.master_efectos = v;
        } else if let Some(c) = self.p.channel_mut(ch) {
            c.efectos = v;
        }
    }

    pub(super) fn efectos_window(&mut self, ui: &mut Ui, ch: u32) {
        let mut lista = self.lista_efectos(ch);
        let antes = lista.clone();
        let mut foto = false;
        let mut quitar: Option<usize> = None;
        let mut mover: Option<(usize, bool)> = None;
        let bpm = self.p.bpm;
        ui.set_min_width(430.0);
        if lista.is_empty() {
            ui.label(RichText::new("Todavía no hay efectos. Agrega uno con el botón de abajo.").color(DIM));
        }
        let n = lista.len();
        for (i, e) in lista.iter_mut().enumerate() {
            Frame::new().fill(Color32::from_gray(30)).corner_radius(10).inner_margin(Margin::same(10)).show(ui, |ui| {
                ui.horizontal(|ui| {
                    let mut on = e.on;
                    if ui.checkbox(&mut on, RichText::new(e.tipo.nombre()).strong()).on_hover_text(e.tipo.ayuda()).changed() {
                        foto = true;
                        e.on = on;
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.small_button("Quitar").clicked() {
                            quitar = Some(i);
                        }
                        if i + 1 < n && ui.small_button("Bajar").clicked() {
                            mover = Some((i, false));
                        }
                        if i > 0 && ui.small_button("Subir").clicked() {
                            mover = Some((i, true));
                        }
                    });
                });
                ui.horizontal(|ui| {
                    let tipo = e.tipo;
                    let nombres = tipo.perillas();
                    for k in 0..4 {
                        ui.vertical(|ui| {
                            ui.set_width(78.0);
                            let r = knob(ui, &mut e.p[k], 0.0, 1.0, tipo.por_defecto().p[k], nombres[k], |v| tipo.valor(k, v, bpm));
                            if r.drag_started() {
                                foto = true;
                            }
                        });
                    }
                    ui.vertical(|ui| {
                        ui.set_width(78.0);
                        let r = knob(ui, &mut e.mezcla, 0.0, 1.0, tipo.por_defecto().mezcla, "Mezcla", |v| format!("{:.0} %", v * 100.0));
                        if r.drag_started() {
                            foto = true;
                        }
                    });
                });
            });
            ui.add_space(6.0);
        }
        // agregar
        let mut nuevo: Option<TipoEfecto> = None;
        ui.menu_button("+ Agregar efecto", |ui| {
            desplazable(ui, |ui| {
                let mut cat = "";
                for t in TipoEfecto::TODOS {
                    if t.categoria() != cat {
                        cat = t.categoria();
                        ui.label(RichText::new(cat.to_uppercase()).small().color(DIM));
                    }
                    if ui.button(t.nombre()).on_hover_text(t.ayuda()).clicked() {
                        nuevo = Some(t);
                        ui.close();
                    }
                }
            });
        });
        if let Some(i) = quitar {
            foto = true;
            lista.remove(i);
        }
        if let Some((i, arriba)) = mover {
            foto = true;
            let j = if arriba { i - 1 } else { i + 1 };
            lista.swap(i, j);
        }
        if let Some(t) = nuevo {
            foto = true;
            lista.push(t.por_defecto());
        }
        if foto {
            self.snap();
        }
        if lista != antes {
            self.poner_efectos(ch, lista);
            self.cambios = true;
            self.send();
        }
    }
}
