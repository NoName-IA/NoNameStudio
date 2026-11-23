//! Convierte proyectos viejos (`.nns.json`) al formato `.nns` con identificador. No borra el original.
//! Uso: cargo run --example convertir -- <archivo.nns.json> [más archivos…]
fn main() {
    for a in std::env::args().skip(1) {
        let origen = std::path::PathBuf::from(&a);
        let Some(p) = std::fs::read_to_string(&origen).ok().and_then(|s| noname_studio::project::de_archivo(&s)) else {
            eprintln!("NO es un proyecto: {a}");
            continue;
        };
        let nombre = origen.file_name().unwrap().to_string_lossy().trim_end_matches(".json").trim_end_matches(".nns").to_string();
        let destino = origen.with_file_name(format!("{nombre}.nns"));
        std::fs::write(&destino, noname_studio::project::a_archivo(&p).unwrap()).unwrap();
        // comprobación: se vuelve a leer igual
        let otra = noname_studio::project::de_archivo(&std::fs::read_to_string(&destino).unwrap()).expect("relectura");
        assert_eq!(serde_json::to_value(&p).unwrap(), serde_json::to_value(&otra).unwrap());
        println!("OK {}", destino.display());
    }
}
