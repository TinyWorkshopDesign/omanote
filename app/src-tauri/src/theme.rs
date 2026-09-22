//! On Omarchy, Omanote wears the system theme: the current palette is read
//! from `~/.local/state/omarchy/current/theme` (Omarchy 4; older releases used
//! `~/.config/omarchy/current/theme`) and re-read when the user switches theme
//! (`omarchy-theme-set`), so the app follows along live.
//!
//! Everywhere else the app falls back to the themes bundled in the UI.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct OmarchyTheme {
    /// Directory name of the theme, e.g. "tokyo-night".
    pub name: String,
    /// "dark" or "light".
    pub mode: String,
    /// Palette keys as used by Omarchy (background, foreground, accent, …).
    pub colors: HashMap<String, String>,
}

fn theme_dir() -> Option<PathBuf> {
    let home = PathBuf::from(std::env::var_os("HOME")?);
    let state = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home.join(".local/state"));
    [state.join("omarchy/current/theme"), home.join(".config/omarchy/current/theme")]
        .into_iter()
        .find(|d| d.is_dir())
}

/// Omarchy 4 copies the theme into `current/theme` and writes its name next to it;
/// older releases made `current/theme` a symlink to the theme directory.
fn theme_name(dir: &Path) -> String {
    dir.parent()
        .and_then(|p| std::fs::read_to_string(p.join("theme.name")).ok())
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
        .or_else(|| {
            std::fs::canonicalize(dir)
                .ok()
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
                .filter(|n| n != "theme")
        })
        .unwrap_or_else(|| "omarchy".into())
}

fn parse_colors_toml(text: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.starts_with('[') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else { continue };
        let v = v.trim();
        // `key = "value"   # comment`: keep only what is inside the quotes.
        let v = match v.strip_prefix('"') {
            Some(rest) => rest.split('"').next().unwrap_or(""),
            None => v.split('#').next().unwrap_or("").trim(),
        };
        if !v.is_empty() {
            out.insert(k.trim().to_string(), v.to_string());
        }
    }
    out
}

fn rgb(hex: &str) -> Option<[f32; 3]> {
    let h = hex.trim_start_matches('#');
    if h.len() < 6 {
        return None;
    }
    let c = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok().map(|v| v as f32);
    Some([c(0)?, c(2)?, c(4)?])
}

/// `a` moved towards `b` by `t` (0..1), as `#rrggbb`.
fn mix(a: &str, b: &str, t: f32) -> Option<String> {
    let (a, b) = (rgb(a)?, rgb(b)?);
    let m = |i: usize| (a[i] + (b[i] - a[i]) * t).round().clamp(0.0, 255.0) as u8;
    Some(format!("#{:02x}{:02x}{:02x}", m(0), m(1), m(2)))
}

/// Themes made with generators (e.g. Aether) ship an ANSI palette (`color0`…`color15`,
/// `selection_background`) instead of Omarchy's named keys. Fill in the named keys the
/// UI uses, derived from what the theme has, without overriding any it defines.
fn complete_palette(c: &mut HashMap<String, String>) {
    const ANSI: [(&str, &str); 12] = [
        ("red", "color1"), ("green", "color2"), ("yellow", "color3"), ("blue", "color4"),
        ("magenta", "color5"), ("cyan", "color6"), ("bright_red", "color9"),
        ("bright_green", "color10"), ("bright_yellow", "color11"), ("bright_blue", "color12"),
        ("bright_magenta", "color13"), ("bright_cyan", "color14"),
    ];
    for (name, ansi) in ANSI {
        if let Some(v) = c.get(ansi).cloned() {
            c.entry(name.into()).or_insert(v);
        }
    }
    let (Some(bg), Some(fg)) = (c.get("background").cloned(), c.get("foreground").cloned()) else {
        return;
    };
    let light = luminance(&bg) > 0.5;
    let edge = if light { "#ffffff" } else { "#000000" };
    let derived = [
        ("dark_background", mix(&bg, edge, 0.25)),
        ("lighter_background", mix(&bg, &fg, 0.07)),
        // Not `selection_background`: that is the (often vivid) text selection colour.
        ("selection", mix(&bg, &fg, 0.14)),
        ("muted", mix(&bg, &fg, 0.28)),
        ("dark_foreground", c.get("color8").cloned().or_else(|| mix(&fg, &bg, 0.45))),
    ];
    for (name, v) in derived {
        if let Some(v) = v {
            c.entry(name.into()).or_insert(v);
        }
    }
    if let Some(v) = c.get("blue").cloned() {
        c.entry("accent".into()).or_insert(v);
    }
}

/// Older Omarchy themes ship no `colors.toml`; derive a palette from Alacritty's.
fn parse_alacritty(text: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let mut section = String::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            section = line.trim_matches(['[', ']']).to_string();
            continue;
        }
        let Some((k, v)) = line.split_once('=') else { continue };
        let (k, v) = (k.trim(), v.trim().trim_matches('"'));
        match (section.as_str(), k) {
            ("colors.primary", "background") => out.insert("background".into(), v.into()),
            ("colors.primary", "foreground") => out.insert("foreground".into(), v.into()),
            ("colors.normal", _) | ("colors.bright", _) => out.insert(
                if section.ends_with("bright") { format!("bright_{k}") } else { k.to_string() },
                v.to_string(),
            ),
            ("colors.selection", "background") => out.insert("selection".into(), v.into()),
            _ => None,
        };
    }
    if let Some(blue) = out.get("blue").cloned() {
        out.entry("accent".into()).or_insert(blue);
    }
    out
}

fn luminance(hex: &str) -> f32 {
    let h = hex.trim_start_matches('#');
    if h.len() < 6 {
        return 0.0;
    }
    let c = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).unwrap_or(0) as f32 / 255.0;
    0.2126 * c(0) + 0.7152 * c(2) + 0.0722 * c(4)
}

/// Reads the theme Omarchy is currently using, if this is an Omarchy system.
pub fn current() -> Option<OmarchyTheme> {
    let dir = theme_dir()?;
    let mut colors = std::fs::read_to_string(dir.join("colors.toml"))
        .ok()
        .map(|t| parse_colors_toml(&t))
        .unwrap_or_default();
    if !colors.contains_key("background") {
        colors = parse_alacritty(&std::fs::read_to_string(dir.join("alacritty.toml")).ok()?);
    }
    complete_palette(&mut colors);
    let background = colors.get("background")?.clone();
    let mode = colors
        .get("mode")
        .cloned()
        .unwrap_or_else(|| if luminance(&background) > 0.5 { "light".into() } else { "dark".into() });
    let name = theme_name(&dir);
    Some(OmarchyTheme { name, mode, colors })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_colors_toml() {
        let c = parse_colors_toml("mode = \"dark\"\n\naccent = \"#7aa2f7\"\n# comment\nbackground = \"#1a1b26\"\n");
        assert_eq!(c["accent"], "#7aa2f7");
        assert_eq!(c["background"], "#1a1b26");
        assert_eq!(c.get("# comment"), None);
    }

    #[test]
    fn falls_back_to_alacritty() {
        let c = parse_alacritty("[colors.primary]\nbackground = \"#1a1b26\"\nforeground = \"#a9b1d6\"\n[colors.normal]\nblue = \"#7aa2f7\"\n");
        assert_eq!(c["background"], "#1a1b26");
        assert_eq!(c["accent"], "#7aa2f7");
    }

    #[test]
    fn strips_inline_comments() {
        let c = parse_colors_toml("accent = \"#36a166\"   # green\ncolor0  = \"#22221b\"  \n");
        assert_eq!(c["accent"], "#36a166");
        assert_eq!(c["color0"], "#22221b");
    }

    #[test]
    fn completes_ansi_palettes() {
        let mut c = parse_colors_toml(
            "accent = \"#36a166\"\nforeground = \"#dcd9d6\"\nbackground = \"#22221b\"\n\
             selection_background = \"#5f9182\"\ncolor1 = \"#ba6236\"\ncolor2 = \"#7d9726\"\ncolor8 = \"#6c6b5a\"\n",
        );
        complete_palette(&mut c);
        assert_eq!(c["red"], "#ba6236");
        assert_eq!(c["green"], "#7d9726");
        assert_eq!(c["dark_foreground"], "#6c6b5a");
        assert_eq!(c["accent"], "#36a166");
        assert_ne!(c["selection"], "#5f9182");
        assert!(luminance(&c["dark_background"]) < luminance("#22221b"));
    }

    #[test]
    fn keeps_named_keys() {
        let mut c = parse_colors_toml("background = \"#1a1b26\"\nforeground = \"#a9b1d6\"\nselection = \"#292e42\"\n");
        complete_palette(&mut c);
        assert_eq!(c["selection"], "#292e42");
    }

    #[test]
    fn detects_light_backgrounds() {
        assert!(luminance("#ffffff") > 0.5);
        assert!(luminance("#1a1b26") < 0.5);
    }
}
