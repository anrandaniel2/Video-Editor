//! Render every brush in the library with the same gesture, side by side, into one PNG.
//!
//! This exists to look at the thing the complaint was about. A unit test can say two strokes
//! differ byte for byte; only an image can say whether the difference is the one the panel prom-
//! ises. Run it with `cargo run --example brush_sheet -p filmcraft-android`, and it writes
//! `brush_sheet.png` in the workspace root.
//!
//! The path taken is the studio's own: `tools.setBrush` names the brush, then `paint.stroke`
//! draws with whatever the engine stored -- no internal helpers, no shortcut into the stamper.

use filmcraft_android::photocraft::{commands, compose, model::Document, Session};

/// Write RGBA to a PNG, with no colour management: what the engine composited is what is shown.
fn write_png(path: &str, rgba: &[u8], w: u32, h: u32) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(rgba).map_err(|e| e.to_string())?;
    Ok(())
}

/// Write one grey channel, which is a third of the bytes of the colour sheet.
fn write_gray_png(path: &str, gray: &[u8], w: u32, h: u32) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    encoder.set_color(png::ColorType::Grayscale);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(gray).map_err(|e| e.to_string())?;
    Ok(())
}

fn main() -> Result<(), String> {
    let names = commands::execute(&mut Session::new(), "brush.presets.list", serde_json::json!({}))
        .map_err(|e| e.to_string())?;
    let names: Vec<String> = names["presets"]
        .as_array()
        .ok_or("no presets")?
        .iter()
        .map(|p| p["name"].as_str().unwrap_or_default().to_string())
        .collect();

    // One row per brush: a wide, short canvas so the gesture is a long wave, at a size the
    // differences in tip and spacing can actually show.
    let (w, h) = (360u32, 64u32);
    let mut sheet = vec![255u8; (w * h * 4 * names.len() as u32) as usize];

    for (row, name) in names.iter().enumerate() {
        let mut s = Session::new();
        s.push(Document::new("Brush sheet", w, h, "#FFFFFF").map_err(|e| e.to_string())?);
        s.select_index(0);
        commands::execute(&mut s, "layer.new.layer", serde_json::json!({}))
            .map_err(|e| e.to_string())?;
        commands::execute(&mut s, "tools.setBrush", serde_json::json!({ "preset": name, "size": 22.0 }))
            .map_err(|e| e.to_string())?;
        // A wave, so a brush that follows the hand bends and a flat tip shows its width turning.
        let mut points = Vec::new();
        for i in 0..=60 {
            let t = i as f64 / 60.0;
            let x = 16.0 + t * f64::from(w - 32);
            let y = f64::from(h) / 2.0 + (t * std::f64::consts::TAU).sin() * f64::from(h) * 0.22;
            points.push(serde_json::json!([x, y]));
        }
        commands::execute(
            &mut s,
            "paint.stroke",
            serde_json::json!({ "points": points, "color": "#141414", "size": 22.0 }),
        )
        .map_err(|e| e.to_string())?;

        let px = compose::render(&s.active().ok_or("no doc")?.doc);
        let dst_row = (names.len() - 1 - row) as u32; // the list reads top-down, the file bottom-up
        let start = (dst_row * h * w * 4) as usize;
        sheet[start..start + px.len()].copy_from_slice(&px);
    }

    // A second, smaller sheet: one grey channel, quarter scale, which is small enough to carry in
    // a check-run summary and still legible enough to judge a brush by.
    let rows = names.len() as u32;
    let small: Vec<u8> = (0..rows).flat_map(|row| {
        (0..h).step_by(4).flat_map(move |y| {
            (0..w).step_by(4).map(move |x| {
                let i = ((row * h + y) * w + x) as usize * 4;
                // What the eye reads: luminance, with the white background staying white.
                ((u32::from(sheet[i]) * 299 + u32::from(sheet[i + 1]) * 587 + u32::from(sheet[i + 2]) * 114)
                    / 1000) as u8
            })
        })
    }).collect();
    write_gray_png("brush_small.png", &small, w / 4, (h / 4) * rows)?;
    write_png("brush_sheet.png", &sheet, w, h * rows)?;
    println!("wrote brush_sheet.png: {w}x{h}, {} brushes", names.len());
    for (i, n) in names.iter().enumerate() {
        println!("  row {}: {n}", i + 1);
    }
    Ok(())
}
