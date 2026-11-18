//! Reads waybar's stylesheet so the board looks like part of the bar: its
//! font and colors. The parser came from Tablet.
//!
//! These are small, forgiving parsers. Waybar accepts more CSS than they
//! understand; anything they cannot read falls back to the built-in theme.

use std::collections::BTreeMap;
use std::path::PathBuf;

/// What the board borrows from the bar's stylesheet.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BarTheme {
    pub font: Option<String>,
    pub background: Option<String>,
    pub foreground: Option<String>,
    /// The first border color on a module rule.
    pub border: Option<String>,
    /// The background of the first `:hover` rule.
    pub hover: Option<String>,
    /// Every `@define-color`.
    pub colors: BTreeMap<String, String>,
}

pub fn style_path() -> PathBuf {
    crate::paths::waybar_dir().join("style.css")
}

pub fn load_theme() -> Option<BarTheme> {
    std::fs::read_to_string(style_path())
        .ok()
        .map(|t| parse_style(&t))
}

fn strip_css_comments(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find("/*") {
        out.push_str(&rest[..i]);
        match rest[i + 2..].find("*/") {
            Some(j) => rest = &rest[i + 2 + j + 2..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

/// The first color token in a CSS value: a hex color, `rgb()`/`rgba()`, or
/// an `@name` reference resolved against `colors`.
fn first_color(value: &str, colors: &BTreeMap<String, String>) -> Option<String> {
    for word in value.split_whitespace() {
        let w = word.trim_end_matches([';', ',']);
        if w.starts_with('#') && w.len() >= 4 && w[1..].chars().all(|c| c.is_ascii_hexdigit()) {
            return Some(w.to_string());
        }
        if let Some(name) = w.strip_prefix('@')
            && let Some(c) = colors.get(name)
        {
            return Some(c.clone());
        }
    }
    let lower = value.to_ascii_lowercase();
    for f in ["rgba(", "rgb("] {
        if let Some(i) = lower.find(f)
            && let Some(j) = value[i..].find(')')
        {
            return Some(value[i..i + j + 1].to_string());
        }
    }
    None
}

pub fn parse_style(text: &str) -> BarTheme {
    let css = strip_css_comments(text);
    let mut theme = BarTheme::default();

    for line in css.lines() {
        let l = line.trim();
        if let Some(rest) = l.strip_prefix("@define-color") {
            let mut it = rest.trim().splitn(2, char::is_whitespace);
            if let (Some(name), Some(value)) = (it.next(), it.next()) {
                theme.colors.insert(
                    name.to_string(),
                    value.trim().trim_end_matches(';').trim().to_string(),
                );
            }
        }
    }

    for block in css.split('}') {
        let Some((sel, body)) = block.split_once('{') else {
            continue;
        };
        // Statements before the selector (`@define-color x y;`) are not part of it.
        let sel = sel.rsplit(';').next().unwrap_or(sel);
        let selectors: Vec<&str> = sel
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty() && !s.starts_with('@'))
            .collect();
        if selectors.is_empty() {
            continue;
        }
        let decls: Vec<(String, String)> = body
            .split(';')
            .filter_map(|d| d.split_once(':'))
            .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
            .collect();
        let get = |name: &str| {
            decls
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.clone())
        };

        if (selectors.contains(&"*") || selectors.contains(&"window#waybar"))
            && let Some(f) = get("font-family")
        {
            theme.font.get_or_insert(f);
        }
        if selectors.contains(&"window#waybar") {
            if let Some(c) = get("background")
                .or_else(|| get("background-color"))
                .and_then(|v| first_color(&v, &theme.colors))
            {
                theme.background = Some(c);
            }
            if let Some(c) = get("color").and_then(|v| first_color(&v, &theme.colors)) {
                theme.foreground = Some(c);
            }
        }
        if theme.border.is_none()
            && selectors
                .iter()
                .any(|s| s.starts_with('#') && !s.starts_with("window"))
        {
            theme.border = decls
                .iter()
                .filter(|(k, _)| {
                    k.starts_with("border") && !k.contains("radius") && !k.contains("width")
                })
                .find_map(|(_, v)| first_color(v, &theme.colors));
        }
        if theme.hover.is_none() && selectors.iter().any(|s| s.ends_with(":hover")) {
            theme.hover = get("background")
                .or_else(|| get("background-color"))
                .and_then(|v| first_color(&v, &theme.colors));
        }
    }

    // A font list keeps its first family: "PragmataPro Liga", monospace.
    theme.font = theme.font.and_then(|f| {
        let first = f
            .split(',')
            .next()?
            .trim()
            .trim_matches(['"', '\''])
            .to_string();
        (!first.is_empty()).then_some(first)
    });
    theme
}
