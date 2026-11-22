// Verifica NN Tune: una voz sintética desafinada debe quedar en la nota de la escala.
use noname_studio::fx::autotune;
use noname_studio::project::TuneP;

fn main() {
    let sr = 44100.0f32;
    for (label, midi) in [("La3 +30 cents", 57.3f32), ("Si3 (fuera de Sol menor) → Sib3/Do4", 59.0), ("Re4 -40 cents", 61.6)] {
        let f = 440.0 * 2f32.powf((midi - 69.0) / 12.0);
        // fase integrada con vibrato real de ±7 cents a 5 Hz
        let mut ph = 0.0f32;
        let x: Vec<f32> = (0..(sr * 2.0) as usize)
            .map(|n| {
                let t = n as f32 / sr;
                let fi = f * (1.0 + 0.004 * (std::f32::consts::TAU * 5.0 * t).sin());
                ph = (ph + fi / sr).fract();
                let a = std::f32::consts::TAU * ph;
                0.4 * (a.sin() + 0.5 * (2.0 * a).sin() + 0.25 * (3.0 * a).sin())
            })
            .collect();
        let t0 = std::time::Instant::now();
        let r = autotune(&x, sr, &TuneP { speed: 0.0, ..TuneP::default() });
        let ms = t0.elapsed().as_millis();
        // medir el tono real de la salida con el mismo detector (corrección apagada)
        let check = autotune(&r.out, sr, &TuneP { on: false, ..TuneP::default() });
        let mid = |v: &[f32]| {
            let mut a: Vec<f32> = v.iter().copied().filter(|m| *m > 0.0).collect();
            a.sort_by(|p, q| p.total_cmp(q));
            if a.is_empty() { 0.0 } else { a[a.len() / 2] }
        };
        println!("   cruces por cero: entrada {} salida {} (razón {:.4}, esperada {:.4})", zc(&x[22050..66150]), zc(&r.out[22050..66150]), zc(&r.out[22050..66150]) as f32 / zc(&x[22050..66150]) as f32, 2f32.powf((mid(&r.corrected) - mid(&r.detected)) / 12.0));
        println!("{label:<36} entrada {:.2} → objetivo {:.2} → salida medida {:.2}  ({} ms)", mid(&r.detected), mid(&r.corrected), mid(&check.detected), ms);
    }
}

#[allow(dead_code)]
pub fn zc(x: &[f32]) -> usize {
    x.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count()
}
