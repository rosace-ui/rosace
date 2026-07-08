//! App icon generation for `tzr new` (D104/D106 scaffolding).
//!
//! The framework's default app icon (`assets/icon/tezzera_icon_tiles.svg`)
//! is bundled into the `tzr` binary at compile time and rasterized into
//! every platform's required icon format at `tzr new` time — no manual
//! asset work, no filesystem lookup outside the generated project.
//!
//! `.ico` (Windows) and `.icns` (macOS) are simple tag/length/data table
//! formats — hand-written here rather than pulling in extra crates for
//! them; `resvg`/`tiny-skia` (already a dependency for SVG→PNG) are the
//! only image tooling needed.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use resvg::{tiny_skia, usvg};

use super::new::Platform;

/// The framework's default app icon, embedded at compile time.
const ICON_SVG: &[u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../assets/icon/tezzera_icon_tiles.svg"));

/// Rasterizes the bundled icon to a square PNG of `size` x `size` pixels.
fn render_png(size: u32) -> Result<Vec<u8>, String> {
    let opt = usvg::Options::default();
    let tree = usvg::Tree::from_data(ICON_SVG, &opt)
        .map_err(|e| format!("failed to parse bundled icon SVG: {e}"))?;
    let src = tree.size();
    let mut pixmap = tiny_skia::Pixmap::new(size, size)
        .ok_or_else(|| format!("invalid icon size {size}"))?;
    let transform = tiny_skia::Transform::from_scale(
        size as f32 / src.width(),
        size as f32 / src.height(),
    );
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    pixmap.encode_png().map_err(|e| format!("failed to encode {size}x{size} PNG: {e}"))
}

fn write_png(path: impl AsRef<Path>, size: u32) -> Result<(), String> {
    let bytes = render_png(size)?;
    fs::write(&path, &bytes)
        .map_err(|e| format!("failed to write {}: {}", path.as_ref().display(), e))
}

/// Packs PNG-compressed images into a Windows `.ico` container. Modern
/// (Vista+) readers accept PNG payloads at any size — no BMP fallback
/// needed for a tool this new.
fn write_ico(path: impl AsRef<Path>, sizes: &[u32]) -> Result<(), String> {
    let mut pngs = Vec::with_capacity(sizes.len());
    for &size in sizes {
        pngs.push((size, render_png(size)?));
    }

    let mut buf = Vec::new();
    buf.extend_from_slice(&0u16.to_le_bytes()); // reserved
    buf.extend_from_slice(&1u16.to_le_bytes()); // type = icon
    buf.extend_from_slice(&(pngs.len() as u16).to_le_bytes());

    let mut offset = (6 + 16 * pngs.len()) as u32; // header + directory entries
    for (size, data) in &pngs {
        // 0 encodes "256" in the ICO directory entry format.
        let dim = if *size >= 256 { 0u8 } else { *size as u8 };
        buf.push(dim); // width
        buf.push(dim); // height
        buf.push(0);   // color count (0 = no palette)
        buf.push(0);   // reserved
        buf.extend_from_slice(&1u16.to_le_bytes());  // color planes
        buf.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
        buf.extend_from_slice(&(data.len() as u32).to_le_bytes());
        buf.extend_from_slice(&offset.to_le_bytes());
        offset += data.len() as u32;
    }
    for (_, data) in &pngs {
        buf.extend_from_slice(data);
    }

    fs::write(&path, &buf)
        .map_err(|e| format!("failed to write {}: {}", path.as_ref().display(), e))
}

/// Packs PNG images into a macOS `.icns` container — the exact tag set
/// `iconutil -c icns` produces from a standard 10-image `.iconset` folder
/// (traditional + `@2x` retina variants sharing pixel sizes where the
/// dimensions coincide, e.g. `icp5`/`ic11` are both 32x32).
fn write_icns(path: impl AsRef<Path>) -> Result<(), String> {
    const ENTRIES: &[(&[u8; 4], u32)] = &[
        (b"icp4", 16), (b"ic11", 32),
        (b"icp5", 32), (b"ic12", 64),
        (b"ic07", 128), (b"ic13", 256),
        (b"ic08", 256), (b"ic14", 512),
        (b"ic09", 512), (b"ic10", 1024),
    ];

    let mut cache: HashMap<u32, Vec<u8>> = HashMap::new();
    let mut body = Vec::new();
    for (tag, size) in ENTRIES {
        let data = match cache.get(size) {
            Some(d) => d.clone(),
            None => {
                let d = render_png(*size)?;
                cache.insert(*size, d.clone());
                d
            }
        };
        body.extend_from_slice(*tag);
        body.extend_from_slice(&((data.len() + 8) as u32).to_be_bytes());
        body.extend_from_slice(&data);
    }

    let mut buf = Vec::new();
    buf.extend_from_slice(b"icns");
    buf.extend_from_slice(&((body.len() + 8) as u32).to_be_bytes());
    buf.extend_from_slice(&body);

    fs::write(&path, &buf)
        .map_err(|e| format!("failed to write {}: {}", path.as_ref().display(), e))
}

/// iOS: a single 1024x1024 "universal" App Icon — the modern (Xcode 14+)
/// single-size `AppIcon.appiconset` format; Xcode derives every smaller
/// slot from it at build time. Sits next to `ios/Info.plist` today, ready
/// to drop into the real `Assets.xcassets` a future Xcode-project generator
/// (Phase 24 Step 2) produces.
fn write_ios_icon(app_dir: &Path) -> Result<(), String> {
    let dir = app_dir.join("ios").join("AppIcon.appiconset");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    write_png(dir.join("AppIcon-1024.png"), 1024)?;
    fs::write(
        dir.join("Contents.json"),
        r#"{
  "images" : [
    {
      "filename" : "AppIcon-1024.png",
      "idiom" : "universal",
      "platform" : "ios",
      "size" : "1024x1024"
    }
  ],
  "info" : {
    "author" : "tzr",
    "version" : 1
  }
}
"#,
    )
    .map_err(|e| e.to_string())
}

/// Android: legacy (pre-adaptive-icon) `mipmap-*` launcher icons at every
/// standard density. Valid on every Android version — the OS applies its
/// own mask/shape. True adaptive icons (separate foreground/background
/// layers via `mipmap-anydpi-v26`) need a foreground-only asset without our
/// icon's baked-in background rect; deferred until that art exists.
fn write_android_icons(app_dir: &Path) -> Result<(), String> {
    const DENSITIES: &[(&str, u32)] = &[
        ("mipmap-mdpi", 48),
        ("mipmap-hdpi", 72),
        ("mipmap-xhdpi", 96),
        ("mipmap-xxhdpi", 144),
        ("mipmap-xxxhdpi", 192),
    ];
    let res_dir = app_dir.join("android").join("app").join("src").join("main").join("res");
    for (dir_name, size) in DENSITIES {
        let dir = res_dir.join(dir_name);
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        write_png(dir.join("ic_launcher.png"), *size)?;
        write_png(dir.join("ic_launcher_round.png"), *size)?;
    }
    Ok(())
}

/// Desktop: `.icns` (macOS dock/Finder) + `.ico` (Windows taskbar/
/// shortcuts). Always generated — desktop is always in `NewOptions.platforms`.
fn write_desktop_icons(app_dir: &Path) -> Result<(), String> {
    let dir = app_dir.join("desktop");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    write_icns(dir.join("icon.icns"))?;
    write_ico(dir.join("icon.ico"), &[16, 32, 48, 256])?;
    Ok(())
}

/// Web: favicon + PWA/manifest icons, plus a minimal `site.webmanifest`.
/// `new::web_index_html` links these from the generated `index.html`.
fn write_web_icons(app_dir: &Path) -> Result<(), String> {
    let dir = app_dir.join("web");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    write_ico(dir.join("favicon.ico"), &[16, 32, 48])?;
    write_png(dir.join("apple-touch-icon.png"), 180)?;
    write_png(dir.join("icon-192.png"), 192)?;
    write_png(dir.join("icon-512.png"), 512)?;
    fs::write(
        dir.join("site.webmanifest"),
        r#"{
  "icons": [
    { "src": "icon-192.png", "sizes": "192x192", "type": "image/png" },
    { "src": "icon-512.png", "sizes": "512x512", "type": "image/png" }
  ]
}
"#,
    )
    .map_err(|e| e.to_string())
}

/// Generates every icon format the selected `platforms` need, rooted at
/// the scaffolded project directory `app_dir`. Called once from
/// `new::run`, after the rest of the project tree is written.
pub fn generate(app_dir: &Path, platforms: &[Platform]) -> Result<(), String> {
    if platforms.contains(&Platform::Ios) {
        write_ios_icon(app_dir)?;
    }
    if platforms.contains(&Platform::Android) {
        write_android_icons(app_dir)?;
    }
    write_desktop_icons(app_dir)?;
    if platforms.contains(&Platform::Web) {
        write_web_icons(app_dir)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_png_produces_a_valid_png_of_the_requested_size() {
        let bytes = render_png(64).expect("render should succeed");
        // PNG magic bytes.
        assert_eq!(&bytes[0..8], &[0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1A, b'\n']);
        let img = tiny_skia::Pixmap::decode_png(&bytes).expect("should decode back");
        assert_eq!((img.width(), img.height()), (64, 64));
    }

    #[test]
    fn write_ico_produces_a_well_formed_directory() {
        let dir = std::env::temp_dir().join(format!("tzr_icons_test_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.ico");
        write_ico(&path, &[16, 32]).expect("ico write should succeed");
        let bytes = fs::read(&path).unwrap();
        assert_eq!(u16::from_le_bytes([bytes[0], bytes[1]]), 0); // reserved
        assert_eq!(u16::from_le_bytes([bytes[2], bytes[3]]), 1); // type = icon
        assert_eq!(u16::from_le_bytes([bytes[4], bytes[5]]), 2); // 2 images
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn write_icns_starts_with_magic_and_reports_consistent_length() {
        let dir = std::env::temp_dir().join(format!("tzr_icns_test_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.icns");
        write_icns(&path).expect("icns write should succeed");
        let bytes = fs::read(&path).unwrap();
        assert_eq!(&bytes[0..4], b"icns");
        let declared_len = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;
        assert_eq!(declared_len, bytes.len());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn generate_writes_expected_files_for_all_platforms() {
        let dir = std::env::temp_dir().join(format!("tzr_gen_icons_test_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let platforms = [Platform::Desktop, Platform::Web, Platform::Ios, Platform::Android];
        generate(&dir, &platforms).expect("generate should succeed");

        assert!(dir.join("ios/AppIcon.appiconset/AppIcon-1024.png").exists());
        assert!(dir.join("ios/AppIcon.appiconset/Contents.json").exists());
        assert!(dir.join("android/app/src/main/res/mipmap-xxxhdpi/ic_launcher.png").exists());
        assert!(dir.join("desktop/icon.icns").exists());
        assert!(dir.join("desktop/icon.ico").exists());
        assert!(dir.join("web/favicon.ico").exists());
        assert!(dir.join("web/site.webmanifest").exists());

        fs::remove_dir_all(&dir).ok();
    }
}
