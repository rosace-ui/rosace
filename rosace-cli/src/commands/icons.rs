//! App icon generation for `rsc new` (D104/D106 scaffolding).
//!
//! The framework's default app icon (`assets/icon/rosace_icon_tiles.svg`)
//! is bundled into the `rsc` binary at compile time and rasterized into
//! every platform's required icon format at `rsc new` time — no manual
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
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../assets/icon/rosace_icon_tiles.svg"));

/// The same icon with the background rect removed (transparent) — Android
/// adaptive icons composite a separate background (a flat color, below)
/// under a foreground layer that the system independently masks/zooms/
/// parallaxes; baking a background into the foreground defeats that.
const ICON_SVG_FOREGROUND: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../assets/icon/rosace_icon_tiles_foreground.svg"
));

/// The bundled icon's background fill (`assets/icon/rosace_icon_tiles.svg`'s
/// `rect`), reused as the Android adaptive icon's background color resource
/// so the two layers stay visually consistent with the flattened icon.
const ICON_BACKGROUND_COLOR: &str = "#071019";

/// Rasterizes `svg` to a square PNG of `size` x `size` pixels.
fn render_svg_png(svg: &[u8], size: u32) -> Result<Vec<u8>, String> {
    let opt = usvg::Options::default();
    let tree = usvg::Tree::from_data(svg, &opt)
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

/// Rasterizes the bundled (background-included) icon to a square PNG.
fn render_png(size: u32) -> Result<Vec<u8>, String> {
    render_svg_png(ICON_SVG, size)
}

fn write_png(path: impl AsRef<Path>, size: u32) -> Result<(), String> {
    let bytes = render_png(size)?;
    fs::write(&path, &bytes)
        .map_err(|e| format!("failed to write {}: {}", path.as_ref().display(), e))
}

fn write_svg_png(path: impl AsRef<Path>, svg: &[u8], size: u32) -> Result<(), String> {
    let bytes = render_svg_png(svg, size)?;
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
/// slot from it at build time. Lives under `ios/App/Assets.xcassets/`, the
/// real asset catalog the Phase 24 Step 2 Xcode project generator wires via
/// `ASSETCATALOG_COMPILER_APPICON_NAME = AppIcon`.
fn write_ios_icon(app_dir: &Path) -> Result<(), String> {
    let xcassets = app_dir.join("ios").join("App").join("Assets.xcassets");
    fs::create_dir_all(&xcassets).map_err(|e| e.to_string())?;
    fs::write(
        xcassets.join("Contents.json"),
        "{\n  \"info\" : {\n    \"author\" : \"rsc\",\n    \"version\" : 1\n  }\n}\n",
    )
    .map_err(|e| e.to_string())?;

    let dir = xcassets.join("AppIcon.appiconset");
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
    "author" : "rsc",
    "version" : 1
  }
}
"#,
    )
    .map_err(|e| e.to_string())
}

/// Android: legacy (pre-API-26) `mipmap-*` launcher icons at every standard
/// density — the OS applies its own mask/shape — PLUS real adaptive icons
/// (API 26+): a flat-color background (`ICON_BACKGROUND_COLOR`, the same
/// color the flattened icon's own background rect uses) composited under a
/// transparent-background foreground layer (`ICON_SVG_FOREGROUND`) that the
/// system independently masks, zooms, and parallaxes. Both are written so
/// the app looks right on every Android version, old and new.
fn write_android_icons(app_dir: &Path) -> Result<(), String> {
    // Legacy square launcher icon, one raster per density.
    const DENSITIES: &[(&str, u32)] = &[
        ("mipmap-mdpi", 48),
        ("mipmap-hdpi", 72),
        ("mipmap-xhdpi", 96),
        ("mipmap-xxhdpi", 144),
        ("mipmap-xxxhdpi", 192),
    ];
    // Adaptive icon foreground layer: standard 108/162/216/324/432dp sizes
    // (Android's adaptive-icon canvas is larger than the legacy launcher
    // icon to allow for system masking/zoom without clipping content).
    const ADAPTIVE_DENSITIES: &[(&str, u32)] = &[
        ("mipmap-mdpi", 108),
        ("mipmap-hdpi", 162),
        ("mipmap-xhdpi", 216),
        ("mipmap-xxhdpi", 324),
        ("mipmap-xxxhdpi", 432),
    ];

    let res_dir = app_dir.join("android").join("app").join("src").join("main").join("res");

    for (dir_name, size) in DENSITIES {
        let dir = res_dir.join(dir_name);
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        write_png(dir.join("ic_launcher.png"), *size)?;
        write_png(dir.join("ic_launcher_round.png"), *size)?;
    }

    for (dir_name, size) in ADAPTIVE_DENSITIES {
        let dir = res_dir.join(dir_name);
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        write_svg_png(dir.join("ic_launcher_foreground.png"), ICON_SVG_FOREGROUND, *size)?;
    }

    let values_dir = res_dir.join("values");
    fs::create_dir_all(&values_dir).map_err(|e| e.to_string())?;
    fs::write(
        values_dir.join("ic_launcher_background.xml"),
        format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
             <resources>\n    \
             <color name=\"ic_launcher_background\">{ICON_BACKGROUND_COLOR}</color>\n\
             </resources>\n"
        ),
    )
    .map_err(|e| e.to_string())?;

    let anydpi_dir = res_dir.join("mipmap-anydpi-v26");
    fs::create_dir_all(&anydpi_dir).map_err(|e| e.to_string())?;
    let adaptive_icon_xml = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
        <adaptive-icon xmlns:android=\"http://schemas.android.com/apk/res/android\">\n    \
        <background android:drawable=\"@color/ic_launcher_background\"/>\n    \
        <foreground android:drawable=\"@mipmap/ic_launcher_foreground\"/>\n\
        </adaptive-icon>\n";
    fs::write(anydpi_dir.join("ic_launcher.xml"), adaptive_icon_xml).map_err(|e| e.to_string())?;
    fs::write(anydpi_dir.join("ic_launcher_round.xml"), adaptive_icon_xml).map_err(|e| e.to_string())?;

    Ok(())
}

/// macOS: `.icns` for the dock/Finder — `new.rs`'s `macos_info_plist`
/// references it via `CFBundleIconFile`, and `package.rs::bundle_macos`
/// copies it into the built `.app`.
fn write_macos_icon(app_dir: &Path) -> Result<(), String> {
    let dir = app_dir.join("macos");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    write_icns(dir.join("icon.icns"))
}

/// Windows: `.ico` for the taskbar/shortcuts — `package.rs::bundle_windows`
/// copies it alongside the built `.exe`.
fn write_windows_icon(app_dir: &Path) -> Result<(), String> {
    let dir = app_dir.join("windows");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    write_ico(dir.join("icon.ico"), &[16, 32, 48, 256])
}

/// Linux: a plain 256x256 PNG — the standard `hicolor` icon theme size
/// desktop environments expect (`usr/share/icons/hicolor/256x256/apps/`).
fn write_linux_icon(app_dir: &Path) -> Result<(), String> {
    let dir = app_dir.join("linux");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    write_png(dir.join("icon.png"), 256)
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
    if platforms.contains(&Platform::MacOs) {
        write_macos_icon(app_dir)?;
    }
    if platforms.contains(&Platform::Windows) {
        write_windows_icon(app_dir)?;
    }
    if platforms.contains(&Platform::Linux) {
        write_linux_icon(app_dir)?;
    }
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
        let platforms = [
            Platform::MacOs, Platform::Windows, Platform::Linux,
            Platform::Web, Platform::Ios, Platform::Android,
        ];
        generate(&dir, &platforms).expect("generate should succeed");

        assert!(dir.join("ios/App/Assets.xcassets/Contents.json").exists());
        assert!(dir.join("ios/App/Assets.xcassets/AppIcon.appiconset/AppIcon-1024.png").exists());
        assert!(dir.join("ios/App/Assets.xcassets/AppIcon.appiconset/Contents.json").exists());
        assert!(dir.join("android/app/src/main/res/mipmap-xxxhdpi/ic_launcher.png").exists());
        assert!(dir.join("macos/icon.icns").exists());
        assert!(dir.join("windows/icon.ico").exists());
        assert!(dir.join("linux/icon.png").exists());
        assert!(dir.join("web/favicon.ico").exists());
        assert!(dir.join("web/site.webmanifest").exists());

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn generate_only_writes_folders_for_selected_platforms() {
        let dir = std::env::temp_dir().join(format!("tzr_gen_icons_partial_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        generate(&dir, &[Platform::MacOs]).expect("generate should succeed");

        assert!(dir.join("macos/icon.icns").exists());
        assert!(!dir.join("windows").exists());
        assert!(!dir.join("linux").exists());
        assert!(!dir.join("ios").exists());
        assert!(!dir.join("android").exists());
        assert!(!dir.join("web").exists());

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn android_adaptive_icon_layers_are_generated() {
        let dir = std::env::temp_dir().join(format!("tzr_gen_adaptive_test_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        write_android_icons(&dir).expect("android icon generation should succeed");

        let res = dir.join("android/app/src/main/res");
        assert!(res.join("mipmap-anydpi-v26/ic_launcher.xml").exists());
        assert!(res.join("mipmap-anydpi-v26/ic_launcher_round.xml").exists());
        assert!(res.join("values/ic_launcher_background.xml").exists());
        assert!(res.join("mipmap-xxxhdpi/ic_launcher_foreground.png").exists());

        // Adaptive foreground layer must be transparent where the diamond
        // cluster doesn't cover — that's the whole point of splitting it
        // from the background. Legacy ic_launcher.png must NOT be (it's the
        // flattened icon with the background rect baked in).
        let fg_bytes = fs::read(res.join("mipmap-mdpi/ic_launcher_foreground.png")).unwrap();
        let fg = tiny_skia::Pixmap::decode_png(&fg_bytes).unwrap();
        let corner = fg.pixel(0, 0).unwrap();
        assert_eq!(corner.alpha(), 0, "foreground corner should be transparent");

        // The bundled icon's background rect has rounded corners (rx=114 on
        // a 512 canvas), so (0,0) is transparent on the legacy icon too —
        // sample the center instead, which is always inside the rounded
        // rect regardless of icon size.
        let legacy_bytes = fs::read(res.join("mipmap-mdpi/ic_launcher.png")).unwrap();
        let legacy = tiny_skia::Pixmap::decode_png(&legacy_bytes).unwrap();
        let (cx, cy) = (legacy.width() / 2, legacy.height() / 2);
        let legacy_center = legacy.pixel(cx, cy).unwrap();
        assert_eq!(legacy_center.alpha(), 255, "legacy icon center should be opaque");

        let bg_xml = fs::read_to_string(res.join("values/ic_launcher_background.xml")).unwrap();
        assert!(bg_xml.contains(ICON_BACKGROUND_COLOR));

        fs::remove_dir_all(&dir).ok();
    }
}
