//! Asocia los proyectos `.nns` con NoName Studio en Windows (solo para el usuario actual, sin permisos
//! de administrador): doble clic abre el archivo en la app, con su icono y el tipo «Proyecto de NoName Studio».
use windows_sys::Win32::System::Registry::*;
use windows_sys::Win32::UI::Shell::{SHChangeNotify, SHCNE_ASSOCCHANGED, SHCNF_IDLIST};

const PROGID: &str = "NoNameStudio.Proyecto";

fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn leer(clave: &str, nombre: Option<&str>) -> Option<String> {
    let (k, n) = (w(clave), nombre.map(w));
    let mut buf = vec![0u16; 2048];
    let mut tam = (buf.len() * 2) as u32;
    let r = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, k.as_ptr(), n.as_ref().map_or(std::ptr::null(), |n| n.as_ptr()), RRF_RT_REG_SZ, std::ptr::null_mut(), buf.as_mut_ptr().cast(), &mut tam)
    };
    (r == 0).then(|| String::from_utf16_lossy(&buf[..(tam as usize / 2).saturating_sub(1)]))
}

fn escribir(clave: &str, nombre: Option<&str>, valor: &str) {
    let (k, n, v) = (w(clave), nombre.map(w), w(valor));
    unsafe {
        RegSetKeyValueW(HKEY_CURRENT_USER, k.as_ptr(), n.as_ref().map_or(std::ptr::null(), |n| n.as_ptr()), REG_SZ, v.as_ptr().cast(), (v.len() * 2) as u32);
    }
}

/// Registra (o corrige) la asociación si falta o apunta a otro .exe. Las copias de desarrollo
/// (dentro de `target\`) no se registran para no robarle la asociación a la app instalada.
pub fn registrar() {
    let Ok(exe) = std::env::current_exe() else { return };
    let exe = exe.to_string_lossy().to_string();
    if exe.to_lowercase().contains("\target\\") {
        return;
    }
    let base = r"Software\Classes";
    let comando = format!("\"{exe}\" \"%1\"");
    // segundo icono del .exe: el de documento (hoja con el diamante y «NNS»), no el del programa
    let icono = format!("\"{exe}\",1");
    let ext = format!(r"{base}\.{}", crate::project::EXTENSION);
    let prog = format!(r"{base}\{PROGID}");
    if leer(&format!(r"{prog}\shell\open\command"), None).as_deref() == Some(comando.as_str())
        && leer(&ext, None).as_deref() == Some(PROGID)
        && leer(&format!(r"{prog}\DefaultIcon"), None).as_deref() == Some(icono.as_str())
    {
        return;
    }
    escribir(&ext, None, PROGID);
    escribir(&ext, Some("Content Type"), "application/x-nonamestudio");
    escribir(&ext, Some("PerceivedType"), "document");
    escribir(&format!(r"{ext}\OpenWithProgids"), Some(PROGID), "");
    escribir(&prog, None, "Proyecto de NoName Studio");
    escribir(&prog, Some("FriendlyTypeName"), "Proyecto de NoName Studio");
    escribir(&format!(r"{prog}\DefaultIcon"), None, &icono);
    escribir(&format!(r"{prog}\shell"), None, "open");
    escribir(&format!(r"{prog}\shell\open"), None, "Abrir con NoName Studio");
    escribir(&format!(r"{prog}\shell\open\command"), None, &comando);
    // «Abrir con…» también la ofrece para estos archivos
    escribir(&format!(r"{base}\Applications\NoNameStudio.exe\SupportedTypes"), Some(".nns"), "");
    escribir(&format!(r"{base}\Applications\NoNameStudio.exe"), Some("FriendlyAppName"), "NoName Studio");
    unsafe { SHChangeNotify(SHCNE_ASSOCCHANGED as i32, SHCNF_IDLIST, std::ptr::null(), std::ptr::null()) };
}
