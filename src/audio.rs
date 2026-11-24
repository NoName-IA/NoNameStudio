//! Salida de audio en tiempo real (WASAPI en Windows, AAudio en Android).
use crate::engine::{Cmd, Engine, Status};
use crate::project::Project;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::mpsc::{channel, Sender};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

pub struct AudioOut {
    _stream: cpal::Stream,
    pub tx: Sender<Cmd>,
    /// plugins retirados por el motor (se destruyen en el hilo de la interfaz)
    pub trash: Option<std::sync::mpsc::Receiver<Box<dyn crate::engine::Proc>>>,
    pub sr: u32,
    pub device: String,
}

/// Nombre que se muestra de un dispositivo. En el teléfono se traduce a algo claro
/// («Micrófono del teléfono», «Audífonos con cable», «Bluetooth: …») y se ignoran los internos raros.
fn nombre(d: &cpal::Device) -> String {
    let Ok(x) = d.description() else { return String::new() };
    #[cfg(target_os = "android")]
    {
        use cpal::{DeviceType as T, InterfaceType as I};
        // Android lo nombra «producto (tipo)»: nos quedamos con el producto
        let prod = x.name().rsplit_once(" (").map(|(a, _)| a).unwrap_or(x.name()).trim().to_string();
        return match (x.interface_type(), x.device_type()) {
            (I::Bluetooth, _) => format!("Bluetooth: {prod}"),
            (I::Usb, _) => format!("USB: {prod}"),
            (_, T::Headset) | (_, T::Headphones) => "Audífonos con cable".into(),
            (I::BuiltIn, T::Microphone) => "Micrófono del teléfono".into(),
            (I::BuiltIn, T::Speaker) => "Altavoz del teléfono".into(),
            _ => String::new(),
        };
    }
    #[allow(unreachable_code)]
    x.name().to_string()
}

/// Lista sin nombres vacíos ni repetidos (el teléfono tiene varios micrófonos internos: se muestra uno).
fn unicos(it: impl Iterator<Item = String>) -> Vec<String> {
    let mut v: Vec<String> = vec![];
    for n in it {
        if !n.is_empty() && !v.contains(&n) {
            v.push(n);
        }
    }
    v
}

/// Nombres de las salidas de audio (parlantes, audífonos…).
pub fn outputs() -> Vec<String> {
    let host = cpal::default_host();
    host.output_devices().map(|it| unicos(it.map(|d| nombre(&d)))).unwrap_or_default()
}
/// Nombres de las entradas de audio (micrófonos).
pub fn inputs() -> Vec<String> {
    let host = cpal::default_host();
    host.input_devices().map(|it| unicos(it.map(|d| nombre(&d)))).unwrap_or_default()
}

/// Nombre del dispositivo elegido por defecto (entrada o salida).
pub const PREDETERMINADO_ENTRADA: &str = "Micrófono predeterminado";
pub const PREDETERMINADO_SALIDA: &str = "Salida predeterminada";

/// `salida` = nombre del dispositivo ("" = el predeterminado); `buffer` = cuadros por bloque (0 = automático).
pub fn start(proj: Project, status: Arc<Status>, salida: &str, buffer: u32) -> Result<AudioOut, String> {
    let host = cpal::default_host();
    let elegido = if salida.is_empty() { None } else { host.output_devices().ok().and_then(|mut it| it.find(|d| nombre(d) == salida)) };
    let dev = match elegido {
        Some(d) => d,
        None => host.default_output_device().ok_or("No hay dispositivo de salida de audio")?,
    };
    let device = dev.description().map(|d| d.name().to_string()).unwrap_or_else(|_| "Salida de audio".into());
    let def = dev.default_output_config().map_err(|e| e.to_string())?;
    let fmt = def.sample_format();
    let mut cfg: cpal::StreamConfig = def.config();
    let sr = cfg.sample_rate;
    let chans = cfg.channels as usize;
    cfg.buffer_size = if buffer > 0 { cpal::BufferSize::Fixed(buffer) } else { cpal::BufferSize::Default };

    let (tx, rx) = channel::<Cmd>();
    struct Filler {
        engine: Engine,
        rx: std::sync::mpsc::Receiver<Cmd>,
        scratch: Vec<f32>,
    }
    impl Filler {
        fn fill(&mut self, frames: usize) -> &[f32] {
            while let Ok(c) = self.rx.try_recv() {
                self.engine.handle(c);
            }
            if self.scratch.len() < frames * 2 {
                self.scratch.resize(frames * 2, 0.0);
            }
            self.engine.render(&mut self.scratch[..frames * 2]);
            &self.scratch[..frames * 2]
        }
    }
    let (ttx, trx) = channel();
    let mut engine = Engine::new(sr as f32, proj, status.clone());
    engine.set_trash(ttx);
    let mut filler = Filler { engine, rx, scratch: vec![0.0; 8192] };
    let caido = status.clone();
    let err = move |e: cpal::Error| {
        eprintln!("audio: {e}");
        if es_grave(&e) {
            caido.audio_caido.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    };

    let stream = match fmt {
        cpal::SampleFormat::F32 => dev.build_output_stream(
            cfg,
            move |data: &mut [f32], _| {
                let frames = data.len() / chans;
                let src = filler.fill(frames);
                for f in 0..frames {
                    for ch in 0..chans {
                        data[f * chans + ch] = if ch < 2 { src[f * 2 + ch] } else { 0.0 };
                    }
                }
            },
            err,
            None,
        ),
        cpal::SampleFormat::I16 => dev.build_output_stream(
            cfg,
            move |data: &mut [i16], _| {
                let frames = data.len() / chans;
                let src = filler.fill(frames);
                for f in 0..frames {
                    for ch in 0..chans {
                        let v = if ch < 2 { src[f * 2 + ch] } else { 0.0 };
                        data[f * chans + ch] = (v.clamp(-1.0, 1.0) * 32767.0) as i16;
                    }
                }
            },
            err,
            None,
        ),
        other => return Err(format!("Formato de audio no soportado: {other:?}")),
    }
    .map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;
    Ok(AudioOut { _stream: stream, tx, trash: Some(trx), sr, device })
}

/// Grabación del micrófono (mono).
pub struct Recorder {
    _stream: cpal::Stream,
    pub buf: Arc<Mutex<Vec<f32>>>,
    pub sr: u32,
}

/// `guardar` = false: solo para escucharse (no acumula lo que entra).
/// `canal`: 0 = mezcla de todos los canales de la entrada; 1, 2… = solo ese canal.
pub fn record(status: Arc<Status>, entrada: &str, guardar: bool, canal: usize) -> Result<Recorder, String> {
    #[cfg(target_os = "android")]
    if !request_mic() {
        return Err("Concede el permiso de micrófono y vuelve a pulsar Grabar.".into());
    }
    let host = cpal::default_host();
    let elegido = if entrada.is_empty() { None } else { host.input_devices().ok().and_then(|mut it| it.find(|d| nombre(d) == entrada)) };
    let dev = match elegido {
        Some(d) => d,
        None => host.default_input_device().ok_or("No hay micrófono disponible")?,
    };
    let def = dev.default_input_config().map_err(|e| e.to_string())?;
    let fmt = def.sample_format();
    let cfg: cpal::StreamConfig = def.config();
    let sr = cfg.sample_rate;
    let chans = cfg.channels as usize;
    let buf = Arc::new(Mutex::new(Vec::<f32>::with_capacity(sr as usize * 30)));
    status.monitor.sr.store(sr, Ordering::Relaxed);
    if let Ok(mut q) = status.monitor.cola.lock() {
        q.clear();
    }
    let caido = status.clone();
    let err = move |e: cpal::Error| {
        eprintln!("micrófono: {e}");
        if es_grave(&e) {
            caido.mic_caido.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    };
    let push = {
        let buf = buf.clone();
        move |frames: &mut dyn Iterator<Item = f32>| {
            let mut pk = 0f32;
            let mut nuevos = 0usize;
            if let Ok(mut b) = buf.lock() {
                if !guardar {
                    b.clear();
                }
                for v in frames {
                    pk = pk.max(v.abs());
                    b.push(v);
                    nuevos += 1;
                }
            }
            let prev = f32::from_bits(status.input.load(Ordering::Relaxed));
            status.input.store(prev.max(pk).to_bits(), Ordering::Relaxed);
            if status.monitor.on.load(Ordering::Relaxed) {
                if let (Ok(b), Ok(mut q)) = (buf.lock(), status.monitor.cola.lock()) {
                    let n = b.len().saturating_sub(nuevos);
                    q.extend(b[n..].iter().copied());
                    let max = 48_000usize;
                    while q.len() > max {
                        q.pop_front();
                    }
                }
            }
        }
    };
    let stream = match fmt {
        cpal::SampleFormat::F32 => {
            let mut push = push;
            dev.build_input_stream(cfg, move |d: &[f32], _| push(&mut d.chunks(chans).map(|f| if canal >= 1 && canal <= f.len() { f[canal - 1] } else { f.iter().sum::<f32>() / chans as f32 })), err, None)
        }
        cpal::SampleFormat::I16 => {
            let mut push = push;
            dev.build_input_stream(cfg, move |d: &[i16], _| push(&mut d.chunks(chans).map(|f| if canal >= 1 && canal <= f.len() { f[canal - 1] as f32 / 32768.0 } else { f.iter().map(|&x| x as f32 / 32768.0).sum::<f32>() / chans as f32 })), err, None)
        }
        other => return Err(format!("Formato de micrófono no soportado: {other:?}")),
    }
    .map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;
    Ok(Recorder { _stream: stream, buf, sr })
}

/// Pide el permiso RECORD_AUDIO en tiempo de ejecución. Devuelve true si ya está concedido.
#[cfg(target_os = "android")]
pub fn request_mic() -> bool {
    request_perm("android.permission.RECORD_AUDIO")
}

/// ¿El error del audio obliga a reconectar? Los microcortes (el equipo no alcanzó a calcular a tiempo),
/// el cambio de salida que el sistema ya resolvió solo (p. ej. conectar audífonos) y el aviso de
/// prioridad no cortan el audio: reconectar por eso paraba la canción y la mandaba al inicio.
fn es_grave(e: &cpal::Error) -> bool {
    use cpal::ErrorKind as K;
    !matches!(e.kind(), K::Xrun | K::DeviceChanged | K::RealtimeDenied)
}

/// La app de Android (para cambiar las banderas de la ventana, como «pantalla siempre encendida»).
#[cfg(target_os = "android")]
pub static APP_ANDROID: std::sync::OnceLock<winit::platform::android::activity::AndroidApp> = std::sync::OnceLock::new();

/// Mantiene la pantalla encendida (mientras suena o graba) o la deja apagarse sola otra vez.
/// Si la pantalla se apaga, Android pausa la app y la canción se corta.
///
/// Se hace en el hilo principal de Java (el dueño de la ventana) con `getWindow().addFlags/clearFlags`.
/// No se usa `AndroidApp::set_window_flags`: toma un bloqueo de escritura que choca con el que el
/// bucle de eventos tiene tomado mientras se dibuja, y la interfaz se congelaba («no responde»).
#[cfg(target_os = "android")]
pub fn pantalla_encendida(si: bool) {
    const FLAG_KEEP_SCREEN_ON: i32 = 0x80;
    let Some(app) = APP_ANDROID.get() else { return };
    app.run_on_java_main_thread(Box::new(move || {
        use jni::objects::JValue;
        let ctx = ndk_context::android_context();
        let Ok(vm) = (unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }) else { return };
        let Ok(mut env) = vm.attach_current_thread() else { return };
        let act = actividad(&ctx);
        let r = (|| -> jni::errors::Result<()> {
            let win = env.call_method(&act, "getWindow", "()Landroid/view/Window;", &[])?.l()?;
            let metodo = if si { "addFlags" } else { "clearFlags" };
            env.call_method(&win, metodo, "(I)V", &[JValue::Int(FLAG_KEEP_SCREEN_ON)])?;
            Ok(())
        })();
        if r.is_err() {
            let _ = env.exception_clear();
        }
    }));
}

/// Vibración corta, la misma que usa Android al mantener apretado algo (no necesita permisos).
#[cfg(target_os = "android")]
pub fn vibrar() {
    const LONG_PRESS: i32 = 0;
    let Some(app) = APP_ANDROID.get() else { return };
    app.run_on_java_main_thread(Box::new(move || {
        use jni::objects::JValue;
        let ctx = ndk_context::android_context();
        let Ok(vm) = (unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }) else { return };
        let Ok(mut env) = vm.attach_current_thread() else { return };
        let act = actividad(&ctx);
        let r = (|| -> jni::errors::Result<()> {
            let win = env.call_method(&act, "getWindow", "()Landroid/view/Window;", &[])?.l()?;
            let vista = env.call_method(&win, "getDecorView", "()Landroid/view/View;", &[])?.l()?;
            env.call_method(&vista, "performHapticFeedback", "(I)Z", &[JValue::Int(LONG_PRESS)])?;
            Ok(())
        })();
        if r.is_err() {
            let _ = env.exception_clear();
        }
    }));
}

/// La actividad de Android (se guarda al iniciar; los permisos se piden sobre ella).
#[cfg(target_os = "android")]
pub static ACTIVIDAD: std::sync::atomic::AtomicPtr<std::ffi::c_void> = std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());

#[cfg(target_os = "android")]
fn actividad(ctx: &ndk_context::AndroidContext) -> jni::objects::JObject<'static> {
    let a = ACTIVIDAD.load(std::sync::atomic::Ordering::Relaxed);
    let p = if a.is_null() { ctx.context() } else { a };
    unsafe { jni::objects::JObject::from_raw(p.cast()) }
}

/// ¿Ya está concedido este permiso?
#[cfg(target_os = "android")]
pub fn permiso_concedido(nombre: &str) -> bool {
    use jni::objects::JValue;
    let ctx = ndk_context::android_context();
    let Ok(vm) = (unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }) else { return false };
    let Ok(mut env) = vm.attach_current_thread() else { return false };
    let act = actividad(&ctx);
    let r = (|| -> jni::errors::Result<bool> {
        let perm = env.new_string(nombre)?;
        Ok(env.call_method(&act, "checkSelfPermission", "(Ljava/lang/String;)I", &[JValue::Object(&perm)])?.i()? == 0)
    })();
    if r.is_err() {
        let _ = env.exception_clear();
    }
    r.unwrap_or(false)
}

/// ¿La pantalla de la app tiene el foco? (false mientras se ve el aviso de permisos del sistema)
#[cfg(target_os = "android")]
pub fn ventana_con_foco() -> bool {
    let ctx = ndk_context::android_context();
    let Ok(vm) = (unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }) else { return false };
    let Ok(mut env) = vm.attach_current_thread() else { return false };
    let act = actividad(&ctx);
    match env.call_method(&act, "hasWindowFocus", "()Z", &[]).and_then(|v| v.z()) {
        Ok(b) => b,
        Err(_) => {
            let _ = env.exception_clear();
            false
        }
    }
}

/// Cierra la app.
#[cfg(target_os = "android")]
pub fn cerrar_app() {
    let ctx = ndk_context::android_context();
    if let Ok(vm) = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) } {
        if let Ok(mut env) = vm.attach_current_thread() {
            let act = actividad(&ctx);
            let _ = env.call_method(&act, "finishAndRemoveTask", "()V", &[]);
            let _ = env.exception_clear();
        }
    }
    std::process::exit(0);
}

/// Pide de una vez los permisos que falten (un solo aviso del sistema).
#[cfg(target_os = "android")]
pub fn pedir_permisos(nombres: &[&str]) -> String {
    use jni::objects::{JObject, JValue};
    fn inner(env: &mut jni::JNIEnv, act: &JObject, nombres: &[&str]) -> jni::errors::Result<()> {
        let mut faltan = vec![];
        for n in nombres {
            let perm = env.new_string(n)?;
            let st = env.call_method(act, "checkSelfPermission", "(Ljava/lang/String;)I", &[JValue::Object(&perm)])?.i()?;
            if st != 0 {
                faltan.push(perm);
            }
        }
        if faltan.is_empty() {
            return Ok(());
        }
        let arr = env.new_object_array(faltan.len() as i32, "java/lang/String", &faltan[0])?;
        for (i, p) in faltan.iter().enumerate() {
            env.set_object_array_element(&arr, i as i32, p)?;
        }
        env.call_method(act, "requestPermissions", "([Ljava/lang/String;I)V", &[JValue::Object(&arr), JValue::Int(8)])?;
        Ok(())
    }
    let ctx = ndk_context::android_context();
    let Ok(vm) = (unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }) else { return "sin VM".into() };
    let Ok(mut env) = vm.attach_current_thread() else { return "sin hilo".into() };
    let act = actividad(&ctx);
    match inner(&mut env, &act, nombres) {
        Ok(()) => "pedidos".into(),
        Err(e) => {
            let _ = env.exception_describe();
            let _ = env.exception_clear();
            format!("error {e}")
        }
    }
}

/// Pide un permiso de Android en tiempo de ejecución. Devuelve true si ya está concedido.
#[cfg(target_os = "android")]
pub fn request_perm(name: &str) -> bool {
    use jni::objects::{JObject, JValue};
    fn inner(env: &mut jni::JNIEnv, act: &JObject, name: &str) -> jni::errors::Result<bool> {
        let perm = env.new_string(name)?;
        let st = env.call_method(act, "checkSelfPermission", "(Ljava/lang/String;)I", &[JValue::Object(&perm)])?.i()?;
        if st == 0 {
            return Ok(true);
        }
        let arr = env.new_object_array(1, "java/lang/String", &perm)?;
        env.call_method(act, "requestPermissions", "([Ljava/lang/String;I)V", &[JValue::Object(&arr), JValue::Int(7)])?;
        Ok(false)
    }
    let ctx = ndk_context::android_context();
    let Ok(vm) = (unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }) else { return false };
    let Ok(mut env) = vm.attach_current_thread() else { return false };
    let act = actividad(&ctx);
    inner(&mut env, &act, name).unwrap_or(false)
}
