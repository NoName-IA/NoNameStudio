//! Plugins CLAP en la interfaz: rack de efectos por canal y master, instrumentos externos,
//! ventanas de los plugins y automatización de sus parámetros.
use super::*;

#[cfg(windows)]
use crate::plugin_host::{self as host, PluginInfo, Shared};

#[derive(Default)]
pub(super) struct PlugState {
    #[cfg(windows)]
    pub list: Vec<PluginInfo>,
    #[cfg(windows)]
    pub live: std::collections::HashMap<u32, Arc<Shared>>,
    #[cfg(windows)]
    pub failed: std::collections::HashSet<u32>,
    pub scanned: bool,
}

impl StudioApp {
    #[cfg(windows)]
    fn sr(&self) -> f64 {
        self.audio.as_ref().map(|a| a.sr as f64).unwrap_or(44_100.0)
    }

    /// Todas las ranuras de plugin del proyecto: (canal o 0 = master, ranura).
    fn all_slots(&self) -> Vec<(u32, PluginSlot)> {
        let mut v: Vec<(u32, PluginSlot)> = self.p.master_inserts.iter().map(|s| (0, s.clone())).collect();
        for c in &self.p.channels {
            for s in c.inserts.iter().chain(c.instrument.iter()) {
                v.push((c.id, s.clone()));
            }
        }
        v
    }

    /// Lista de plugins (de la caché o buscando en disco).
    pub(super) fn plugins_list_load(&mut self) {
        #[cfg(windows)]
        if !self.plug.scanned {
            self.plug.scanned = true;
            if let Some(l) = std::fs::read_to_string(self.plat.data_dir.join("plugins.json")).ok().and_then(|s| serde_json::from_str(&s).ok()) {
                self.plug.list = l;
            }
        }
    }

    pub(super) fn rescan_plugins(&mut self) {
        #[cfg(windows)]
        {
            let mut dirs = host::default_dirs();
            dirs.extend(self.settings.plugin_dirs.iter().map(PathBuf::from));
            let (list, errs) = host::scan(&dirs);
            self.msg = if list.is_empty() {
                format!("No se encontraron plugins CLAP en {}. Instala alguno (por ejemplo Surge XT) o agrega su carpeta en Ajustes.", dirs.iter().map(|d| d.display().to_string()).collect::<Vec<_>>().join(", "))
            } else {
                format!("{} plugins CLAP encontrados{}.", list.len(), if errs.is_empty() { String::new() } else { format!(" ({} archivos no se pudieron abrir)", errs.len()) })
            };
            self.plug.list = list;
            self.plug.scanned = true;
            if let Ok(s) = serde_json::to_string_pretty(&self.plug.list) {
                let _ = std::fs::write(self.plat.data_dir.join("plugins.json"), s);
            }
        }
    }

    /// Crea lo que falte, suelta lo que sobra y vuelve a mandar los procesadores de cada canal.
    pub(super) fn plugins_reload_all(&mut self) {
        #[cfg(windows)]
        {
            let slots = self.all_slots();
            let keep: std::collections::HashSet<u32> = slots.iter().map(|s| s.1.uid).collect();
            let gone: Vec<u32> = self.plug.live.keys().copied().filter(|u| !keep.contains(u)).collect();
            for u in gone {
                self.plug.live.remove(&u);
            }
            let mut chans: Vec<u32> = slots.iter().map(|s| s.0).collect();
            chans.extend(self.p.channels.iter().filter(|c| c.inst == Inst::Plugin).map(|c| c.id));
            chans.push(0);
            chans.sort();
            chans.dedup();
            for ch in chans {
                self.plugins_reload_channel(ch);
            }
        }
    }

    /// (Re)envía al motor los plugins de un canal (0 = master).
    pub(super) fn plugins_reload_channel(&mut self, ch: u32) {
        #[cfg(windows)]
        {
            let (ins, inst): (Vec<PluginSlot>, Option<PluginSlot>) = if ch == 0 {
                (self.p.master_inserts.clone(), None)
            } else {
                match self.p.channel(ch) {
                    Some(c) => (c.inserts.clone(), c.instrument.clone()),
                    None => return,
                }
            };
            let mut procs = vec![];
            for s in &ins {
                if let Some(sh) = self.plugin_live(s) {
                    procs.push(sh.make_proc());
                }
            }
            self.cmd(Cmd::Inserts { ch, procs });
            if ch != 0 {
                let p = inst.as_ref().and_then(|s| self.plugin_live(s)).map(|sh| sh.make_proc());
                self.cmd(Cmd::Instrument { ch, proc: p });
            }
        }
        #[cfg(not(windows))]
        let _ = ch;
    }

    #[cfg(windows)]
    fn plugin_live(&mut self, s: &PluginSlot) -> Option<Arc<Shared>> {
        if let Some(sh) = self.plug.live.get(&s.uid) {
            return Some(sh.clone());
        }
        if self.plug.failed.contains(&s.uid) {
            return None;
        }
        match host::instantiate(&s.path, &s.id, s.uid, self.sr(), &s.state) {
            Ok(sh) => {
                self.plug.live.insert(s.uid, sh.clone());
                Some(sh)
            }
            Err(e) => {
                self.plug.failed.insert(s.uid);
                self.msg = format!("Plugin «{}»: {e}", s.name);
                None
            }
        }
    }

    /// Guarda en el proyecto el estado actual de cada plugin (antes de guardar o exportar).
    pub(super) fn plugins_store_state(&mut self) {
        #[cfg(windows)]
        {
            let live = self.plug.live.clone();
            let upd = |s: &mut PluginSlot| {
                if let Some(sh) = live.get(&s.uid) {
                    let st = sh.save_state();
                    if !st.is_empty() {
                        s.state = st;
                    }
                }
            };
            for s in self.p.master_inserts.iter_mut() {
                upd(s);
            }
            for c in self.p.channels.iter_mut() {
                for s in c.inserts.iter_mut() {
                    upd(s);
                }
                if let Some(s) = c.instrument.as_mut() {
                    upd(s);
                }
            }
        }
    }

    pub(super) fn plugins_poll(&mut self) -> bool {
        #[cfg(windows)]
        {
            if let Some(rx) = &self.trash_rx {
                while let Ok(p) = rx.try_recv() {
                    drop(p);
                }
            }
            let mut any = false;
            for sh in self.plug.live.values() {
                sh.poll();
                any |= sh.gui_open();
            }
            return any;
        }
        #[cfg(not(windows))]
        false
    }

    pub(super) fn plugin_param_name(&self, uid: u32, pid: u32) -> String {
        #[cfg(windows)]
        if let Some(p) = self.plug.live.get(&uid).and_then(|sh| sh.params.iter().find(|p| p.id == pid)) {
            return p.name.clone();
        }
        let _ = uid;
        format!("parámetro {pid}")
    }

    pub(super) fn plugin_auto_entries(&mut self, ui: &mut Ui, ch: u32, slots: &[PluginSlot]) {
        #[cfg(windows)]
        for s in slots {
            let Some(sh) = self.plug.live.get(&s.uid).cloned() else { continue };
            if sh.params.is_empty() {
                continue;
            }
            ui.menu_button(s.name.clone(), |ui| {
                egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                    for p in sh.params.iter().take(400) {
                        if ui.button(&p.name).clicked() {
                            ui.close();
                            self.add_auto(AutoTarget::Param(ch, s.uid, p.id));
                        }
                    }
                });
            });
        }
        #[cfg(not(windows))]
        let _ = (ui, ch, slots);
    }

    /// Ventana con los plugins de un canal (0 = master).
    pub(super) fn inserts_window(&mut self, ui: &mut Ui, ch: u32) {
        #[cfg(not(windows))]
        {
            let _ = ch;
            ui.label("Los plugins CLAP están disponibles en la versión de Windows.");
        }
        #[cfg(windows)]
        {
            self.plugins_list_load();
            let (slots, inst, is_plugin_ch) = if ch == 0 {
                (self.p.master_inserts.clone(), None, false)
            } else {
                match self.p.channel(ch) {
                    Some(c) => (c.inserts.clone(), c.instrument.clone(), c.inst == Inst::Plugin),
                    None => return,
                }
            };
            let mut changed = false;
            let mut new_slots = slots.clone();
            let mut new_inst = inst.clone();
            if is_plugin_ch {
                ui.label(RichText::new("INSTRUMENTO").small().color(DIM));
                match &inst {
                    Some(s) => {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&s.name).strong());
                            if ui.button("Abrir interfaz").clicked() {
                                self.open_plugin_gui(s);
                            }
                            if ui.button("Quitar").clicked() {
                                new_inst = None;
                                changed = true;
                            }
                        });
                    }
                    None => {
                        ui.label("Sin instrumento.");
                    }
                }
                ui.menu_button("Elegir instrumento…", |ui| {
                    let list: Vec<PluginInfo> = self.plug.list.iter().filter(|p| p.instrument).cloned().collect();
                    if list.is_empty() {
                        ui.label("No hay instrumentos CLAP. Usa «Buscar plugins».");
                    }
                    egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                        for p in list {
                            if ui.button(format!("{} — {}", p.name, p.vendor)).clicked() {
                                ui.close();
                                let uid = self.p.new_id();
                                new_inst = Some(PluginSlot { path: p.path.clone(), id: p.id.clone(), name: p.name.clone(), on: true, state: String::new(), uid });
                                changed = true;
                            }
                        }
                    });
                });
                ui.separator();
            }
            ui.label(RichText::new("EFECTOS (en orden)").small().color(DIM));
            if slots.is_empty() {
                ui.label(RichText::new("Sin efectos externos.").color(DIM));
            }
            let n = slots.len();
            for (i, s) in slots.iter().enumerate() {
                ui.horizontal(|ui| {
                    let mut on = s.on;
                    if ui.checkbox(&mut on, "").on_hover_text("Encendido").changed() {
                        new_slots[i].on = on;
                        changed = true;
                    }
                    ui.label(RichText::new(&s.name).strong());
                    if self.plug.failed.contains(&s.uid) {
                        ui.label(RichText::new("no cargó").small().color(RED));
                    }
                    if ui.small_button("Interfaz").clicked() {
                        self.open_plugin_gui(s);
                    }
                    if i > 0 && ui.small_button("Subir").clicked() {
                        new_slots.swap(i, i - 1);
                        changed = true;
                    }
                    if i + 1 < n && ui.small_button("Bajar").clicked() {
                        new_slots.swap(i, i + 1);
                        changed = true;
                    }
                    if ui.small_button("Quitar").clicked() {
                        new_slots.retain(|x| x.uid != s.uid);
                        changed = true;
                    }
                });
            }
            ui.horizontal(|ui| {
                ui.menu_button("+ Agregar efecto", |ui| {
                    let list: Vec<PluginInfo> = self.plug.list.iter().filter(|p| !p.instrument).cloned().collect();
                    if list.is_empty() {
                        ui.label("No hay efectos CLAP. Usa «Buscar plugins».");
                    }
                    egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                        for p in list {
                            if ui.button(format!("{} — {}", p.name, p.vendor)).clicked() {
                                ui.close();
                                let uid = self.p.new_id();
                                new_slots.push(PluginSlot { path: p.path.clone(), id: p.id.clone(), name: p.name.clone(), on: true, state: String::new(), uid });
                                changed = true;
                            }
                        }
                    });
                });
                if ui.button("Buscar plugins").clicked() {
                    self.rescan_plugins();
                }
            });
            ui.label(RichText::new(format!("{} plugins CLAP disponibles. Carpetas: Common Files\\CLAP y las de Ajustes.", self.plug.list.len())).small().color(DIM));
            if changed {
                self.snap();
                if ch == 0 {
                    self.p.master_inserts = new_slots;
                } else if let Some(c) = self.p.channel_mut(ch) {
                    c.inserts = new_slots;
                    c.instrument = new_inst;
                }
                self.send();
                self.plugins_reload_all();
            }
        }
    }

    #[cfg(windows)]
    pub(super) fn debug_gui(&mut self, s: &PluginSlot) {
        self.open_plugin_gui(s);
    }

    #[cfg(windows)]
    fn open_plugin_gui(&mut self, s: &PluginSlot) {
        match self.plugin_live(s) {
            Some(sh) => {
                if let Err(e) = sh.open_gui(&s.name) {
                    self.msg = format!("{}: {e}", s.name);
                }
            }
            None => self.msg = format!("El plugin «{}» no está cargado.", s.name),
        }
    }

    /// Prepara los plugins dentro de un hilo de exportación (instancias nuevas con el estado guardado).
    pub(super) fn export_setup(&self) -> impl FnMut(&mut crate::engine::Engine) + Send + 'static {
        let slots = self.all_slots();
        move |e: &mut crate::engine::Engine| {
            #[cfg(windows)]
            {
                let mut by_ch: std::collections::BTreeMap<u32, Vec<Box<dyn crate::engine::Proc>>> = Default::default();
                let sr = e.sr as f64;
                for (ch, s) in &slots {
                    let is_inst = *ch != 0 && e.proj.channel(*ch).and_then(|c| c.instrument.as_ref()).is_some_and(|i| i.uid == s.uid);
                    if let Ok(sh) = host::instantiate(&s.path, &s.id, s.uid, sr, &s.state) {
                        if is_inst {
                            e.set_instrument(*ch, Some(sh.make_proc()));
                        } else {
                            by_ch.entry(*ch).or_default().push(sh.make_proc());
                        }
                    }
                }
                for (ch, v) in by_ch {
                    e.set_inserts(ch, v);
                }
            }
            #[cfg(not(windows))]
            let _ = (&slots, e);
        }
    }
}
