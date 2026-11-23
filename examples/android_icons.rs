// Genera los iconos del lanzador de Android desde assets/logo_cel.png (diamante con fondo negro)
fn main() {
    let img = image::open("assets/logo_cel.png").expect("icono").to_rgba8();
    for (dir, s) in [("mdpi", 48u32), ("hdpi", 72), ("xhdpi", 96), ("xxhdpi", 144), ("xxxhdpi", 192)] {
        let d = format!("android/res/mipmap-{dir}");
        std::fs::create_dir_all(&d).unwrap();
        image::imageops::resize(&img, s, s, image::imageops::FilterType::Lanczos3).save(format!("{d}/ic_launcher.png")).unwrap();
    }
    println!("iconos listos");
}
