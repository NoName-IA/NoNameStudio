//! Audio de archivos: decodificación (WAV/MP3/FLAC/OGG/M4A), estiramiento de tiempo,
//! cambio de tono, detección de transitorios y la biblioteca de buffers del proyecto.
use crate::project::Project;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Audio en memoria (1 o 2 canales intercalados).
pub struct Buf {
    pub ch: usize,
    pub sr: f32,
    pub data: Vec<f32>,
}
impl Buf {
    pub fn frames(&self) -> usize {
        self.data.len() / self.ch.max(1)
    }
    pub fn secs(&self) -> f32 {
        self.frames() as f32 / self.sr
    }
    #[inline]
    pub fn at(&self, i: usize) -> (f32, f32) {
        if self.ch == 1 {
            let v = self.data[i];
            (v, v)
        } else {
            (self.data[i * 2], self.data[i * 2 + 1])
        }
    }
    /// Lectura con interpolación lineal en la posición fraccionaria `p` (en cuadros).
    #[inline]
    pub fn lerp(&self, p: f64) -> (f32, f32) {
        let n = self.frames();
        if n == 0 || p < 0.0 {
            return (0.0, 0.0);
        }
        let i = p as usize;
        if i + 1 >= n {
            return if i < n { self.at(i) } else { (0.0, 0.0) };
        }
        let f = (p - i as f64) as f32;
        let (a, b) = self.at(i);
        let (c, d) = self.at(i + 1);
        (a + (c - a) * f, b + (d - b) * f)
    }
    pub fn mono(&self) -> Vec<f32> {
        if self.ch == 1 {
            return self.data.clone();
        }
        self.data.chunks(self.ch).map(|f| f.iter().sum::<f32>() / self.ch as f32).collect()
    }
    /// Picos por tramo para dibujar la forma de onda.
    pub fn peaks(&self, buckets: usize) -> Vec<f32> {
        let n = self.frames();
        if n == 0 {
            return vec![0.0; buckets];
        }
        (0..buckets)
            .map(|b| {
                let a = b * n / buckets;
                let e = ((b + 1) * n / buckets).max(a + 1).min(n);
                let mut m = 0f32;
                for i in a..e {
                    let (l, r) = self.at(i);
                    m = m.max(l.abs()).max(r.abs());
                }
                m
            })
            .collect()
    }
}

/// Biblioteca de audio del proyecto: archivo (o variante procesada) → buffer.
pub type Files = HashMap<String, Arc<Buf>>;

pub const AUDIO_EXT: [&str; 8] = ["wav", "mp3", "flac", "ogg", "oga", "m4a", "aac", "aif"];

pub fn is_audio(p: &Path) -> bool {
    p.extension().map(|e| e.to_string_lossy().to_lowercase()).is_some_and(|e| AUDIO_EXT.contains(&e.as_str()))
}

/// Decodifica cualquier formato soportado a f32 (máximo 2 canales).
pub fn decode(path: &Path) -> Result<Buf, String> {
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
    use symphonia::core::errors::Error;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let file = std::fs::File::open(path).map_err(|e| format!("No se pudo abrir {}: {e}", path.display()))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension() {
        hint.with_extension(&ext.to_string_lossy());
    }
    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(|e| format!("Formato no reconocido: {e}"))?;
    let mut format = probed.format;
    let track = format.tracks().iter().find(|t| t.codec_params.codec != CODEC_TYPE_NULL).ok_or("El archivo no tiene audio")?;
    let tid = track.id;
    let mut dec = symphonia::default::get_codecs().make(&track.codec_params, &DecoderOptions::default()).map_err(|e| format!("Códec no soportado: {e}"))?;
    let mut sr = track.codec_params.sample_rate.unwrap_or(44100) as f32;
    let mut out: Vec<f32> = vec![];
    let mut chans = 0usize;
    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(Error::IoError(_)) | Err(Error::ResetRequired) => break,
            Err(e) => return Err(format!("Error leyendo el audio: {e}")),
        };
        if packet.track_id() != tid {
            continue;
        }
        let decoded = match dec.decode(&packet) {
            Ok(d) => d,
            Err(Error::DecodeError(_)) => continue,
            Err(e) => return Err(format!("Error decodificando: {e}")),
        };
        let spec = *decoded.spec();
        sr = spec.rate as f32;
        let c = spec.channels.count().max(1);
        let mut sb = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
        sb.copy_interleaved_ref(decoded);
        let s = sb.samples();
        let keep = c.min(2);
        if chans == 0 {
            chans = keep;
        }
        for f in s.chunks(c) {
            if keep == 1 || chans == 1 {
                out.push(f.iter().sum::<f32>() / c as f32);
            } else {
                out.push(f[0]);
                out.push(f[1]);
            }
        }
        if out.len() > 48_000 * 2 * 60 * 12 {
            return Err("El archivo dura más de 12 minutos.".into());
        }
    }
    if out.is_empty() {
        return Err("El archivo no tiene muestras.".into());
    }
    let ch = chans.max(1);
    // canal estéreo que en realidad es mono → mono (ahorra memoria y CPU)
    if ch == 2 && out.chunks(2).all(|f| (f[0] - f[1]).abs() < 1e-5) {
        let m: Vec<f32> = out.chunks(2).map(|f| f[0]).collect();
        return Ok(Buf { ch: 1, sr, data: m });
    }
    Ok(Buf { ch, sr, data: out })
}

/// Escribe un buffer como WAV de 16 bits.
pub fn write_wav(path: &Path, b: &Buf) -> std::io::Result<()> {
    let ch = b.ch as u16;
    let sr = b.sr as u32;
    let data_len = (b.data.len() * 2) as u32;
    let mut v = Vec::with_capacity(44 + data_len as usize);
    v.extend_from_slice(b"RIFF");
    v.extend_from_slice(&(36 + data_len).to_le_bytes());
    v.extend_from_slice(b"WAVEfmt ");
    v.extend_from_slice(&16u32.to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes());
    v.extend_from_slice(&ch.to_le_bytes());
    v.extend_from_slice(&sr.to_le_bytes());
    v.extend_from_slice(&(sr * 2 * ch as u32).to_le_bytes());
    v.extend_from_slice(&(2 * ch).to_le_bytes());
    v.extend_from_slice(&16u16.to_le_bytes());
    v.extend_from_slice(b"data");
    v.extend_from_slice(&data_len.to_le_bytes());
    for s in &b.data {
        v.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    std::fs::write(path, v)
}

/// Carpetas donde viven los archivos del proyecto.
pub fn samples_dir(data: &Path) -> PathBuf {
    data.join("samples")
}
pub fn resolve(data: &Path, name: &str) -> Option<PathBuf> {
    if name.is_empty() {
        return None;
    }
    for d in [data.join("samples"), data.join("audio")] {
        let p = d.join(name);
        if p.is_file() {
            return Some(p);
        }
    }
    let p = PathBuf::from(name);
    p.is_file().then_some(p)
}

/// Copia un archivo externo a la carpeta de samples (nombre único). Devuelve el nombre.
pub fn import_file(data: &Path, src: &Path) -> Result<String, String> {
    let dir = samples_dir(data);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let stem = src.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "sample".into());
    let ext = src.extension().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_else(|| "wav".into());
    let stem = crate::export::safe_name(&stem);
    // si ya existe un archivo idéntico, se reutiliza
    let len = std::fs::metadata(src).map(|m| m.len()).unwrap_or(0);
    for i in 0..1000 {
        let name = if i == 0 { format!("{stem}.{ext}") } else { format!("{stem} ({i}).{ext}") };
        let dst = dir.join(&name);
        if dst.exists() {
            if std::fs::metadata(&dst).map(|m| m.len()).unwrap_or(1) == len && src.canonicalize().ok() != dst.canonicalize().ok() {
                if std::fs::read(&dst).ok() == std::fs::read(src).ok() {
                    return Ok(name);
                }
            } else if src.canonicalize().ok() == dst.canonicalize().ok() {
                return Ok(name);
            }
            continue;
        }
        std::fs::copy(src, &dst).map_err(|e| format!("No se pudo copiar el archivo: {e}"))?;
        return Ok(name);
    }
    Err("Demasiados archivos con el mismo nombre.".into())
}

/// Clave de una variante procesada (estirada y/o transpuesta) de un archivo.
pub fn proc_key(file: &str, ratio: f32, pitch: f32, reverse: bool) -> String {
    let r = (ratio * 1000.0).round() / 1000.0;
    let p = (pitch * 100.0).round() / 100.0;
    if (r - 1.0).abs() < 1e-4 && p.abs() < 1e-3 && !reverse {
        file.to_string()
    } else {
        format!("{file}#r{r:.3}#p{p:.2}{}", if reverse { "#rev" } else { "" })
    }
}
pub fn parse_key(key: &str) -> (String, f32, f32, bool) {
    let mut it = key.split('#');
    let file = it.next().unwrap_or("").to_string();
    let (mut r, mut p, mut rev) = (1.0, 0.0, false);
    for part in it {
        if let Some(x) = part.strip_prefix('r').and_then(|v| v.parse().ok()) {
            r = x;
        } else if let Some(x) = part.strip_prefix('p').and_then(|v| v.parse().ok()) {
            p = x;
        } else if part == "rev" {
            rev = true;
        }
    }
    (file, r, p, rev)
}

/// Todas las claves de audio que necesita el proyecto (originales y procesadas).
pub fn needed_keys(p: &Project) -> Vec<String> {
    let mut v = vec![];
    for c in &p.channels {
        if c.inst == crate::project::Inst::Sampler && !c.smp.file.is_empty() {
            v.push(c.smp.file.clone());
            let k = proc_key(&c.smp.file, c.smp.ratio(p.bpm), c.smp.pitch, false);
            if k != c.smp.file {
                v.push(k);
            }
        }
    }
    for a in &p.audio_clips {
        for t in &a.takes {
            v.push(t.clone());
        }
        let f = a.file();
        let k = proc_key(f, a.ratio(p.bpm), a.pitch, a.reverse);
        if k != f {
            v.push(k);
        }
    }
    v.sort();
    v.dedup();
    v
}

/// Produce el buffer de una clave (decodifica y procesa).
pub fn build(data: &Path, key: &str, files: &Files) -> Result<Buf, String> {
    let (file, r, p, rev) = parse_key(key);
    let src: Arc<Buf> = match files.get(&file) {
        Some(b) => b.clone(),
        None => Arc::new(decode(&resolve(data, &file).ok_or_else(|| format!("No se encuentra «{file}»"))?)?),
    };
    if key == file {
        return Ok(Buf { ch: src.ch, sr: src.sr, data: src.data.clone() });
    }
    let mut b = transform(&src, r, p);
    if rev {
        let ch = b.ch;
        let n = b.frames();
        let mut d = vec![0.0; b.data.len()];
        for i in 0..n {
            for c in 0..ch {
                d[i * ch + c] = b.data[(n - 1 - i) * ch + c];
            }
        }
        b.data = d;
    }
    Ok(b)
}

/// Carga en `files` todo lo que falte para el proyecto (para exportar sin la interfaz).
pub fn ensure(p: &Project, data: &Path, files: &mut Files) {
    for k in needed_keys(p) {
        if !files.contains_key(&k) {
            if let Ok(b) = build(data, &k, files) {
                files.insert(k, Arc::new(b));
            }
        }
    }
}

// ===================== estiramiento de tiempo y tono =====================

/// Estira la duración por `ratio` y cambia el tono `pitch` semitonos (independientes).
pub fn transform(src: &Buf, ratio: f32, pitch: f32) -> Buf {
    let pf = 2f32.powf(pitch / 12.0);
    // WSOLA estira ratio·pf, luego se re-muestrea por pf → duración ratio, tono pf
    let st = wsola(src, (ratio * pf) as f64);
    if (pf - 1.0).abs() < 1e-4 {
        return st;
    }
    resample(&st, pf as f64)
}

/// Lee el buffer a velocidad `speed` (2 = el doble de rápido, una octava arriba).
pub fn resample(b: &Buf, speed: f64) -> Buf {
    let n = b.frames();
    let out_n = ((n as f64) / speed).floor() as usize;
    let mut d = Vec::with_capacity(out_n * b.ch);
    for i in 0..out_n {
        let (l, r) = b.lerp(i as f64 * speed);
        d.push(l);
        if b.ch == 2 {
            d.push(r);
        }
    }
    Buf { ch: b.ch, sr: b.sr, data: d }
}

/// WSOLA (superposición con búsqueda de similitud): cambia la duración sin cambiar el tono.
pub fn wsola(b: &Buf, ratio: f64) -> Buf {
    let n = b.frames();
    if (ratio - 1.0).abs() < 1e-4 || n < 4096 {
        return if (ratio - 1.0).abs() < 1e-4 { Buf { ch: b.ch, sr: b.sr, data: b.data.clone() } } else { resample(b, 1.0 / ratio) };
    }
    let win = ((b.sr * 0.046) as usize).max(256) & !1; // ~46 ms
    let hop_out = win / 2;
    let hop_in = hop_out as f64 / ratio;
    let tol = ((b.sr * 0.012) as usize).max(32);
    let mono = b.mono();
    let out_n = (n as f64 * ratio) as usize + win;
    let ch = b.ch;
    let mut out = vec![0f32; out_n * ch];
    let mut norm = vec![0f32; out_n];
    let hann: Vec<f32> = (0..win).map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / win as f32).cos()).collect();
    let mut prev_pos: isize = 0;
    let mut k = 0usize;
    loop {
        let out_pos = k * hop_out;
        if out_pos + win > out_n {
            break;
        }
        let nominal = (k as f64 * hop_in) as isize;
        if nominal as usize >= n {
            break;
        }
        // buscar el desplazamiento que mejor continúa el segmento anterior
        let pos = if k == 0 {
            0
        } else {
            let target = prev_pos + hop_out as isize; // continuación natural
            let mut best = nominal;
            let mut best_c = f32::MIN;
            let cmp = (win / 2).min(512);
            let mut d = -(tol as isize);
            while d <= tol as isize {
                let cand = nominal + d;
                if cand >= 0 && (cand as usize) + cmp < n && target >= 0 && (target as usize) + cmp < n {
                    let mut c = 0f32;
                    let mut e = 1e-9f32;
                    let mut i = 0;
                    while i < cmp {
                        let a = mono[target as usize + i];
                        let bb = mono[cand as usize + i];
                        c += a * bb;
                        e += bb * bb;
                        i += 2;
                    }
                    let score = c / e.sqrt();
                    if score > best_c {
                        best_c = score;
                        best = cand;
                    }
                }
                d += 4;
            }
            best
        };
        for i in 0..win {
            let si = pos as usize + i;
            if si >= n {
                break;
            }
            let w = hann[i];
            for c in 0..ch {
                out[(out_pos + i) * ch + c] += b.data[si * ch + c] * w;
            }
            norm[out_pos + i] += w;
        }
        prev_pos = pos;
        k += 1;
    }
    let final_n = ((n as f64 * ratio) as usize).min(out_n);
    for i in 0..final_n {
        let g = if norm[i] > 0.05 { 1.0 / norm[i] } else { 0.0 };
        for c in 0..ch {
            out[i * ch + c] *= g;
        }
    }
    out.truncate(final_n * ch);
    Buf { ch, sr: b.sr, data: out }
}

/// Detección de transitorios (golpes) para el slicer. Devuelve posiciones en segundos.
pub fn onsets(b: &Buf, sensitivity: f32, max: usize) -> Vec<f32> {
    let x = b.mono();
    let hop = 256usize;
    let frames = x.len() / hop;
    if frames < 4 {
        return vec![0.0];
    }
    // energía por tramo con un pasa-altos sencillo (resalta ataques)
    let mut env = Vec::with_capacity(frames);
    let mut prev = 0f32;
    for f in 0..frames {
        let mut e = 0f32;
        for i in 0..hop {
            let v = x[f * hop + i];
            let hp = v - prev * 0.95;
            prev = v;
            e += hp * hp;
        }
        env.push((e / hop as f32).sqrt());
    }
    let mut flux: Vec<f32> = (0..frames).map(|f| if f == 0 { 0.0 } else { (env[f] - env[f - 1]).max(0.0) }).collect();
    let mx = flux.iter().fold(0f32, |m, v| m.max(*v)).max(1e-9);
    for v in &mut flux {
        *v /= mx;
    }
    let thr = (0.35 - sensitivity.clamp(0.0, 1.0) * 0.3).max(0.03);
    let min_gap = ((b.sr * 0.07) as usize / hop).max(1);
    let mut out = vec![0.0f32];
    let mut last = 0usize;
    for f in 2..frames.saturating_sub(1) {
        // máximo local por encima del umbral adaptativo
        let local: f32 = flux[f.saturating_sub(8)..(f + 8).min(frames)].iter().sum::<f32>() / 16.0;
        if flux[f] > thr && flux[f] >= flux[f - 1] && flux[f] >= flux[f + 1] && flux[f] > local * 1.5 && f - last >= min_gap {
            out.push((f * hop) as f32 / b.sr);
            last = f;
        }
    }
    if out.len() > max {
        // quedarse con los más fuertes
        let mut sc: Vec<(f32, f32)> = out.iter().skip(1).map(|&t| (flux[((t * b.sr) as usize / hop).min(frames - 1)], t)).collect();
        sc.sort_by(|a, b| b.0.total_cmp(&a.0));
        let mut keep: Vec<f32> = sc.into_iter().take(max - 1).map(|x| x.1).collect();
        keep.push(0.0);
        keep.sort_by(|a, b| a.total_cmp(b));
        return keep;
    }
    out
}

/// Cortes iguales (por ejemplo 8 o 16 trozos).
pub fn even_slices(b: &Buf, n: usize) -> Vec<f32> {
    let d = b.secs();
    (0..n.max(1)).map(|i| d * i as f32 / n.max(1) as f32).collect()
}

/// Adivina el tempo de un loop: primero por el nombre («loop 92bpm»), luego por la duración.
pub fn guess_bpm(name: &str, secs: f32) -> f32 {
    let low = name.to_lowercase();
    let bytes: Vec<char> = low.chars().collect();
    for i in 0..bytes.len() {
        if bytes[i].is_ascii_digit() && (i == 0 || !bytes[i - 1].is_ascii_digit()) {
            let mut j = i;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            let num: String = bytes[i..j].iter().collect();
            let rest: String = bytes[j..].iter().collect::<String>().trim_start_matches([' ', '_', '-']).to_string();
            if let Ok(v) = num.parse::<f32>() {
                if (60.0..=200.0).contains(&v) && (rest.starts_with("bpm") || j - i == 2 || j - i == 3) && (rest.starts_with("bpm") || low.contains("bpm")) {
                    return v;
                }
            }
        }
    }
    // duración = compases × 4 tiempos × 60 / bpm  → el número de compases que da un tempo razonable
    let mut best = 0.0;
    let mut best_d = f32::MAX;
    for bars in [1.0f32, 2.0, 4.0, 8.0, 16.0] {
        let bpm = bars * 4.0 * 60.0 / secs.max(0.1);
        if (70.0..=180.0).contains(&bpm) {
            let d = (bpm - 115.0).abs();
            if d < best_d {
                best_d = d;
                best = bpm;
            }
        }
    }
    (best * 10.0f32).round() / 10.0
}
