//! Exportación a MP3 (codificador Rust puro), WAV y stems (una pista por canal).
use crate::engine::{render_song_with, Bank, Engine};
use crate::project::Project;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

pub const SR: u32 = 44_100;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Format {
    Mp3,
    Wav,
    /// una pista WAV por canal, sin la cadena del master
    Stems,
}

/// Resultado de una exportación.
pub struct Done {
    pub path: PathBuf,
    pub secs: f32,
    pub peak_db: f32,
    pub lufs: f32,
    pub files: usize,
}

/// Opciones de exportación (vienen de Ajustes).
#[derive(Clone, Copy, Debug)]
pub struct Opts {
    /// frecuencia de muestreo (44100 o 48000)
    pub sr: u32,
    /// calidad del MP3 en kbps
    pub kbps: u32,
    /// bits del WAV (16 o 24)
    pub bits: u16,
    /// 0 = automático (normaliza solo sin limitador), 1 = siempre, 2 = nunca
    pub norm: u8,
}
impl Default for Opts {
    fn default() -> Self {
        Opts { sr: SR, kbps: 320, bits: 16, norm: 0 }
    }
}

/// Preparación del motor para exportar (por ejemplo, cargar plugins en este hilo).
pub type Setup<'a> = &'a mut dyn FnMut(&mut Engine);

/// Renderiza y escribe la canción (MP3/WAV) o los stems.
pub fn export_full(proj: &Project, bank: &Bank, dir: &Path, fmt: Format, opts: &Opts, progress: &AtomicU32, cancel: &AtomicBool, setup: Setup) -> Result<Done, String> {
    let sr = if opts.sr == 48_000 { 48_000 } else { SR };
    if proj.song_bars() == 0 {
        return Err("La playlist está vacía.".into());
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("No se pudo crear {}: {e}", dir.display()))?;
    let base = safe_name(&proj.name);
    if fmt == Format::Stems {
        let folder = dir.join(format!("{base} - stems"));
        std::fs::create_dir_all(&folder).map_err(|e| format!("No se pudo crear {}: {e}", folder.display()))?;
        let chans: Vec<usize> = (0..proj.channels.len()).filter(|&i| proj.audible(&proj.channels[i])).collect();
        let n = chans.len().max(1);
        let sub = AtomicU32::new(0);
        let mut secs = 0.0;
        let mut written = 0;
        for (k, &i) in chans.iter().enumerate() {
            let mut q = proj.clone();
            for (j, c) in q.channels.iter_mut().enumerate() {
                c.solo = j == i;
                c.mute = false;
            }
            let audio = render_song_with(&q, bank, sr as f32, &sub, cancel, &mut |e: &mut Engine| {
                e.bypass_master = true;
                setup(e);
            });
            progress.store(((k + 1) * 1000 / n) as u32, Ordering::Relaxed);
            if cancel.load(Ordering::Relaxed) {
                return Err("Exportación cancelada.".into());
            }
            if audio.iter().all(|x| x.abs() < 1e-5) {
                continue;
            }
            secs = audio.len() as f32 / 2.0 / sr as f32;
            let name = format!("{:02} - {}.wav", k + 1, safe_name(&proj.channels[i].name));
            let bytes = if opts.bits == 24 { wav24(&audio, sr) } else { wav_bytes(&audio.iter().map(|x| (x.clamp(-1.0, 1.0) * 32767.0) as i16).collect::<Vec<_>>(), sr) };
            std::fs::write(folder.join(&name), bytes).map_err(|e| format!("No se pudo escribir {name}: {e}"))?;
            written += 1;
        }
        return Ok(Done { path: folder, secs, peak_db: 0.0, lufs: -70.0, files: written });
    }
    let audio = render_song_with(proj, bank, sr as f32, progress, cancel, setup);
    if cancel.load(Ordering::Relaxed) {
        return Err("Exportación cancelada.".into());
    }
    let peak = audio.iter().fold(0f32, |m, x| m.max(x.abs()));
    // con el limitador encendido el nivel ya es el de la mezcla masterizada: no se toca.
    let normalizar = match opts.norm {
        1 => true,
        2 => false,
        _ => !proj.limiter.on,
    };
    let k = if normalizar && peak > 1e-6 { 0.944 / peak } else if peak > 0.999 { 0.999 / peak } else { 1.0 };
    let pcm: Vec<i16> = audio.iter().map(|x| ((x * k).clamp(-1.0, 1.0) * 32767.0) as i16).collect();
    let path = dir.join(format!("{base}.{}", if fmt == Format::Mp3 { "mp3" } else { "wav" }));
    let bytes = match fmt {
        Format::Mp3 => {
            let kbps = [128, 192, 256, 320].into_iter().find(|b| *b == opts.kbps).unwrap_or(320);
            let cfg = shine_rs::Mp3EncoderConfig::new().sample_rate(sr).bitrate(kbps).channels(2).stereo_mode(shine_rs::StereoMode::Stereo);
            shine_rs::encode_pcm_to_mp3(cfg, &pcm).map_err(|e| format!("Error MP3: {e:?}"))?
        }
        _ if opts.bits == 24 => wav24(&audio.iter().map(|x| x * k).collect::<Vec<_>>(), sr),
        _ => wav_bytes(&pcm, sr),
    };
    std::fs::write(&path, bytes).map_err(|e| format!("No se pudo escribir {}: {e}", path.display()))?;
    let scaled: Vec<f32> = audio.iter().map(|x| x * k).collect();
    let (lufs, _) = crate::loudness::measure(&scaled, sr as f32);
    Ok(Done { path, secs: audio.len() as f32 / 2.0 / sr as f32, peak_db: 20.0 * peak.max(1e-9).log10(), lufs, files: 1 })
}

pub fn safe_name(s: &str) -> String {
    let n: String = s.chars().filter(|c| !matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') && !c.is_control()).collect();
    let n = n.trim().trim_matches('.').to_string();
    if n.is_empty() { "beat".into() } else { n.chars().take(80).collect() }
}

/// Compatibilidad: exporta sin preparación extra. Devuelve (ruta, duración s, pico dBFS).
pub fn export(proj: &Project, bank: &Bank, dir: &Path, fmt: Format, progress: &AtomicU32, cancel: &AtomicBool) -> Result<(PathBuf, f32, f32), String> {
    export_full(proj, bank, dir, fmt, &Opts::default(), progress, cancel, &mut |_| {}).map(|d| (d.path, d.secs, d.peak_db))
}

pub fn wav_bytes(pcm: &[i16], sr: u32) -> Vec<u8> {
    let data_len = (pcm.len() * 2) as u32;
    let mut v = Vec::with_capacity(44 + data_len as usize);
    v.extend_from_slice(b"RIFF");
    v.extend_from_slice(&(36 + data_len).to_le_bytes());
    v.extend_from_slice(b"WAVEfmt ");
    v.extend_from_slice(&16u32.to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes());
    v.extend_from_slice(&2u16.to_le_bytes());
    v.extend_from_slice(&sr.to_le_bytes());
    v.extend_from_slice(&(sr * 4).to_le_bytes());
    v.extend_from_slice(&4u16.to_le_bytes());
    v.extend_from_slice(&16u16.to_le_bytes());
    v.extend_from_slice(b"data");
    v.extend_from_slice(&data_len.to_le_bytes());
    for s in pcm {
        v.extend_from_slice(&s.to_le_bytes());
    }
    v
}

/// WAV mono de 16 bits (tomas de voz).
pub fn write_wav_mono(path: &Path, x: &[f32], sr: u32) -> std::io::Result<()> {
    let pcm: Vec<i16> = x.iter().map(|v| (v.clamp(-1.0, 1.0) * 32767.0) as i16).collect();
    let mut v = wav_bytes(&[], sr);
    // reescribe la cabecera para 1 canal
    let data_len = (pcm.len() * 2) as u32;
    v.truncate(0);
    v.extend_from_slice(b"RIFF");
    v.extend_from_slice(&(36 + data_len).to_le_bytes());
    v.extend_from_slice(b"WAVEfmt ");
    v.extend_from_slice(&16u32.to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes());
    v.extend_from_slice(&sr.to_le_bytes());
    v.extend_from_slice(&(sr * 2).to_le_bytes());
    v.extend_from_slice(&2u16.to_le_bytes());
    v.extend_from_slice(&16u16.to_le_bytes());
    v.extend_from_slice(b"data");
    v.extend_from_slice(&data_len.to_le_bytes());
    for s in pcm {
        v.extend_from_slice(&s.to_le_bytes());
    }
    std::fs::write(path, v)
}

/// Lee un WAV PCM de 16 bits (mono o estéreo → mono). Devuelve (muestras, frecuencia).
pub fn read_wav_mono(path: &Path) -> Option<(Vec<f32>, u32)> {
    let b = std::fs::read(path).ok()?;
    if b.len() < 44 || &b[0..4] != b"RIFF" || &b[8..12] != b"WAVE" {
        return None;
    }
    let mut i = 12;
    let (mut ch, mut sr, mut bits) = (1u16, 44100u32, 16u16);
    while i + 8 <= b.len() {
        let id = &b[i..i + 4];
        let len = u32::from_le_bytes(b[i + 4..i + 8].try_into().ok()?) as usize;
        let body = i + 8;
        if id == b"fmt " {
            ch = u16::from_le_bytes(b[body + 2..body + 4].try_into().ok()?);
            sr = u32::from_le_bytes(b[body + 4..body + 8].try_into().ok()?);
            bits = u16::from_le_bytes(b[body + 14..body + 16].try_into().ok()?);
        } else if id == b"data" {
            if bits != 16 {
                return None;
            }
            let end = (body + len).min(b.len());
            let s: Vec<i16> = b[body..end].chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect();
            let chn = ch.max(1) as usize;
            let mono = s.chunks(chn).map(|f| f.iter().map(|&x| x as f32 / 32768.0).sum::<f32>() / chn as f32).collect();
            return Some((mono, sr));
        }
        i = body + len + (len & 1);
    }
    None
}

/// WAV estéreo de 24 bits.
pub fn wav24(x: &[f32], sr: u32) -> Vec<u8> {
    let data_len = (x.len() * 3) as u32;
    let mut v = Vec::with_capacity(44 + data_len as usize);
    v.extend_from_slice(b"RIFF");
    v.extend_from_slice(&(36 + data_len).to_le_bytes());
    v.extend_from_slice(b"WAVEfmt ");
    v.extend_from_slice(&16u32.to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes());
    v.extend_from_slice(&2u16.to_le_bytes());
    v.extend_from_slice(&sr.to_le_bytes());
    v.extend_from_slice(&(sr * 6).to_le_bytes());
    v.extend_from_slice(&6u16.to_le_bytes());
    v.extend_from_slice(&24u16.to_le_bytes());
    v.extend_from_slice(b"data");
    v.extend_from_slice(&data_len.to_le_bytes());
    for s in x {
        let i = (s.clamp(-1.0, 1.0) * 8_388_607.0) as i32;
        v.extend_from_slice(&i.to_le_bytes()[..3]);
    }
    v
}
