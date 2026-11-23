// Incrusta el icono «NoName Studio» y los datos de versión en el .exe de Windows.
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=assets/NoName Studio.png");
    println!("cargo:rerun-if-changed=build.rs");
    let target = std::env::var("TARGET").unwrap_or_default();
    if !target.contains("windows") {
        return;
    }
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let ico = out.join("noname_studio.ico");

    // PNG → ICO con varios tamaños (16…256)
    let img = image::open("assets/NoName Studio.png").expect("icono").to_rgba8();
    let frames: Vec<image::codecs::ico::IcoFrame> = [16u32, 24, 32, 48, 64, 128, 256]
        .iter()
        .map(|&s| {
            let r = image::imageops::resize(&img, s, s, image::imageops::FilterType::Lanczos3);
            image::codecs::ico::IcoFrame::as_png(r.as_raw(), s, s, image::ExtendedColorType::Rgba8).expect("frame")
        })
        .collect();
    let f = std::fs::File::create(&ico).unwrap();
    image::codecs::ico::IcoEncoder::new(f).encode_images(&frames).expect("ico");

    // icono de los documentos .nns: hoja con el diamante y la etiqueta «NNS» (distinto del programa)
    let doc = icono_documento(&img);
    let ico_doc = out.join("proyecto_nns.ico");
    let frames: Vec<image::codecs::ico::IcoFrame> = [16u32, 24, 32, 48, 64, 128, 256]
        .iter()
        .map(|&s| {
            let r = image::imageops::resize(&doc, s, s, image::imageops::FilterType::Lanczos3);
            image::codecs::ico::IcoFrame::as_png(r.as_raw(), s, s, image::ExtendedColorType::Rgba8).expect("frame")
        })
        .collect();
    image::codecs::ico::IcoEncoder::new(std::fs::File::create(&ico_doc).unwrap()).encode_images(&frames).expect("ico");
    let _ = doc.save(out.join("proyecto_nns.png"));

    let ver = std::env::var("CARGO_PKG_VERSION").unwrap();
    let v: Vec<&str> = ver.split('.').collect();
    let rc = out.join("app.rc");
    std::fs::write(
        &rc,
        format!(
            r#"1 ICON "{ico}"
2 ICON "{ico_doc}"
1 VERSIONINFO
FILEVERSION {a},{b},{c},0
PRODUCTVERSION {a},{b},{c},0
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "0C0A04B0"
    BEGIN
      VALUE "CompanyName", "NoName Studio"
      VALUE "FileDescription", "NoName Studio"
      VALUE "ProductName", "NoName Studio"
      VALUE "FileVersion", "{ver}"
      VALUE "ProductVersion", "{ver}"
      VALUE "OriginalFilename", "NoNameStudio.exe"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x0C0A, 1200
  END
END
"#,
            ico = ico.display().to_string().replace('\\', "/"),
            ico_doc = ico_doc.display().to_string().replace('\\', "/"),
            a = v[0],
            b = v[1],
            c = v[2],
        ),
    )
    .unwrap();

    let obj = out.join("app_res.o");
    let st = std::process::Command::new("windres")
        .args(["-O", "coff", "-i"])
        .arg(&rc)
        .arg("-o")
        .arg(&obj)
        .status();
    match st {
        Ok(s) if s.success() => println!("cargo:rustc-link-arg-bins={}", obj.display()),
        _ => println!("cargo:warning=windres no disponible: el .exe quedará sin icono incrustado"),
    }
}

/// Dibuja el icono de documento a 1024 px (se reduce después, así los bordes quedan suaves).
fn icono_documento(logo: &image::RgbaImage) -> image::RgbaImage {
    use image::{Rgba, RgbaImage};
    const T: u32 = 1024;
    let k = T as f32 / 256.0;
    let mut im = RgbaImage::from_pixel(T, T, Rgba([0, 0, 0, 0]));
    // hoja (en coordenadas de 256): x 40..216, y 12..244, esquina doblada de 52
    let (x0, x1, y0, y1, dob) = (40.0, 216.0, 12.0, 244.0, 52.0);
    let borde = 3.0;
    let dentro = |x: f32, y: f32, m: f32| x >= x0 + m && x <= x1 - m && y >= y0 + m && y <= y1 - m && (x - (x1 - dob)) - (y - y0) <= -m * 1.414;
    for py in 0..T {
        for px in 0..T {
            let (x, y) = (px as f32 / k, py as f32 / k);
            if dentro(x, y, 0.0) {
                let c = if dentro(x, y, borde) { 255 } else { 150 };
                im.put_pixel(px, py, Rgba([c, c, c, 255]));
            }
            // pliegue de la esquina
            let (fx, fy) = (x - (x1 - dob), y - y0);
            if fx >= 0.0 && fy <= dob && fx <= dob && fy >= 0.0 && fx - fy >= 0.0 {
                let c = if fx - fy < borde * 1.414 || fx > dob - borde || fy < borde { 150 } else { 222 };
                im.put_pixel(px, py, Rgba([c, c, c, 255]));
            }
        }
    }
    // diamante en el centro de la hoja
    let lado = (118.0 * k) as u32;
    let d = image::imageops::resize(logo, lado, lado, image::imageops::FilterType::Lanczos3);
    image::imageops::overlay(&mut im, &d, ((128.0 - 59.0) * k) as i64, (46.0 * k) as i64);
    // etiqueta oscura con «NNS»
    let (bx0, bx1, by0, by1, r) = (58.0, 198.0, 180.0, 226.0, 9.0);
    for py in (by0 * k) as u32..(by1 * k) as u32 {
        for px in (bx0 * k) as u32..(bx1 * k) as u32 {
            let (x, y) = (px as f32 / k, py as f32 / k);
            let cx = x.clamp(bx0 + r, bx1 - r);
            let cy = y.clamp(by0 + r, by1 - r);
            if (x - cx).powi(2) + (y - cy).powi(2) <= r * r {
                im.put_pixel(px, py, Rgba([22, 22, 24, 255]));
            }
        }
    }
    // letras de 5x7 en píxeles gruesos
    const N: [u8; 7] = [0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001];
    const S: [u8; 7] = [0b01110, 0b10001, 0b10000, 0b01110, 0b00001, 0b10001, 0b01110];
    let celda = 4.6;
    let ancho = 3.0 * 5.0 * celda + 2.0 * 2.0 * celda;
    let mut lx = 128.0 - ancho / 2.0;
    let ly = (by0 + by1) / 2.0 - 3.5 * celda;
    for g in [N, N, S] {
        for (fila, bits) in g.iter().enumerate() {
            for col in 0..5 {
                if bits & (1 << (4 - col)) != 0 {
                    let (ax, ay) = (lx + col as f32 * celda, ly + fila as f32 * celda);
                    for py in (ay * k) as u32..((ay + celda) * k) as u32 {
                        for px in (ax * k) as u32..((ax + celda) * k) as u32 {
                            im.put_pixel(px, py, Rgba([235, 235, 238, 255]));
                        }
                    }
                }
            }
        }
        lx += 7.0 * celda;
    }
    image::imageops::resize(&im, 256, 256, image::imageops::FilterType::Lanczos3)
}
