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

pub fn start(proj: Project, status: Arc<Status>) -> Result<AudioOut, String> {
    let host = cpal::default_host();
    let dev = host.default_output_device().ok_or("No hay dispositivo de salida de audio")?;
    let device = dev.description().map(|d| d.name().to_string()).unwrap_or_else(|_| "Salida de audio".into());
    let def = dev.default_output_config().map_err(|e| e.to_string())?;
    let fmt = def.sample_format();
    let mut cfg: cpal::StreamConfig = def.config();
    let sr = cfg.sample_rate;
    let chans = cfg.channels as usize;
    cfg.buffer_size = cpal::BufferSize::Default;

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
    let mut engine = Engine::new(sr as f32, proj, status);
    engine.set_trash(ttx);
    let mut filler = Filler { engine, rx, scratch: vec![0.0; 8192] };
    let err = |e| eprintln!("audio: {e}");

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

pub fn record(status: Arc<Status>) -> Result<Recorder, String> {
    #[cfg(target_os = "android")]
    if !request_mic() {
        return Err("Concede el permiso de micrófono y vuelve a pulsar Grabar.".into());
    }
    let host = cpal::default_host();
    let dev = host.default_input_device().ok_or("No hay micrófono disponible")?;
    let def = dev.default_input_config().map_err(|e| e.to_string())?;
    let fmt = def.sample_format();
    let cfg: cpal::StreamConfig = def.config();
    let sr = cfg.sample_rate;
    let chans = cfg.channels as usize;
    let buf = Arc::new(Mutex::new(Vec::<f32>::with_capacity(sr as usize * 30)));
    let err = |e| eprintln!("micrófono: {e}");
    let push = {
        let buf = buf.clone();
        move |frames: &mut dyn Iterator<Item = f32>| {
            let mut pk = 0f32;
            if let Ok(mut b) = buf.lock() {
                for v in frames {
                    pk = pk.max(v.abs());
                    b.push(v);
                }
            }
            let prev = f32::from_bits(status.input.load(Ordering::Relaxed));
            status.input.store(prev.max(pk).to_bits(), Ordering::Relaxed);
        }
    };
    let stream = match fmt {
        cpal::SampleFormat::F32 => {
            let mut push = push;
            dev.build_input_stream(cfg, move |d: &[f32], _| push(&mut d.chunks(chans).map(|f| f.iter().sum::<f32>() / chans as f32)), err, None)
        }
        cpal::SampleFormat::I16 => {
            let mut push = push;
            dev.build_input_stream(cfg, move |d: &[i16], _| push(&mut d.chunks(chans).map(|f| f.iter().map(|&x| x as f32 / 32768.0).sum::<f32>() / chans as f32)), err, None)
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
    let act = unsafe { JObject::from_raw(ctx.context().cast()) };
    inner(&mut env, &act, name).unwrap_or(false)
}
