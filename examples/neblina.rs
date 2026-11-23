// Genera la plantilla «Neblina»: guarda el proyecto (.nns.json) y exporta un MP3 de muestra.
use noname_studio::export::{export_full, Format, Opts};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32};

fn main() {
    let dir = PathBuf::from(std::env::args().nth(1).expect("carpeta de salida"));
    let p = noname_studio::templates::build(7);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("Neblina.nns.json"), serde_json::to_string_pretty(&p).unwrap()).unwrap();
    let (pr, ca) = (AtomicU32::new(0), AtomicBool::new(false));
    let d = export_full(&p, &Default::default(), &dir, Format::Mp3, &Opts::default(), &pr, &ca, &mut |_| {}).unwrap();
    println!("{} · {:.1} s · pico {:.1} dB · {:.1} LUFS", d.path.display(), d.secs, d.peak_db, d.lufs);
}
