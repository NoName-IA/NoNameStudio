//! Anfitrión de plugins CLAP (Windows): búsqueda, carga, audio, notas, parámetros,
//! estado guardado e interfaz gráfica del plugin en su propia ventana.
#![allow(clippy::missing_safety_doc)]
use crate::engine::{Ev, Proc, BLOCK};
use clap_sys::audio_buffer::clap_audio_buffer;
use clap_sys::entry::clap_plugin_entry;
use clap_sys::events::*;
use clap_sys::ext::audio_ports::*;
use clap_sys::ext::gui::*;
use clap_sys::ext::params::*;
use clap_sys::ext::state::*;
use clap_sys::ext::thread_check::*;
use clap_sys::factory::plugin_factory::*;
use clap_sys::host::clap_host;
use clap_sys::plugin::*;
use clap_sys::process::*;
use clap_sys::stream::*;
use clap_sys::version::CLAP_VERSION;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ffi::{c_char, c_void, CStr, CString};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PluginInfo {
    pub path: String,
    pub id: String,
    pub name: String,
    pub vendor: String,
    pub instrument: bool,
}

#[derive(Clone, Debug)]
pub struct ParamInfo {
    pub id: u32,
    pub name: String,
    pub min: f64,
    pub max: f64,
    pub stepped: bool,
}

// ---------------------------------------------------------------- bibliotecas

struct Lib {
    _lib: libloading::Library,
    entry: *const clap_plugin_entry,
}
unsafe impl Send for Lib {}
unsafe impl Sync for Lib {}

/// Las bibliotecas no se descargan nunca: algunos plugins dejan hilos vivos.
fn libs() -> &'static Mutex<HashMap<String, Arc<Lib>>> {
    static L: OnceLock<Mutex<HashMap<String, Arc<Lib>>>> = OnceLock::new();
    L.get_or_init(|| Mutex::new(HashMap::new()))
}
static MAIN_THREAD: OnceLock<std::thread::ThreadId> = OnceLock::new();

fn cs(p: *const c_char) -> String {
    if p.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(p) }.to_string_lossy().to_string()
    }
}

fn load_lib(path: &str) -> Result<Arc<Lib>, String> {
    MAIN_THREAD.get_or_init(|| std::thread::current().id());
    let mut map = libs().lock().map_err(|_| "bloqueo")?;
    if let Some(l) = map.get(path) {
        return Ok(l.clone());
    }
    unsafe {
        let lib = libloading::Library::new(path).map_err(|e| format!("No se pudo cargar {path}: {e}"))?;
        let entry: *const clap_plugin_entry = {
            let sym: libloading::Symbol<*const clap_plugin_entry> = lib.get(b"clap_entry\0").map_err(|_| "No es un plugin CLAP".to_string())?;
            *sym
        };
        if entry.is_null() {
            return Err("Plugin CLAP inválido".into());
        }
        let cpath = CString::new(path).map_err(|_| "ruta inválida")?;
        if let Some(init) = (*entry).init {
            if !init(cpath.as_ptr()) {
                return Err("El plugin no pudo iniciarse".into());
            }
        }
        let l = Arc::new(Lib { _lib: lib, entry });
        map.insert(path.to_string(), l.clone());
        Ok(l)
    }
}

fn factory(lib: &Lib) -> Option<*const clap_plugin_factory> {
    unsafe {
        let f = (*lib.entry).get_factory?(CLAP_PLUGIN_FACTORY_ID.as_ptr()) as *const clap_plugin_factory;
        (!f.is_null()).then_some(f)
    }
}

/// Carpetas estándar de plugins CLAP en Windows.
pub fn default_dirs() -> Vec<PathBuf> {
    let mut v = vec![];
    for var in ["COMMONPROGRAMFILES", "CommonProgramFiles"] {
        if let Ok(p) = std::env::var(var) {
            v.push(PathBuf::from(p).join("CLAP"));
        }
    }
    if let Ok(p) = std::env::var("LOCALAPPDATA") {
        v.push(PathBuf::from(p).join("Programs").join("Common").join("CLAP"));
    }
    v.sort();
    v.dedup();
    v
}

fn find_clap(dir: &Path, out: &mut Vec<PathBuf>, depth: u32) {
    if depth > 4 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("clap")) && p.is_file() {
            out.push(p);
        } else if p.is_dir() {
            find_clap(&p, out, depth + 1);
        }
    }
}

/// Busca plugins CLAP en las carpetas dadas.
pub fn scan(dirs: &[PathBuf]) -> (Vec<PluginInfo>, Vec<String>) {
    let mut files = vec![];
    for d in dirs {
        find_clap(d, &mut files, 0);
    }
    let mut out = vec![];
    let mut errs = vec![];
    for f in files {
        let path = f.to_string_lossy().to_string();
        match load_lib(&path) {
            Ok(lib) => {
                let Some(fac) = factory(&lib) else { continue };
                unsafe {
                    let n = (*fac).get_plugin_count.map(|g| g(fac)).unwrap_or(0);
                    for i in 0..n {
                        let Some(get) = (*fac).get_plugin_descriptor else { break };
                        let d = get(fac, i);
                        if d.is_null() {
                            continue;
                        }
                        let mut instrument = false;
                        let mut fp = (*d).features;
                        while !fp.is_null() && !(*fp).is_null() {
                            if cs(*fp) == "instrument" {
                                instrument = true;
                            }
                            fp = fp.add(1);
                        }
                        out.push(PluginInfo { path: path.clone(), id: cs((*d).id), name: cs((*d).name), vendor: cs((*d).vendor), instrument });
                    }
                }
            }
            Err(e) => errs.push(e),
        }
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    (out, errs)
}

// ---------------------------------------------------------------- funciones del anfitrión

struct HostData {
    callback: AtomicBool,
    restart: AtomicBool,
    closed: AtomicBool,
    resize: AtomicU64,
}

unsafe fn hd(host: *const clap_host) -> &'static HostData {
    &*((*host).host_data as *const HostData)
}
unsafe extern "C" fn h_get_ext(_h: *const clap_host, id: *const c_char) -> *const c_void {
    let id = CStr::from_ptr(id);
    if id == CLAP_EXT_GUI {
        return &HOST_GUI as *const _ as *const c_void;
    }
    if id == CLAP_EXT_THREAD_CHECK {
        return &HOST_THREAD as *const _ as *const c_void;
    }
    if id == CLAP_EXT_PARAMS {
        return &HOST_PARAMS as *const _ as *const c_void;
    }
    if id == CLAP_EXT_STATE {
        return &HOST_STATE as *const _ as *const c_void;
    }
    std::ptr::null()
}
unsafe extern "C" fn h_restart(h: *const clap_host) {
    hd(h).restart.store(true, Ordering::Relaxed);
}
unsafe extern "C" fn h_process(_h: *const clap_host) {}
unsafe extern "C" fn h_callback(h: *const clap_host) {
    hd(h).callback.store(true, Ordering::Relaxed);
}
unsafe extern "C" fn g_hints(_h: *const clap_host) {}
unsafe extern "C" fn g_resize(h: *const clap_host, w: u32, hh: u32) -> bool {
    hd(h).resize.store(((w as u64) << 32) | hh as u64, Ordering::Relaxed);
    true
}
unsafe extern "C" fn g_show(_h: *const clap_host) -> bool {
    true
}
unsafe extern "C" fn g_hide(_h: *const clap_host) -> bool {
    true
}
unsafe extern "C" fn g_closed(h: *const clap_host, _destroyed: bool) {
    hd(h).closed.store(true, Ordering::Relaxed);
}
unsafe extern "C" fn t_main(_h: *const clap_host) -> bool {
    MAIN_THREAD.get().is_some_and(|m| *m == std::thread::current().id())
}
unsafe extern "C" fn t_audio(_h: *const clap_host) -> bool {
    !t_main(_h)
}
unsafe extern "C" fn p_rescan(_h: *const clap_host, _f: u32) {}
unsafe extern "C" fn p_clear(_h: *const clap_host, _id: u32, _f: u32) {}
unsafe extern "C" fn p_flush(_h: *const clap_host) {}
unsafe extern "C" fn s_dirty(_h: *const clap_host) {}

static HOST_GUI: clap_host_gui = clap_host_gui { resize_hints_changed: Some(g_hints), request_resize: Some(g_resize), request_show: Some(g_show), request_hide: Some(g_hide), closed: Some(g_closed) };
static HOST_THREAD: clap_host_thread_check = clap_host_thread_check { is_main_thread: Some(t_main), is_audio_thread: Some(t_audio) };
static HOST_PARAMS: clap_host_params = clap_host_params { rescan: Some(p_rescan), clear: Some(p_clear), request_flush: Some(p_flush) };
static HOST_STATE: clap_host_state = clap_host_state { mark_dirty: Some(s_dirty) };

// ---------------------------------------------------------------- instancia

/// Una instancia de plugin. Se comparte entre la interfaz (estado, ventana) y el hilo de audio.
pub struct Shared {
    _lib: Arc<Lib>,
    plugin: *const clap_plugin,
    _host: Box<clap_host>,
    hd: Box<HostData>,
    _strings: Vec<CString>,
    pub name: String,
    pub uid: u32,
    pub params: Vec<ParamInfo>,
    in_ports: Vec<u32>,
    out_ports: Vec<u32>,
    in_main: usize,
    out_main: usize,
    gui: Mutex<Option<usize>>,
}
unsafe impl Send for Shared {}
unsafe impl Sync for Shared {}

impl Drop for Shared {
    fn drop(&mut self) {
        self.close_gui();
        unsafe {
            if let Some(d) = (*self.plugin).deactivate {
                d(self.plugin);
            }
            if let Some(d) = (*self.plugin).destroy {
                d(self.plugin);
            }
        }
    }
}

unsafe fn ext<T>(p: *const clap_plugin, id: &CStr) -> Option<&'static T> {
    let f = (*p).get_extension?;
    let e = f(p, id.as_ptr()) as *const T;
    (!e.is_null()).then(|| &*e)
}

unsafe extern "C" fn os_write(s: *const clap_ostream, buf: *const c_void, size: u64) -> i64 {
    let v = &mut *((*s).ctx as *mut Vec<u8>);
    v.extend_from_slice(std::slice::from_raw_parts(buf as *const u8, size as usize));
    size as i64
}
struct Reader {
    data: Vec<u8>,
    pos: usize,
}
unsafe extern "C" fn is_read(s: *const clap_istream, buf: *mut c_void, size: u64) -> i64 {
    let r = &mut *((*s).ctx as *mut Reader);
    let n = (size as usize).min(r.data.len() - r.pos);
    std::ptr::copy_nonoverlapping(r.data.as_ptr().add(r.pos), buf as *mut u8, n);
    r.pos += n;
    n as i64
}

/// Crea, inicia y activa una instancia. Debe llamarse desde el hilo que la va a manejar.
pub fn instantiate(path: &str, id: &str, uid: u32, sr: f64, state: &str) -> Result<Arc<Shared>, String> {
    let lib = load_lib(path)?;
    let fac = factory(&lib).ok_or("El plugin no tiene fábrica")?;
    let strings = vec![CString::new("NoName Studio").unwrap(), CString::new("NoName").unwrap(), CString::new("").unwrap(), CString::new("1.1.0").unwrap()];
    let mut hd = Box::new(HostData { callback: AtomicBool::new(false), restart: AtomicBool::new(false), closed: AtomicBool::new(false), resize: AtomicU64::new(0) });
    let host = Box::new(clap_host {
        clap_version: CLAP_VERSION,
        host_data: &mut *hd as *mut HostData as *mut c_void,
        name: strings[0].as_ptr(),
        vendor: strings[1].as_ptr(),
        url: strings[2].as_ptr(),
        version: strings[3].as_ptr(),
        get_extension: Some(h_get_ext),
        request_restart: Some(h_restart),
        request_process: Some(h_process),
        request_callback: Some(h_callback),
    });
    let cid = CString::new(id).map_err(|_| "id inválido")?;
    unsafe {
        let plugin = (*fac).create_plugin.ok_or("fábrica inválida")?(fac, &*host, cid.as_ptr());
        if plugin.is_null() {
            return Err(format!("No se pudo crear el plugin {id}"));
        }
        if !(*plugin).init.is_some_and(|f| f(plugin)) {
            if let Some(d) = (*plugin).destroy {
                d(plugin);
            }
            return Err("El plugin no pudo iniciarse".into());
        }
        let name = cs((*(*plugin).desc).name);
        // puertos de audio
        let (mut in_ports, mut out_ports, mut in_main, mut out_main) = (vec![], vec![], 0, 0);
        if let Some(ap) = ext::<clap_plugin_audio_ports>(plugin, CLAP_EXT_AUDIO_PORTS) {
            for input in [true, false] {
                let n = ap.count.map(|c| c(plugin, input)).unwrap_or(0).min(8);
                for i in 0..n {
                    let mut info: clap_audio_port_info = std::mem::zeroed();
                    if ap.get.is_some_and(|g| g(plugin, i, input, &mut info)) {
                        let ch = info.channel_count.clamp(1, 8);
                        let main = info.flags & CLAP_AUDIO_PORT_IS_MAIN != 0;
                        if input {
                            if main {
                                in_main = in_ports.len();
                            }
                            in_ports.push(ch);
                        } else {
                            if main {
                                out_main = out_ports.len();
                            }
                            out_ports.push(ch);
                        }
                    }
                }
            }
        } else {
            in_ports.push(2);
            out_ports.push(2);
        }
        // parámetros
        let mut params = vec![];
        if let Some(pe) = ext::<clap_plugin_params>(plugin, CLAP_EXT_PARAMS) {
            let n = pe.count.map(|c| c(plugin)).unwrap_or(0).min(4096);
            for i in 0..n {
                let mut info: clap_param_info = std::mem::zeroed();
                if pe.get_info.is_some_and(|g| g(plugin, i, &mut info)) && info.flags & (CLAP_PARAM_IS_HIDDEN | CLAP_PARAM_IS_READONLY) == 0 {
                    params.push(ParamInfo { id: info.id, name: cs(info.name.as_ptr()), min: info.min_value, max: info.max_value, stepped: info.flags & CLAP_PARAM_IS_STEPPED != 0 });
                }
            }
        }
        let sh = Shared { _lib: lib, plugin, _host: host, hd, _strings: strings, name, uid, params, in_ports, out_ports, in_main, out_main, gui: Mutex::new(None) };
        if !state.is_empty() {
            sh.load_state(state);
        }
        if !(*plugin).activate.is_some_and(|a| a(plugin, sr, 1, BLOCK as u32)) {
            return Err("El plugin no se pudo activar".into());
        }
        Ok(Arc::new(sh))
    }
}

impl Shared {
    pub fn save_state(&self) -> String {
        unsafe {
            let Some(st) = ext::<clap_plugin_state>(self.plugin, CLAP_EXT_STATE) else { return String::new() };
            let mut v: Vec<u8> = vec![];
            let os = clap_ostream { ctx: &mut v as *mut Vec<u8> as *mut c_void, write: Some(os_write) };
            if st.save.is_some_and(|s| s(self.plugin, &os)) {
                b64_encode(&v)
            } else {
                String::new()
            }
        }
    }
    pub fn load_state(&self, s: &str) -> bool {
        unsafe {
            let Some(st) = ext::<clap_plugin_state>(self.plugin, CLAP_EXT_STATE) else { return false };
            let Some(data) = b64_decode(s) else { return false };
            let mut r = Reader { data, pos: 0 };
            let is = clap_istream { ctx: &mut r as *mut Reader as *mut c_void, read: Some(is_read) };
            st.load.is_some_and(|l| l(self.plugin, &is))
        }
    }

    /// Procesador para el hilo de audio.
    pub fn make_proc(self: &Arc<Self>) -> Box<dyn Proc> {
        let mk = |ports: &[u32]| -> (Vec<Vec<f32>>, Vec<*mut f32>, Vec<clap_audio_buffer>) {
            let mut bufs: Vec<Vec<f32>> = vec![];
            for &c in ports {
                for _ in 0..c {
                    bufs.push(vec![0.0; BLOCK]);
                }
            }
            let ptrs: Vec<*mut f32> = bufs.iter_mut().map(|b| b.as_mut_ptr()).collect();
            (bufs, ptrs, vec![])
        };
        let (ib, ip, _) = mk(&self.in_ports);
        let (ob, op, _) = mk(&self.out_ports);
        let mut p = ClapProc { sh: self.clone(), started: false, ibufs: ib, iptrs: ip, obufs: ob, optrs: op, ist: vec![], ost: vec![], notes: Vec::with_capacity(256), pvals: Vec::with_capacity(128), order: Vec::with_capacity(512), steady: 0 };
        // estructuras de buffers (punteros estables porque los Vec ya no cambian de tamaño)
        let mut k = 0;
        for &c in &self.in_ports {
            p.ist.push(clap_audio_buffer { data32: unsafe { p.iptrs.as_mut_ptr().add(k) }, data64: std::ptr::null_mut(), channel_count: c, latency: 0, constant_mask: 0 });
            k += c as usize;
        }
        let mut k = 0;
        for &c in &self.out_ports {
            p.ost.push(clap_audio_buffer { data32: unsafe { p.optrs.as_mut_ptr().add(k) }, data64: std::ptr::null_mut(), channel_count: c, latency: 0, constant_mask: 0 });
            k += c as usize;
        }
        Box::new(p)
    }

    /// Hay que llamarlo seguido desde la interfaz: atiende pedidos del plugin y de su ventana.
    pub fn poll(&self) {
        unsafe {
            if self.hd.callback.swap(false, Ordering::Relaxed) {
                if let Some(f) = (*self.plugin).on_main_thread {
                    f(self.plugin);
                }
            }
        }
        self.hd.restart.store(false, Ordering::Relaxed);
        if self.hd.closed.swap(false, Ordering::Relaxed) {
            self.close_gui();
        }
        let g = self.gui.lock().map(|g| *g).unwrap_or(None);
        let r = self.hd.resize.swap(0, Ordering::Relaxed);
        if let (Some(h), true) = (g, r != 0) {
            win::resize(h, (r >> 32) as u32, r as u32);
        }
        if g.is_some_and(win::was_closed) {
            self.close_gui();
        }
    }

    pub fn gui_open(&self) -> bool {
        self.gui.lock().map(|g| g.is_some()).unwrap_or(false)
    }

    pub fn has_gui(&self) -> bool {
        unsafe { ext::<clap_plugin_gui>(self.plugin, CLAP_EXT_GUI).is_some() }
    }

    /// Abre la interfaz gráfica del plugin en una ventana propia.
    pub fn open_gui(&self, title: &str) -> Result<(), String> {
        if self.gui_open() {
            if let Some(h) = *self.gui.lock().unwrap() {
                win::focus(h);
            }
            return Ok(());
        }
        unsafe {
            let g = ext::<clap_plugin_gui>(self.plugin, CLAP_EXT_GUI).ok_or("Este plugin no tiene interfaz gráfica")?;
            let api = CLAP_WINDOW_API_WIN32.as_ptr();
            if g.is_api_supported.is_some_and(|f| f(self.plugin, api, false)) && g.create.is_some_and(|f| f(self.plugin, api, false)) {
                let (mut w, mut h) = (640u32, 400u32);
                if let Some(f) = g.get_size {
                    f(self.plugin, &mut w, &mut h);
                }
                let resizable = g.can_resize.is_some_and(|f| f(self.plugin));
                let hwnd = win::create(title, w.max(100), h.max(60), resizable)?;
                let cw = clap_window { api, specific: clap_window_handle { win32: hwnd as *mut c_void } };
                if !g.set_parent.is_some_and(|f| f(self.plugin, &cw)) {
                    if let Some(d) = g.destroy {
                        d(self.plugin);
                    }
                    win::destroy(hwnd);
                    return Err("El plugin no pudo mostrar su interfaz".into());
                }
                if let Some(f) = g.show {
                    f(self.plugin);
                }
                win::show(hwnd);
                *self.gui.lock().unwrap() = Some(hwnd);
                return Ok(());
            }
            // interfaz flotante (el plugin crea su propia ventana)
            if g.is_api_supported.is_some_and(|f| f(self.plugin, api, true)) && g.create.is_some_and(|f| f(self.plugin, api, true)) {
                if let Some(f) = g.suggest_title {
                    let t = CString::new(title).unwrap_or_default();
                    f(self.plugin, t.as_ptr());
                }
                if let Some(f) = g.show {
                    f(self.plugin);
                }
                *self.gui.lock().unwrap() = Some(0);
                return Ok(());
            }
            Err("El plugin no ofrece una interfaz compatible".into())
        }
    }

    pub fn close_gui(&self) {
        let Ok(mut g) = self.gui.lock() else { return };
        let Some(h) = g.take() else { return };
        unsafe {
            if let Some(ge) = ext::<clap_plugin_gui>(self.plugin, CLAP_EXT_GUI) {
                if let Some(f) = ge.hide {
                    f(self.plugin);
                }
                if let Some(f) = ge.destroy {
                    f(self.plugin);
                }
            }
        }
        if h != 0 {
            win::destroy(h);
        }
    }
}

// ---------------------------------------------------------------- procesador (hilo de audio)

#[repr(C)]
#[derive(Clone, Copy)]
union AnyEv {
    note: clap_event_note,
    param: clap_event_param_value,
}

struct InEvents {
    ptrs: *const Vec<*const clap_event_header>,
}
unsafe extern "C" fn ie_size(l: *const clap_input_events) -> u32 {
    let e = &*((*l).ctx as *const InEvents);
    let v: &Vec<*const clap_event_header> = &*e.ptrs;
    v.len() as u32
}
unsafe extern "C" fn ie_get(l: *const clap_input_events, i: u32) -> *const clap_event_header {
    let e = &*((*l).ctx as *const InEvents);
    let v: &Vec<*const clap_event_header> = &*e.ptrs;
    v.get(i as usize).copied().unwrap_or(std::ptr::null())
}
unsafe extern "C" fn oe_push(_l: *const clap_output_events, _e: *const clap_event_header) -> bool {
    true
}

pub struct ClapProc {
    sh: Arc<Shared>,
    started: bool,
    ibufs: Vec<Vec<f32>>,
    iptrs: Vec<*mut f32>,
    obufs: Vec<Vec<f32>>,
    optrs: Vec<*mut f32>,
    ist: Vec<clap_audio_buffer>,
    ost: Vec<clap_audio_buffer>,
    notes: Vec<AnyEv>,
    pvals: Vec<(u32, f64)>,
    order: Vec<*const clap_event_header>,
    steady: i64,
}
unsafe impl Send for ClapProc {}

impl Drop for ClapProc {
    fn drop(&mut self) {
        self.stop();
    }
}

impl Proc for ClapProc {
    fn uid(&self) -> u32 {
        self.sh.uid
    }
    fn set_param(&mut self, id: u32, v: f64) {
        if let Some(p) = self.sh.params.iter().find(|p| p.id == id) {
            let mut x = p.min + (p.max - p.min) * v.clamp(0.0, 1.0);
            if p.stepped {
                x = x.round();
            }
            if let Some(e) = self.pvals.iter_mut().find(|e| e.0 == id) {
                e.1 = x;
            } else if self.pvals.len() < self.pvals.capacity() {
                self.pvals.push((id, x));
            }
        }
    }
    fn stop(&mut self) {
        if self.started {
            self.started = false;
            unsafe {
                if let Some(f) = (*self.sh.plugin).stop_processing {
                    f(self.sh.plugin);
                }
            }
        }
    }
    fn process(&mut self, l: &mut [f32], r: &mut [f32], ev: &[Ev]) {
        let n = l.len().min(BLOCK);
        let plugin = self.sh.plugin;
        unsafe {
            if !self.started {
                self.started = (*plugin).start_processing.is_none_or(|f| f(plugin));
                if !self.started {
                    return;
                }
            }
        }
        // entradas: puerto principal con nuestra señal, el resto en silencio
        let mut k = 0;
        for (pi, &c) in self.sh.in_ports.iter().enumerate() {
            for ch in 0..c as usize {
                let b = &mut self.ibufs[k];
                if pi == self.sh.in_main {
                    match (c, ch) {
                        (1, _) => {
                            for i in 0..n {
                                b[i] = (l[i] + r[i]) * 0.5;
                            }
                        }
                        (_, 0) => b[..n].copy_from_slice(&l[..n]),
                        (_, 1) => b[..n].copy_from_slice(&r[..n]),
                        _ => b[..n].fill(0.0),
                    }
                } else {
                    b[..n].fill(0.0);
                }
                k += 1;
            }
        }
        for b in self.obufs.iter_mut() {
            b[..n].fill(0.0);
        }
        // eventos: parámetros al inicio y luego las notas en orden
        self.notes.clear();
        for &(id, v) in &self.pvals {
            self.notes.push(AnyEv {
                param: clap_event_param_value {
                    header: clap_event_header { size: std::mem::size_of::<clap_event_param_value>() as u32, time: 0, space_id: CLAP_CORE_EVENT_SPACE_ID, type_: CLAP_EVENT_PARAM_VALUE, flags: 0 },
                    param_id: id,
                    cookie: std::ptr::null_mut(),
                    note_id: -1,
                    port_index: -1,
                    channel: -1,
                    key: -1,
                    value: v,
                },
            });
        }
        self.pvals.clear();
        for e in ev.iter().take(self.notes.capacity().saturating_sub(self.notes.len())) {
            self.notes.push(AnyEv {
                note: clap_event_note {
                    header: clap_event_header {
                        size: std::mem::size_of::<clap_event_note>() as u32,
                        time: e.at.min(n as u32 - 1),
                        space_id: CLAP_CORE_EVENT_SPACE_ID,
                        type_: if e.on { CLAP_EVENT_NOTE_ON } else { CLAP_EVENT_NOTE_OFF },
                        flags: 0,
                    },
                    note_id: -1,
                    port_index: 0,
                    channel: 0,
                    key: e.note as i16,
                    velocity: e.vel as f64,
                },
            });
        }
        self.order.clear();
        for e in &self.notes {
            self.order.push(e as *const AnyEv as *const clap_event_header);
        }
        let ie_ctx = InEvents { ptrs: &self.order };
        let ie = clap_input_events { ctx: &ie_ctx as *const InEvents as *mut c_void, size: Some(ie_size), get: Some(ie_get) };
        let oe = clap_output_events { ctx: std::ptr::null_mut(), try_push: Some(oe_push) };
        let proc_ = clap_process {
            steady_time: self.steady,
            frames_count: n as u32,
            transport: std::ptr::null(),
            audio_inputs: if self.ist.is_empty() { std::ptr::null() } else { self.ist.as_ptr() },
            audio_outputs: if self.ost.is_empty() { std::ptr::null_mut() } else { self.ost.as_mut_ptr() },
            audio_inputs_count: self.ist.len() as u32,
            audio_outputs_count: self.ost.len() as u32,
            in_events: &ie,
            out_events: &oe,
        };
        unsafe {
            if let Some(p) = (*plugin).process {
                p(plugin, &proc_);
            }
        }
        self.steady += n as i64;
        // salida del puerto principal
        if self.sh.out_ports.is_empty() {
            return;
        }
        let base: usize = self.sh.out_ports[..self.sh.out_main].iter().map(|c| *c as usize).sum();
        let c = self.sh.out_ports[self.sh.out_main];
        if c == 1 {
            let b = &self.obufs[base];
            l[..n].copy_from_slice(&b[..n]);
            r[..n].copy_from_slice(&b[..n]);
        } else {
            l[..n].copy_from_slice(&self.obufs[base][..n]);
            r[..n].copy_from_slice(&self.obufs[base + 1][..n]);
        }
    }
}

// ---------------------------------------------------------------- ventana Win32

mod win {
    use std::sync::atomic::{AtomicBool, Ordering};
    use windows_sys::Win32::Foundation::*;
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    static REGISTERED: AtomicBool = AtomicBool::new(false);
    const CLASS: &str = "NoNameStudioPlugin";
    const CLOSED_PROP: isize = 0x4e4e5300;

    fn w(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    unsafe extern "system" fn proc_(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        if msg == WM_CLOSE {
            // la ventana se destruye desde la interfaz, después de soltar el plugin
            SetWindowLongPtrW(h, GWLP_USERDATA, CLOSED_PROP);
            ShowWindow(h, SW_HIDE);
            return 0;
        }
        DefWindowProcW(h, msg, wp, lp)
    }

    pub fn create(title: &str, cw: u32, ch: u32, resizable: bool) -> Result<usize, String> {
        unsafe {
            let inst = GetModuleHandleW(std::ptr::null());
            let class = w(CLASS);
            if !REGISTERED.swap(true, Ordering::SeqCst) {
                let wc = WNDCLASSW {
                    style: CS_HREDRAW | CS_VREDRAW,
                    lpfnWndProc: Some(proc_),
                    cbClsExtra: 0,
                    cbWndExtra: 0,
                    hInstance: inst,
                    hIcon: std::ptr::null_mut(),
                    hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
                    hbrBackground: std::ptr::null_mut(),
                    lpszMenuName: std::ptr::null(),
                    lpszClassName: class.as_ptr(),
                };
                RegisterClassW(&wc);
            }
            let style = if resizable { WS_OVERLAPPEDWINDOW & !WS_MAXIMIZEBOX } else { WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX };
            let mut r = RECT { left: 0, top: 0, right: cw as i32, bottom: ch as i32 };
            AdjustWindowRectEx(&mut r, style, 0, WS_EX_TOOLWINDOW);
            let t = w(title);
            let h = CreateWindowExW(WS_EX_TOOLWINDOW, class.as_ptr(), t.as_ptr(), style | WS_CLIPCHILDREN, CW_USEDEFAULT, CW_USEDEFAULT, r.right - r.left, r.bottom - r.top, std::ptr::null_mut(), std::ptr::null_mut(), inst, std::ptr::null());
            if h.is_null() {
                return Err("No se pudo crear la ventana del plugin".into());
            }
            SetWindowLongPtrW(h, GWLP_USERDATA, 0);
            SetWindowPos(h, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);
            Ok(h as usize)
        }
    }
    pub fn show(h: usize) {
        unsafe {
            ShowWindow(h as HWND, SW_SHOW);
        }
    }
    pub fn focus(h: usize) {
        if h != 0 {
            unsafe {
                ShowWindow(h as HWND, SW_SHOW);
                SetForegroundWindow(h as HWND);
            }
        }
    }
    pub fn destroy(h: usize) {
        unsafe {
            DestroyWindow(h as HWND);
        }
    }
    pub fn was_closed(h: usize) -> bool {
        h != 0 && unsafe { GetWindowLongPtrW(h as HWND, GWLP_USERDATA) } == CLOSED_PROP
    }
    pub fn resize(h: usize, cw: u32, ch: u32) {
        if h == 0 {
            return;
        }
        unsafe {
            let style = GetWindowLongW(h as HWND, GWL_STYLE) as u32;
            let mut r = RECT { left: 0, top: 0, right: cw as i32, bottom: ch as i32 };
            AdjustWindowRectEx(&mut r, style, 0, WS_EX_TOOLWINDOW);
            SetWindowPos(h as HWND, std::ptr::null_mut(), 0, 0, r.right - r.left, r.bottom - r.top, SWP_NOMOVE | SWP_NOZORDER);
        }
    }
}

// ---------------------------------------------------------------- base64

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn b64_encode(d: &[u8]) -> String {
    let mut s = String::with_capacity(d.len().div_ceil(3) * 4);
    for c in d.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        s.push(B64[(n >> 18) as usize & 63] as char);
        s.push(B64[(n >> 12) as usize & 63] as char);
        s.push(if c.len() > 1 { B64[(n >> 6) as usize & 63] as char } else { '=' });
        s.push(if c.len() > 2 { B64[n as usize & 63] as char } else { '=' });
    }
    s
}

pub fn b64_decode(s: &str) -> Option<Vec<u8>> {
    let mut v = Vec::with_capacity(s.len() / 4 * 3);
    let val = |c: u8| -> Option<u32> { B64.iter().position(|&x| x == c).map(|p| p as u32) };
    let b: Vec<u8> = s.bytes().filter(|c| !c.is_ascii_whitespace()).collect();
    for c in b.chunks(4) {
        if c.len() < 4 {
            return None;
        }
        let a = val(c[0])?;
        let bb = val(c[1])?;
        let cc = if c[2] == b'=' { 0 } else { val(c[2])? };
        let dd = if c[3] == b'=' { 0 } else { val(c[3])? };
        let n = a << 18 | bb << 12 | cc << 6 | dd;
        v.push((n >> 16) as u8);
        if c[2] != b'=' {
            v.push((n >> 8) as u8);
        }
        if c[3] != b'=' {
            v.push(n as u8);
        }
    }
    Some(v)
}
