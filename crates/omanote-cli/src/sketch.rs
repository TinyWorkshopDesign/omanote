//! Sketches drawn outside the app (the Omarchy bar panel): strokes in, an editable
//! sketch PNG out, in the same format as the app's web canvas (`Sketch.svelte`,
//! `sketchData.ts`): cropped to the ink with a 12 px margin, at 2x, on the theme
//! background, with the strokes in a zlib-compressed iTXt chunk `omanote-sketch`
//! (coordinates from the PNG's top-left corner, in CSS pixels).

use std::io::{Read, Write};

use serde::{Deserialize, Serialize};
use tiny_skia::{BlendMode, Color, FillRule, LineCap, LineJoin, Paint, PathBuilder, Pixmap, PixmapPaint, Stroke, Transform};

const KEYWORD: &str = "omanote-sketch";
const PAD: f32 = 12.0;
const SCALE: f32 = 2.0;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct StrokeData {
    /// "pen", "pencil", "marker", "fountain", or "erase" (a pixel-eraser pass).
    pub kind: String,
    /// #rrggbb ("" for "erase").
    #[serde(default)]
    pub color: String,
    /// Width in CSS pixels.
    pub width: f32,
    /// Pressure is real (a pen), not simulated.
    #[serde(default)]
    pub pen: bool,
    /// [x, y, pressure 0..1]
    pub pts: Vec<Vec<f32>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SketchData {
    pub v: u32,
    pub strokes: Vec<StrokeData>,
    /// The iPad's PencilKit drawing (base64); kept only by the app.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pencilkit: Option<String>,
}

pub struct Rendered {
    pub png: Vec<u8>,
    /// Width in CSS pixels: the `width` attribute of the note's `<img>` tag.
    pub width: u32,
}

fn opacity(kind: &str) -> f32 {
    match kind {
        "pencil" => 0.8,
        "marker" => 0.35,
        _ => 1.0,
    }
}

fn parse_hex(hex: &str) -> Option<Color> {
    let h = hex.trim().trim_start_matches('#');
    if h.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(h, 16).ok()?;
    Some(Color::from_rgba8((v >> 16) as u8, (v >> 8) as u8, v as u8, 255))
}

/// Renders the strokes (any coordinates) to a cropped sketch PNG with its strokes
/// inside. `None` when there is no ink.
pub fn render(strokes: &[StrokeData], background: &str) -> Result<Option<Rendered>, String> {
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for s in strokes.iter().filter(|s| s.kind != "erase") {
        for p in s.pts.iter().filter(|p| p.len() >= 2) {
            x0 = x0.min(p[0] - s.width);
            y0 = y0.min(p[1] - s.width);
            x1 = x1.max(p[0] + s.width);
            y1 = y1.max(p[1] + s.width);
        }
    }
    if x0 > x1 {
        return Ok(None);
    }
    let w = (x1 - x0 + PAD * 2.0).ceil();
    let h = (y1 - y0 + PAD * 2.0).ceil();
    let (dx, dy) = (PAD - x0, PAD - y0);
    let pw = (w * SCALE).round() as u32;
    let ph = (h * SCALE).round() as u32;

    // The strokes on a transparent layer first, so the eraser cuts the ink and not
    // the background.
    let mut layer = Pixmap::new(pw, ph).ok_or("sketch too large")?;
    let to_px = Transform::from_row(SCALE, 0.0, 0.0, SCALE, dx * SCALE, dy * SCALE);
    for s in strokes {
        let pts: Vec<_> = s.pts.iter().filter(|p| p.len() >= 2).collect();
        if pts.is_empty() {
            continue;
        }
        let mut paint = Paint { anti_alias: true, ..Paint::default() };
        if s.kind == "erase" {
            paint.blend_mode = BlendMode::Clear;
            paint.set_color(Color::BLACK);
        } else {
            let mut c = parse_hex(&s.color).unwrap_or(Color::BLACK);
            c.apply_opacity(opacity(&s.kind));
            paint.set_color(c);
        }
        if pts.len() == 1 {
            if let Some(dot) = PathBuilder::from_circle(pts[0][0], pts[0][1], (s.width / 2.0).max(0.5)) {
                layer.fill_path(&dot, &paint, FillRule::Winding, to_px, None);
            }
            continue;
        }
        let mut pb = PathBuilder::new();
        pb.move_to(pts[0][0], pts[0][1]);
        for p in &pts[1..] {
            pb.line_to(p[0], p[1]);
        }
        if let Some(path) = pb.finish() {
            let stroke = Stroke { width: s.width.max(0.5), line_cap: LineCap::Round, line_join: LineJoin::Round, ..Stroke::default() };
            layer.stroke_path(&path, &paint, &stroke, to_px, None);
        }
    }
    let mut out = Pixmap::new(pw, ph).ok_or("sketch too large")?;
    out.fill(parse_hex(background).unwrap_or(Color::WHITE));
    out.draw_pixmap(0, 0, layer.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
    let png = out.encode_png().map_err(|e| e.to_string())?;

    let round = |v: f32, k: f32| (v * k).round() / k;
    let data = SketchData {
        v: 1,
        strokes: strokes
            .iter()
            .map(|s| StrokeData {
                kind: s.kind.clone(),
                color: if s.kind == "erase" { String::new() } else { s.color.to_lowercase() },
                width: round(s.width, 10.0),
                pen: s.pen,
                pts: s
                    .pts
                    .iter()
                    .filter(|p| p.len() >= 2)
                    .map(|p| vec![round(p[0] + dx, 10.0), round(p[1] + dy, 10.0), round(*p.get(2).unwrap_or(&0.5), 100.0)])
                    .collect(),
            })
            .collect(),
        pencilkit: None,
    };
    Ok(Some(Rendered { png: with_sketch_data(&png, &data)?, width: w as u32 }))
}

// ------------------------------------------------------------------ PNG chunks

const SIGNATURE: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];

fn crc32(bytes: &[u8]) -> u32 {
    let mut c = 0xffff_ffffu32;
    for &b in bytes {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
        }
    }
    c ^ 0xffff_ffff
}

/// (type, offset of the length field, data) for each chunk up to IEND.
fn chunks(png: &[u8]) -> Option<Vec<(String, usize, &[u8])>> {
    if png.len() < 8 || png[..8] != SIGNATURE {
        return None;
    }
    let mut out = Vec::new();
    let mut at = 8;
    while at + 12 <= png.len() {
        let len = u32::from_be_bytes(png[at..at + 4].try_into().ok()?) as usize;
        let ty = String::from_utf8_lossy(&png[at + 4..at + 8]).into_owned();
        let data = png.get(at + 8..at + 8 + len)?;
        let end = ty == "IEND";
        out.push((ty, at, data));
        at += 12 + len;
        if end {
            break;
        }
    }
    Some(out)
}

fn keyword(data: &[u8]) -> &[u8] {
    data.iter().position(|&b| b == 0).map(|n| &data[..n]).unwrap_or(&[])
}

/// The PNG with the sketch's strokes added before IEND (replacing an older copy).
pub fn with_sketch_data(png: &[u8], data: &SketchData) -> Result<Vec<u8>, String> {
    let list = chunks(png).ok_or("not a PNG")?;
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    z.write_all(serde_json::to_string(data).map_err(|e| e.to_string())?.as_bytes()).map_err(|e| e.to_string())?;
    let text = z.finish().map_err(|e| e.to_string())?;
    // iTXt: keyword, NUL, compressed (1), method (0 = zlib), language, NUL, translated keyword, NUL, text.
    let mut typed = b"iTXt".to_vec();
    typed.extend_from_slice(KEYWORD.as_bytes());
    typed.extend_from_slice(&[0, 1, 0, 0, 0]);
    typed.extend_from_slice(&text);
    let mut chunk = ((typed.len() - 4) as u32).to_be_bytes().to_vec();
    chunk.extend_from_slice(&typed);
    chunk.extend_from_slice(&crc32(&typed).to_be_bytes());

    let mut out = SIGNATURE.to_vec();
    for (ty, at, d) in &list {
        if ty == "iTXt" && keyword(d) == KEYWORD.as_bytes() {
            continue;
        }
        if ty == "IEND" {
            out.extend_from_slice(&chunk);
        }
        out.extend_from_slice(&png[*at..*at + 12 + d.len()]);
    }
    Ok(out)
}

/// The strokes saved in a sketch PNG, or None (not a sketch, or one made before editing).
pub fn read_sketch_data(png: &[u8]) -> Option<SketchData> {
    let (_, _, d) = chunks(png)?.into_iter().find(|(ty, _, d)| ty == "iTXt" && keyword(d) == KEYWORD.as_bytes())?;
    let mut i = d.iter().position(|&b| b == 0)? + 1;
    let compressed = *d.get(i)? == 1;
    i += 2;
    i += d[i..].iter().position(|&b| b == 0)? + 1; // language
    i += d[i..].iter().position(|&b| b == 0)? + 1; // translated keyword
    let raw = &d[i..];
    let json = if compressed {
        let mut s = String::new();
        flate2::read::ZlibDecoder::new(raw).read_to_string(&mut s).ok()?;
        s
    } else {
        String::from_utf8(raw.to_vec()).ok()?
    };
    let data: SketchData = serde_json::from_str(&json).ok()?;
    (data.v == 1).then_some(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stroke(kind: &str, pts: &[[f32; 2]]) -> StrokeData {
        StrokeData {
            kind: kind.into(),
            color: if kind == "erase" { String::new() } else { "#FF0000".into() },
            width: 4.0,
            pen: false,
            pts: pts.iter().map(|p| vec![p[0], p[1], 0.5]).collect(),
        }
    }

    #[test]
    fn renders_cropped_at_2x_with_strokes_inside() {
        let strokes = vec![stroke("pen", &[[100.0, 50.0], [140.0, 70.0]]), stroke("erase", &[[120.0, 60.0]])];
        let r = render(&strokes, "#101315").unwrap().unwrap();
        // ink 96..144 x 46..74, plus a 12 px margin each side
        assert_eq!(r.width, 72);
        let pix = Pixmap::decode_png(&r.png).unwrap();
        assert_eq!((pix.width(), pix.height()), (144, 104));
        // a corner is the background
        let c = pix.pixel(0, 0).unwrap();
        assert_eq!((c.red(), c.green(), c.blue(), c.alpha()), (0x10, 0x13, 0x15, 255));

        let data = read_sketch_data(&r.png).unwrap();
        assert_eq!(data.strokes.len(), 2);
        // coordinates move to the PNG's corner: 100 - (96 - 12) = 16
        assert_eq!(data.strokes[0].pts[0], vec![16.0, 16.0, 0.5]);
        assert_eq!(data.strokes[0].color, "#ff0000");
        assert_eq!(data.strokes[1].color, "");
    }

    #[test]
    fn no_ink_no_png() {
        assert!(render(&[stroke("erase", &[[1.0, 1.0]])], "#ffffff").unwrap().is_none());
        assert!(render(&[], "#ffffff").unwrap().is_none());
    }

    #[test]
    fn replaces_an_older_chunk() {
        let r = render(&[stroke("pen", &[[0.0, 0.0], [10.0, 0.0]])], "#ffffff").unwrap().unwrap();
        let data = SketchData { v: 1, strokes: vec![], pencilkit: Some("abc".into()) };
        let again = with_sketch_data(&r.png, &data).unwrap();
        let n = chunks(&again).unwrap().iter().filter(|(t, ..)| t == "iTXt").count();
        assert_eq!(n, 1);
        assert_eq!(read_sketch_data(&again).unwrap(), data);
        assert!(Pixmap::decode_png(&again).is_ok());
    }
}
