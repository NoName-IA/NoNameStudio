// Diagnóstico: nivel RMS/pico de cada instrumento por separado.
use noname_studio::engine::render_song;
use noname_studio::project::Project;
use std::sync::atomic::{AtomicBool, AtomicU32};

fn stats(a: &[f32]) -> (f32, f32) {
    let rms = (a.iter().map(|x| x * x).sum::<f32>() / a.len() as f32).sqrt();
    let pk = a.iter().fold(0f32, |m, x| m.max(x.abs()));
    (20.0 * rms.max(1e-9).log10(), 20.0 * pk.max(1e-9).log10())
}

fn main() {
    let base = Project::demo();
    let (pr, ca) = (AtomicU32::new(0), AtomicBool::new(false));
    let sm = Default::default();
    let full = render_song(&base, &sm, 44100.0, &pr, &ca);
    let (r, p) = stats(&full);
    println!("{:<12} rms {:6.1} dB  pico {:6.1} dB", "MEZCLA", r, p);
    let mut v = base.clone();
    for c in &mut v.channels {
        c.mute = true;
    }
    let (r, p) = stats(&render_song(&v, &sm, 44100.0, &pr, &ca));
    println!("{:<12} rms {:6.1} dB  pico {:6.1} dB", "solo vinilo", r, p);
    for i in 0..base.channels.len() {
        let mut q = base.clone();
        q.vinyl = 0.0;
        for (j, c) in q.channels.iter_mut().enumerate() {
            c.solo = j == i;
        }
        let (r, p) = stats(&render_song(&q, &sm, 44100.0, &pr, &ca));
        println!("{:<12} rms {:6.1} dB  pico {:6.1} dB", base.channels[i].name, r, p);
    }
}
