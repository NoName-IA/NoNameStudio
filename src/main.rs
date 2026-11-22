#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();
    // Modo consola para verificar el motor sin interfaz:
    //   NoNameStudio.exe --export <carpeta> [proyecto.json]
    if args.get(1).map(String::as_str) == Some("--export") {
        let dir = std::path::PathBuf::from(args.get(2).cloned().unwrap_or_else(|| ".".into()));
        let proj = args
            .get(3)
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(noname_studio::project::Project::demo);
        let (pr, ca) = (std::sync::atomic::AtomicU32::new(0), std::sync::atomic::AtomicBool::new(false));
        let t = std::time::Instant::now();
        for fmt in [noname_studio::export::Format::Mp3, noname_studio::export::Format::Wav] {
            match noname_studio::export::export(&proj, &Default::default(), &dir, fmt, &pr, &ca) {
                Ok((p, d, pk)) => println!("OK {} dur={:.2}s pico={:.2}dBFS t={:.2}s", p.display(), d, pk, t.elapsed().as_secs_f32()),
                Err(e) => {
                    eprintln!("ERROR {e}");
                    std::process::exit(1);
                }
            }
        }
        return;
    }
    let autotest = args.iter().any(|a| a == "--autotest");
    let plugins = args.iter().position(|a| a == "--plugins").and_then(|i| args.get(i + 1).cloned());
    if let Err(e) = noname_studio::run_desktop(autotest, plugins) {
        eprintln!("{e}");
    }
}
