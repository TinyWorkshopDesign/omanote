//! On Omarchy, Omanote wears the system theme: the current palette is read
//! from `~/.config/omarchy/current/theme` and re-read when the user switches
//! theme (`omarchy-theme-set`), so the app follows along live.
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
    let home = std::env::var_os("HOME")?;
    let dir = Path::new(&home).join(".config/omarchy/current/theme");
    dir.exists().then_some(dir)
}

fn parse_colors_toml(text: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.starts_with('[') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else { continue };
        let v = v.trim().trim_matches('"');
        if !v.is_empty() {
            out.insert(k.trim().to_string(), v.to_string());
        }
    }
    out
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
    let background = colors.get("background")?.clone();
    let mode = colors
        .get("mode")
        .cloned()
        .unwrap_or_else(|| if luminance(&background) > 0.5 { "light".into() } else { "dark".into() });
    let name = std::fs::canonicalize(&dir)
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
        .unwrap_or_else(|| "omarchy".into());
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
    fn detects_light_backgrounds() {
        assert!(luminance("#ffffff") > 0.5);
        assert!(luminance("#1a1b26") < 0.5);
    }
}
