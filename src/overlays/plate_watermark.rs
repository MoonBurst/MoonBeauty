//! Visible plate watermark — exact match of the QML QuickshotEngine.lua
//! `apply_watermark` function.
//!
//! - Tile 240x220 with the watermark text
//! - Text rotated 30° to create the diagonal pattern
//! - Fill (100, 0, 255) at 0.008 alpha — essentially invisible
//! - Pointsize 20 Liberation-Sans-Bold
//! - Tiled across the whole image
//! - png compression-level=1
//!
//! Reveal via B−G channel differencing (since the plate is purple, blue
//! minus green is the strongest signal).

use std::path::PathBuf;

fn magick_or_convert() -> Option<&'static str> {
    if std::process::Command::new("magick")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        Some("magick")
    } else if std::process::Command::new("convert")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        Some("convert")
    } else {
        None
    }
}

fn private_dir() -> PathBuf {
    let rt = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
    let d = PathBuf::from(rt).join("mbs-watermark");
    let _ = std::fs::create_dir_all(&d);
    d
}

/// Build the watermark text that fills the 240x220 tile.
/// The QML version repeats the name with whitespace to fill horizontally.
fn watermark_text(text: &str) -> String {
    format!("{}        {}        {}", text, text, text)
}

/// Add the watermark. Matches QuickshotEngine.lua's apply_watermark exactly:
///
///     convert raw.png \
///       ( -size 240x220 xc:none -fill "rgba(100,0,255,0.008)" \
///         -font Liberation-Sans-Bold -pointsize 20 -gravity Center \
///         -annotate +0+0 "<repeated text>" \
///         -distort ScaleRotateTranslate 30 -write mpr:text +delete ) \
///       ( +clone -tile mpr:text -draw "color 0,0 reset" ) \
///       -compose Over -composite \
///       -define png:compression-level=1 out.png
pub fn add_plate_watermark(bytes: &[u8], name: &str) -> Result<Vec<u8>, String> {
    let bin = magick_or_convert().ok_or("ImageMagick not installed")?;

    let stamp = format!(
        "{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    );
    let dir = private_dir();
    let tmp_in = dir.join(format!("plate-in-{}.png", stamp));
    let tmp_out = dir.join(format!("plate-out-{}.png", stamp));
    std::fs::write(&tmp_in, bytes).map_err(|e| format!("write: {e}"))?;

    let tile_text = watermark_text(name);

    let args = [
        tmp_in.to_string_lossy().into_owned(),
        "(".into(),
        "-size".into(),
        "240x220".into(),
        "xc:none".into(),
        "-fill".into(),
        "rgba(100, 0, 255, 0.008)".into(),
        "-font".into(),
        "Liberation-Sans-Bold".into(),
        "-pointsize".into(),
        "20".into(),
        "-gravity".into(),
        "Center".into(),
        "-annotate".into(),
        "+0+0".into(),
        tile_text.clone(),
        "-distort".into(),
        "ScaleRotateTranslate".into(),
        "30".into(),
        "-write".into(),
        "mpr:text".into(),
        "+delete".into(),
        ")".into(),
        "(".into(),
        "+clone".into(),
        "-tile".into(),
        "mpr:text".into(),
        "-draw".into(),
        "color 0,0 reset".into(),
        ")".into(),
        "-compose".into(),
        "Over".into(),
        "-composite".into(),
        "-define".into(),
        "png:compression-level=1".into(),
        tmp_out.to_string_lossy().into_owned(),
    ];

    // Finding 07: bound ImageMagick resource use.
    let status = std::process::Command::new(bin)
        .args([
            "-limit", "memory", "512MiB",
            "-limit", "map", "1GiB",
            "-limit", "disk", "2GiB",
            "-limit", "width", "16384",
            "-limit", "height", "16384",
        ])
        .args(&args)
        .status()
        .map_err(|e| format!("spawn {bin}: {e}"))?;

    let result = if status.success() {
        std::fs::read(&tmp_out).map_err(|e| format!("read: {e}"))
    } else {
        Err(format!("{bin} exited nonzero"))
    };

    let _ = std::fs::remove_file(&tmp_in);
    let _ = std::fs::remove_file(&tmp_out);

    if result.is_ok() {
        eprintln!("[plate] {} (tiled 240x220 rotate 30°, alpha 0.008)", name);
    }
    result
}

/// Reveal the plate watermark by high-pass filtering the blue channel.
///
/// Exact port of QuickshotEngine.lua's C.reveal():
///
///     magick crop.png -channel B -separate \
///       ( +clone -blur 0x2 ) \
///       -compose Subtract -composite \
///       -auto-level -define png:compression-level=1 out.png
///
/// The plate is purple (100, 0, 255) at 0.008 alpha → ~2 units of blue
/// offset per tile pixel. The blur removes low-frequency background;
/// subtracting leaves only the high-frequency watermark pattern. Auto-level
/// stretches that faint signal to full 0–255 for legibility.
///
/// Always returns the reveal image. The user reads the diagonal text pattern
/// (or its absence) directly — same behavior as the QML build.
pub fn reveal(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let bin = magick_or_convert().ok_or("ImageMagick not installed")?;

    let stamp = format!(
        "{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    );
    let dir = private_dir();
    let tmp_in = dir.join(format!("reveal-in-{}.png", stamp));
    let tmp_out = dir.join(format!("REVEALED_{}.png", stamp));

    std::fs::write(&tmp_in, bytes).map_err(|e| format!("write: {e}"))?;

    // Finding 07: bound ImageMagick resource use.
    let mut cmd = std::process::Command::new(bin);
    cmd.args([
        "-limit", "memory", "512MiB",
        "-limit", "map", "1GiB",
        "-limit", "disk", "2GiB",
        "-limit", "width", "16384",
        "-limit", "height", "16384",
    ]);
    cmd.args([
        tmp_in.to_str().unwrap(),
        "-channel", "B", "-separate", "(", "+clone", "-blur", "0x2", ")",
        "-compose", "Subtract", "-composite", "-auto-level",
        "-define", "png:compression-level=1",
        tmp_out.to_str().unwrap(),
    ]);
    let status = cmd
        .status()
        .map_err(|e| format!("spawn {bin}: {e}"))?;

    let _ = std::fs::remove_file(&tmp_in);

    if !status.success() {
        let _ = std::fs::remove_file(&tmp_out);
        return Err(format!("{bin} exited nonzero"));
    }

    let result = std::fs::read(&tmp_out).map_err(|e| format!("read: {e}"))?;
    let _ = std::fs::remove_file(&tmp_out);

    // Diagnostic: compute signal strength as the standard deviation of the
    // auto-leveled image. High variance = distinctive pattern (watermark).
    // Low variance = flat noise (no watermark). Purely informational.
    if let Ok(img) = image::load_from_memory(&result) {
        let gray = img.to_luma8();
        let n = gray.pixels().count() as f64;
        if n > 0.0 {
            let mean: f64 = gray.pixels().map(|p| p[0] as f64).sum::<f64>() / n;
            let var: f64 = gray
                .pixels()
                .map(|p| (p[0] as f64 - mean).powi(2))
                .sum::<f64>()
                / n;
            let std_dev = var.sqrt();
            eprintln!(
                "[reveal] signal std_dev={:.1} (high variance = pattern present)",
                std_dev
            );
        }
    }

    Ok(result)
}
