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

    let ver = std::env::var("CARGO_PKG_VERSION").unwrap();
    let v: Vec<&str> = ver.split('.').collect();
    let rc = out.join("app.rc");
    std::fs::write(
        &rc,
        format!(
            r#"1 ICON "{ico}"
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
