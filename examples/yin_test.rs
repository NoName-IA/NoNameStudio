use noname_studio::fx::yin;
fn main() {
    for sr in [44100.0f32, 22050.0] {
        for hz in [110.0f32, 220.0, 233.08, 300.0] {
            let n = if sr > 30000.0 { 2048 } else { 1024 };
            let x: Vec<f32> = (0..n).map(|i| { let ph = std::f32::consts::TAU * hz * i as f32 / sr; ph.sin() + 0.5 * (2.0 * ph).sin() + 0.25 * (3.0 * ph).sin() }).collect();
            let off: Vec<f32> = (0..n).map(|i| { let ph = std::f32::consts::TAU * hz * (i as f32 + 37.0) / sr; ph.sin() + 0.5 * (2.0 * ph).sin() + 0.25 * (3.0 * ph).sin() }).collect();
            println!("sr {sr} real {hz:7.2} → {:?} / desfasado {:?}", yin(&x, sr), yin(&off, sr));
        }
    }
}
