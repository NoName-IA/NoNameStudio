//! NoName Studio — estudio de producción musical nativo (Windows y Android).
pub mod app;
#[cfg(windows)]
pub mod asociacion;
pub mod audio;
pub mod dsp;
pub mod efectos;
pub mod engine;
pub mod export;
pub mod fx;
pub mod loudness;
pub mod media;
pub mod midi;
#[cfg(windows)]
pub mod plugin_host;
pub mod templates;
pub mod project;

pub use app::{AutoTest, Platform, StudioApp};

pub const ICON_PNG: &[u8] = include_bytes!("../assets/NoName Studio.png");

/// Icono en RGBA al tamaño pedido (fondo blanco, como el PNG original).
pub fn icon_rgba(size: u32) -> (Vec<u8>, u32, u32) {
    let img = image::load_from_memory(ICON_PNG).expect("icono").to_rgba8();
    let img = image::imageops::resize(&img, size, size, image::imageops::FilterType::Lanczos3);
    (img.into_raw(), size, size)
}

#[cfg(not(target_os = "android"))]
/// `archivo`: proyecto con el que se abrió la app (doble clic en un .nns).
pub fn run_desktop(autotest: bool, plugins: Option<String>, archivo: Option<std::path::PathBuf>) -> eframe::Result {
    #[cfg(windows)]
    std::thread::spawn(asociacion::registrar);
    let home = std::env::var("USERPROFILE").map(std::path::PathBuf::from).unwrap_or_else(|_| ".".into());
    let data_dir = std::env::var("APPDATA").map(std::path::PathBuf::from).unwrap_or_else(|_| home.clone()).join("NoName Studio");
    let desktop = home.join("Desktop");
    let export_dir = if desktop.is_dir() { desktop } else { home.join("Music") };
    let (rgba, w, h) = icon_rgba(256);
    // NN_MOVIL=873x393: simula la pantalla de un teléfono en horizontal (para revisar el diseño móvil en la PC)
    let movil: Option<(f32, f32)> = std::env::var("NN_MOVIL").ok().map(|v| {
        let mut it = v.split('x').filter_map(|n| n.trim().parse::<f32>().ok());
        (it.next().unwrap_or(873.0), it.next().unwrap_or(393.0))
    });
    let (tam, minimo) = match movil {
        Some((a, b)) => ([a, b], [a, b]),
        None => ([1280.0, 720.0], [760.0, 480.0]),
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("NoName Studio")
            .with_inner_size(tam)
            .with_maximized(movil.is_none())
            .with_min_inner_size(minimo)
            .with_icon(egui::IconData { rgba, width: w, height: h }),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "NoName Studio",
        options,
        Box::new(move |cc| {
            let mut app = StudioApp::new(cc, Platform { data_dir, export_dir, mobile: movil.is_some() });
            if let Some(f) = archivo {
                app.abrir_al_iniciar(f);
            }
            if autotest {
                app.autotest = Some(AutoTest { frame: 0, log: vec![] });
            }
            if let Some(w) = &plugins {
                app.debug_open(w);
            }
            Ok(Box::new(app))
        }),
    )
}

#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(app: winit::platform::android::activity::AndroidApp) {
    let internal = app.internal_data_path().unwrap_or_else(|| "/data/local/tmp".into());
    let external = app.external_data_path().unwrap_or_else(|| internal.clone());
    // Carpeta pública Música/NoName Studio (Android 11+ permite crear audio ahí sin permisos)
    let public = std::path::PathBuf::from("/storage/emulated/0/Music/NoName Studio");
    let export_dir = if std::fs::create_dir_all(&public).is_ok() && std::fs::write(public.join(".probe"), b"ok").is_ok() {
        let _ = std::fs::remove_file(public.join(".probe"));
        public
    } else {
        external
    };
    audio::ACTIVIDAD.store(app.activity_as_ptr(), std::sync::atomic::Ordering::Relaxed);
    // ¿se abrió tocando un archivo .nns? Se copia a la carpeta de proyectos y se abre desde ahí
    let archivo = archivo_del_intent(&export_dir);
    let options = eframe::NativeOptions { android_app: Some(app), renderer: eframe::Renderer::Glow, ..Default::default() };
    let _ = eframe::run_native(
        "NoName Studio",
        options,
        Box::new(move |cc| {
            let mut app = StudioApp::new(cc, Platform { data_dir: internal, export_dir, mobile: true });
            if let Some(f) = archivo {
                app.abrir_al_iniciar(f);
            }
            Ok(Box::new(app))
        }),
    );
}

/// Lee el archivo con el que Android abrió la app (Intent VIEW con un content://) y lo copia a `dir`.
#[cfg(target_os = "android")]
fn archivo_del_intent(dir: &std::path::Path) -> Option<std::path::PathBuf> {
    use jni::objects::{JByteArray, JObject, JValue};
    fn leer(env: &mut jni::JNIEnv, act: &JObject) -> jni::errors::Result<Option<(String, Vec<u8>)>> {
        let intent = env.call_method(act, "getIntent", "()Landroid/content/Intent;", &[])?.l()?;
        if intent.is_null() {
            return Ok(None);
        }
        let uri = env.call_method(&intent, "getData", "()Landroid/net/Uri;", &[])?.l()?;
        if uri.is_null() {
            return Ok(None);
        }
        let seg = env.call_method(&uri, "getLastPathSegment", "()Ljava/lang/String;", &[])?.l()?;
        let nombre: String = if seg.is_null() { String::new() } else { env.get_string(&seg.into())?.into() };
        let cr = env.call_method(act, "getContentResolver", "()Landroid/content/ContentResolver;", &[])?.l()?;
        let input = env.call_method(&cr, "openInputStream", "(Landroid/net/Uri;)Ljava/io/InputStream;", &[JValue::Object(&uri)])?.l()?;
        let buf: JByteArray = env.new_byte_array(65536)?;
        let mut datos = vec![];
        loop {
            let n = env.call_method(&input, "read", "([B)I", &[JValue::Object(&buf)])?.i()?;
            if n < 0 {
                break;
            }
            let mut tmp = vec![0i8; n as usize];
            env.get_byte_array_region(&buf, 0, &mut tmp)?;
            datos.extend(tmp.iter().map(|&b| b as u8));
            if datos.len() > 200 << 20 {
                break;
            }
        }
        let _ = env.call_method(&input, "close", "()V", &[]);
        Ok(Some((nombre, datos)))
    }
    let ctx = ndk_context::android_context();
    let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }.ok()?;
    let mut env = vm.attach_current_thread().ok()?;
    let act = unsafe { JObject::from_raw(audio::ACTIVIDAD.load(std::sync::atomic::Ordering::Relaxed).cast()) };
    let r = leer(&mut env, &act);
    if r.is_err() {
        let _ = env.exception_clear();
    }
    let (nombre, datos) = r.ok()??;
    // solo proyectos de verdad (con el identificador o el formato viejo)
    let texto = String::from_utf8(datos).ok()?;
    project::de_archivo(&texto)?;
    // nombre limpio: la última parte de la ruta, sin la extensión
    let base = nombre.rsplit(['/', ':']).next().unwrap_or("").trim_end_matches(".json").trim_end_matches(".nns").to_string();
    let base = if base.trim().is_empty() { "Proyecto abierto".to_string() } else { base };
    let _ = std::fs::create_dir_all(dir);
    let mut destino = dir.join(format!("{base}.nns"));
    let mut i = 2;
    // si ya hay un archivo con ese nombre y es otro proyecto, no se pisa
    while destino.exists() && std::fs::read_to_string(&destino).ok().as_deref() != Some(texto.as_str()) {
        destino = dir.join(format!("{base} ({i}).nns"));
        i += 1;
    }
    std::fs::write(&destino, &texto).ok()?;
    Some(destino)
}
