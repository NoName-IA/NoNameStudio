// Mide NN Tune con grabaciones de voz reales: qué tan clavada queda la salida en las notas,
// cuánto se cuela la voz sin afinar y si aparecen chasquidos.
// Uso: cargo run --example tune_real -- <carpeta_salida> <archivo.wav>...
use noname_studio::fx::{autotune, yin, ESTILOS};
use noname_studio::project::{scale_mask, TuneP};
use std::path::Path;

fn cents_fuera(hz: f32) -> f32 {
    let m = 69.0 + 12.0 * (hz / 440.0).log2();
    (m - m.round()) * 100.0
}

fn medir(x: &[f32], sr: f32) -> (usize, Vec<f32>) {
    let (win, hop) = (2048usize, 512usize);
    let mut err = vec![];
    let mut total = 0;
    let mut k = 0;
    while k + win <= x.len() {
        let fr = &x[k..k + win];
        let rms = (fr.iter().map(|v| v * v).sum::<f32>() / win as f32).sqrt();
        if rms > 0.01 {
            total += 1;
            if let Some(h) = yin(fr, sr) {
                if h > 70.0 && h < 1000.0 {
                    err.push(cents_fuera(h).abs());
                }
            }
        }
        k += hop;
    }
    (total, err)
}

fn chasquidos(x: &[f32]) -> usize {
    // saltos muy grandes entre muestras vecinas comparados con el nivel local
    let mut n = 0;
    for w in x.windows(3) {
        let d2 = (w[2] - 2.0 * w[1] + w[0]).abs();
        if d2 > 0.25 {
            n += 1;
        }
    }
    n
}

fn resumen(e: &[f32]) -> String {
    if e.is_empty() {
        return "sin tono".into();
    }
    let mut a = e.to_vec();
    a.sort_by(|p, q| p.total_cmp(q));
    let p = |q: f32| a[((a.len() - 1) as f32 * q) as usize];
    let fuera = a.iter().filter(|v| **v > 15.0).count() as f32 / a.len() as f32 * 100.0;
    format!("mediana {:.0}¢ · p90 {:.0}¢ · {:.0}% de tramos a más de 15¢", p(0.5), p(0.9), fuera)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = Path::new(&args[1]);
    std::fs::create_dir_all(dir).ok();
    for f in &args[2..] {
        let b = noname_studio::media::decode(Path::new(f)).expect("no se pudo leer");
        let ch = b.ch as usize;
        let sr = b.sr as f32;
        let x: Vec<f32> = b.data.chunks(ch).map(|c| c.iter().sum::<f32>() / ch as f32).collect();
        let pk = x.iter().fold(0f32, |m, v| m.max(v.abs()));
        let nombre = Path::new(f).file_stem().unwrap().to_string_lossy().to_string();
        println!("== {nombre}: {:.1} s, {} Hz, pico {:.2}", x.len() as f32 / sr, sr, pk);
        let (t, e) = medir(&x, sr);
        println!("   original        : {} tramos con voz, {} con tono · {}", t, e.len(), resumen(&e));
        diagnostico(&x, sr);
        tramo_a_tramo(&x, sr);
        let cx = chasquidos(&x);
        for (estilo, alg, speed) in [(0u8, 0u8, 0.0f32), (0, 0, 0.45), (1, 0, 0.45), (0, 1, 0.0), (2, 0, 0.0)] {
            // escala cromática: la salida debe caer en cualquier semitono exacto
            let tp = TuneP { on: true, key: 0, scale: 0, speed, mix: 1.0, gain: 1.0, mask: 0xFFF, estilo, algoritmo: alg, mezcla: 1.0 };
            let r = autotune(&x, sr, &tp);
            let (_, e2) = medir(&r.out, sr);
            let pk2 = r.out.iter().fold(0f32, |m, v| m.max(v.abs()));
            println!(
                "   {:<22}: {} · pico {:.2} · chasquidos {} (orig {})",
                format!("{}{} vel {:.2}", ESTILOS[estilo as usize].nombre, if alg == 1 { " (baja dem.)" } else { "" }, speed),
                resumen(&e2),
                pk2,
                chasquidos(&r.out),
                cx
            );
            let out = noname_studio::media::Buf { data: r.out.clone(), ch: 1, sr: b.sr };
            let _ = noname_studio::media::write_wav(&dir.join(format!("{nombre}_e{estilo}_a{alg}_v{:.0}.wav", speed * 100.0)), &out);
        }
        let _ = scale_mask;
    }
}

#[allow(dead_code)]
pub fn diagnostico(x: &[f32], sr: f32) {
    use noname_studio::fx::TuneLive;
    let tp = TuneP { on: true, key: 0, scale: 0, speed: 0.0, mix: 1.0, gain: 1.0, mask: 0xFFF, estilo: 2, algoritmo: 0, mezcla: 1.0 };
    let mut tl = TuneLive::new(sr);
    let (mut sin_det, mut octava, mut otra, mut bien, mut voz) = (0, 0, 0, 0, 0);
    for i in 0..x.len() {
        tl.process(x[i], &tp);
        if i % 512 == 0 && i >= 2048 {
            let fr = &x[i - 2048..i];
            let rms = (fr.iter().map(|v| v * v).sum::<f32>() / 2048.0).sqrt();
            if rms < 0.01 { continue; }
            if let Some(h) = yin(fr, sr) {
                voz += 1;
                let m = 69.0 + 12.0 * (h / 440.0).log2();
                let c = tl.cantada;
                if c <= 0.0 { sin_det += 1 } else if ((c - m).abs() - 12.0).abs() < 0.7 { octava += 1 } else if (c - m).abs() > 0.7 { otra += 1 } else { bien += 1 }
            }
        }
    }
    println!("   detector en vivo: {voz} tramos con tono · bien {bien} · sin detectar {sin_det} · octava mal {octava} · otro tono {otra}");
}

#[allow(dead_code)]
pub fn tramo_a_tramo(x: &[f32], sr: f32) {
    use noname_studio::fx::TuneLive;
    let tp = TuneP { on: true, key: 0, scale: 0, speed: 0.0, mix: 1.0, gain: 1.0, mask: 0xFFF, estilo: 2, algoritmo: 0, mezcla: 1.0 };
    let mut tl = TuneLive::new(sr);
    let lat = tl.latency(&tp);
    let mut out = vec![];
    let mut obj = vec![];
    for i in 0..x.len() + lat {
        out.push(tl.process(x.get(i).copied().unwrap_or(0.0), &tp));
        obj.push(tl.objetivo);
    }
    let out = &out[lat..];
    let mut malos = 0;
    let mut total = 0;
    let mut i = 4096;
    while i + 2048 < x.len() {
        let fr = &out[i..i + 2048];
        let rms = (fr.iter().map(|v| v * v).sum::<f32>() / 2048.0).sqrt();
        if rms > 0.01 {
            if let (Some(hi), Some(ho)) = (yin(&x[i..i + 2048], sr), yin(fr, sr)) {
                total += 1;
                let mo = 69.0 + 12.0 * (ho / 440.0).log2();
                let mi = 69.0 + 12.0 * (hi / 440.0).log2();
                let t = obj[i + 1024 + lat];
                if (mo - mo.round()).abs() > 0.15 {
                    malos += 1;
                    if malos <= 12 {
                        println!("     t={:.2}s entrada {:.2} salida {:.2} objetivo {}", i as f32 / sr, mi, mo, t);
                    }
                }
            }
        }
        i += 1024;
    }
    println!("   tramo a tramo: {malos} de {total} fuera");
}
