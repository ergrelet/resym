#[cfg(windows)]
fn main() {
    let target = std::env::var("TARGET").unwrap();
    if target.contains("wasm") {
        return;
    }

    // Convert ICO to raw pixels at build time. This avoids adding the `image` crate in `resym`
    // only to display an icon.
    let icon_bytes_path = std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/resources/resym_96.bin"
    ));
    // Only generate the binary file if it doesn't exist
    if !icon_bytes_path.exists() {
        let image_path = concat!(env!("CARGO_MANIFEST_DIR"), "/resources/resym_96.ico");
        let image = image::open(image_path)
            .expect("failed to open 96x96 ICO")
            .into_rgba8();
        let image_bytes = image.into_raw();

        std::fs::write(icon_bytes_path, image_bytes).expect("failed to save 96x96 icon bytes");
    }

    // Compile the resource file with metadata and an icon
    let mut res = winres::WindowsResource::new();
    res.set_icon("resources/resym_256.ico");
    res.compile().expect("Windows resource compilation failed");
}

#[cfg(unix)]
fn main() {}
