//! NoName Studio — estudio de producción musical nativo (Windows y Android).
pub mod app;
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
pub fn run_desktop(autotest: bool, plugins: Option<String>) -> eframe::Result {
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
    let options = eframe::NativeOptions { android_app: Some(app), renderer: eframe::Renderer::Glow, ..Default::default() };
    let _ = eframe::run_native(
        "NoName Studio",
        options,
        Box::new(move |cc| Ok(Box::new(StudioApp::new(cc, Platform { data_dir: internal, export_dir, mobile: true })))),
    );
}
