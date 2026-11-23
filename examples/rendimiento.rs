// Mide cuánto tarda el motor en calcular el audio (veces más rápido que el tiempo real).
use noname_studio::engine::{render_song, Bank};
use noname_studio::project::Project;
use std::sync::atomic::{AtomicBool, AtomicU32};
use std::time::Instant;

fn main() {
    let (pr, ca) = (AtomicU32::new(0), AtomicBool::new(false));
    let mut casos: Vec<(String, Project)> = vec![("Beat de ejemplo".into(), Project::demo())];
    for (i, n) in noname_studio::templates::TEMPLATES.iter().enumerate().skip(1) {
        casos.push((format!("Plantilla {n}"), noname_studio::templates::build(i)));
    }
    for (n, p) in casos {
        let mut ms = f64::MAX;
        let mut a = vec![];
        for _ in 0..3 {
            let t0 = Instant::now();
            a = render_song(&p, &Bank::default(), 48_000.0, &pr, &ca);
            ms = ms.min(t0.elapsed().as_secs_f64() * 1000.0);
        }
        let secs = a.len() as f64 / 2.0 / 48_000.0;
        println!("{n:<22} {secs:6.1} s de audio en {ms:7.0} ms  →  {:5.0}× tiempo real ({:.1} % de un núcleo)", secs * 1000.0 / ms, ms / (secs * 10.0));
    }
    if let (Some(m), Some(r)) = (std::env::args().nth(1), std::env::args().nth(2)) {
        comparar(&r, m == "guardar");
        return;
    }
    if std::env::args().nth(1).as_deref() == Some("demo") {
        por_canal(999);
        let (pr, ca) = (AtomicU32::new(0), AtomicBool::new(false));
        let mut p = Project::demo();
        for c in p.channels.iter_mut() {
            c.mute = true;
        }
        let t0 = Instant::now();
        let _ = render_song(&p, &Bank::default(), 48_000.0, &pr, &ca);
        println!("  todo en silencio: {:.0} ms", t0.elapsed().as_secs_f64() * 1000.0);
        p.vinyl = 0.0;
        let t0 = Instant::now();
        let _ = render_song(&p, &Bank::default(), 48_000.0, &pr, &ca);
        println!("  todo en silencio y sin vinilo: {:.0} ms", t0.elapsed().as_secs_f64() * 1000.0);
        return;
    }
    if let Some(nombre) = std::env::args().nth(1).filter(|a| a.starts_with("pl:")) {
        let nombre = nombre.trim_start_matches("pl:").to_string();
        println!("{nombre}:");
        por_canal(noname_studio::templates::TEMPLATES.iter().position(|n| n.contains(nombre.as_str())).unwrap());
        return;
    }
    // motor detenido: debe descansar
    {
        let st = std::sync::Arc::new(noname_studio::engine::Status::default());
        let mut e = noname_studio::engine::Engine::new(48_000.0, Project::demo(), st);
        let mut b = vec![0f32; 1024];
        let t0 = Instant::now();
        for _ in 0..2000 {
            e.render(&mut b);
        }
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        println!("Motor detenido         {:.1} s de silencio en {ms:.1} ms → {:.2} % de un núcleo", 2000.0 * 512.0 / 48_000.0, ms / (2000.0 * 512.0 / 48_000.0) / 10.0);
    }
    // NN Tune en vivo
    let mut tl = noname_studio::fx::TuneLive::new(48_000.0);
    let tp = noname_studio::project::TuneP::default();
    let t0 = Instant::now();
    let mut acc = 0f32;
    for i in 0..48_000 * 10 {
        acc += tl.process((i as f32 / 48_000.0 * 220.0 * 6.283).sin() * 0.5, &tp);
    }
    let ms = t0.elapsed().as_secs_f64() * 1000.0;
    println!("NN Tune en vivo        10 s de voz en {ms:5.0} ms → {:.1} % de un núcleo {}", ms / 100.0, if acc.is_finite() { "" } else { "!" });
}

#[allow(dead_code)]
pub fn por_canal(i: usize) {
    let (pr, ca) = (AtomicU32::new(0), AtomicBool::new(false));
    let base = if i == 999 { Project::demo() } else { noname_studio::templates::build(i) };
    let t = |p: &Project| {
        let t0 = Instant::now();
        let _ = render_song(p, &Bank::default(), 48_000.0, &pr, &ca);
        t0.elapsed().as_secs_f64() * 1000.0
    };
    let todo = t(&base);
    println!("  todo: {todo:.0} ms");
    for k in 0..base.channels.len() {
        let mut p = base.clone();
        p.channels[k].mute = true;
        println!("  sin {:<14} ({:?}, rev {:.2}, dly {:.2}, space {:.2}, comp {}): {:.0} ms", p.channels[k].name, p.channels[k].inst, p.channels[k].rev, p.channels[k].dly, p.channels[k].space, p.channels[k].comp.on, t(&p));
    }
    let mut p = base.clone();
    p.limiter.on = false;
    p.master_comp.on = false;
    println!("  sin limitador ni compresor del master: {:.0} ms", t(&p));
    println!("  verb {:?} / space {:?}", base.verb, base.space);
}

/// Guarda (o compara con) el audio de una plantilla, para verificar que una optimización no cambia el sonido.
#[allow(dead_code)]
pub fn comparar(ruta: &str, guardar: bool) {
    let (pr, ca) = (AtomicU32::new(0), AtomicBool::new(false));
    let mut total = vec![];
    for i in 1..noname_studio::templates::TEMPLATES.len() {
        total.extend(render_song(&noname_studio::templates::build(i), &Bank::default(), 48_000.0, &pr, &ca));
    }
    total.extend(render_song(&Project::demo(), &Bank::default(), 48_000.0, &pr, &ca));
    if guardar {
        let b: Vec<u8> = total.iter().flat_map(|v| v.to_le_bytes()).collect();
        std::fs::write(ruta, b).unwrap();
        println!("guardado: {} muestras", total.len());
    } else {
        let b = std::fs::read(ruta).unwrap();
        let antes: Vec<f32> = b.chunks(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
        let n = antes.len().min(total.len());
        let (mut e, mut d) = (0f64, 0f64);
        for i in 0..n {
            e += (antes[i] as f64).powi(2);
            d += ((antes[i] - total[i]) as f64).powi(2);
        }
        println!("largo antes {} / ahora {} · diferencia: {:.1} dB por debajo de la música", antes.len(), total.len(), 10.0 * (e / d.max(1e-30)).log10());
    }
}
