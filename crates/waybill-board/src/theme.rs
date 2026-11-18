//! The board's colors. One palette feeds both the stylesheet (as
//! `@define-color` tokens) and the charts, which draw with cairo and cannot
//! read CSS. With `theme.from = "waybar"` the bar's own colors and font
//! replace the built-in ones, so the board looks like it belongs to the bar.

use std::cell::RefCell;
use waybill_core::config::Config;
use waybill_core::waybar::BarTheme;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgba(pub f64, pub f64, pub f64, pub f64);

impl Rgba {
    pub fn css(self) -> String {
        format!(
            "rgba({}, {}, {}, {:.3})",
            (self.0 * 255.0).round() as u8,
            (self.1 * 255.0).round() as u8,
            (self.2 * 255.0).round() as u8,
            self.3
        )
    }

    /// `#rrggbb`, for Pango markup.
    pub fn hex(self) -> String {
        format!(
            "#{:02x}{:02x}{:02x}",
            (self.0 * 255.0).round() as u8,
            (self.1 * 255.0).round() as u8,
            (self.2 * 255.0).round() as u8
        )
    }

    pub fn mix(self, o: Rgba, t: f64) -> Rgba {
        Rgba(
            self.0 + (o.0 - self.0) * t,
            self.1 + (o.1 - self.1) * t,
            self.2 + (o.2 - self.2) * t,
            self.3 + (o.3 - self.3) * t,
        )
    }

    pub fn with_alpha(self, a: f64) -> Rgba {
        Rgba(self.0, self.1, self.2, a)
    }
}

const fn rgb(hex: u32) -> Rgba {
    Rgba(
        ((hex >> 16) & 0xff) as f64 / 255.0,
        ((hex >> 8) & 0xff) as f64 / 255.0,
        (hex & 0xff) as f64 / 255.0,
        1.0,
    )
}

/// Reads `#rgb`, `#rrggbb`, `#rrggbbaa`, `rgb(...)` and `rgba(...)`.
pub fn parse_color(s: &str) -> Option<Rgba> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix('#') {
        if !hex.is_ascii() {
            return None;
        }
        let h = |i: usize, n: usize| {
            u8::from_str_radix(&hex[i..i + n], 16)
                .ok()
                .map(|v| if n == 1 { v * 17 } else { v } as f64 / 255.0)
        };
        return match hex.len() {
            3 => Some(Rgba(h(0, 1)?, h(1, 1)?, h(2, 1)?, 1.0)),
            6 => Some(Rgba(h(0, 2)?, h(2, 2)?, h(4, 2)?, 1.0)),
            8 => Some(Rgba(h(0, 2)?, h(2, 2)?, h(4, 2)?, h(6, 2)?)),
            _ => None,
        };
    }
    let inner = s
        .strip_prefix("rgba(")
        .or_else(|| s.strip_prefix("rgb("))?
        .strip_suffix(')')?;
    let parts: Vec<f64> = inner
        .split(',')
        .filter_map(|p| p.trim().parse().ok())
        .collect();
    match parts.as_slice() {
        [r, g, b] => Some(Rgba(r / 255.0, g / 255.0, b / 255.0, 1.0)),
        [r, g, b, a] => Some(Rgba(r / 255.0, g / 255.0, b / 255.0, *a)),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Palette {
    /// The window.
    pub panel: Rgba,
    /// Sunken areas: lanes, chart wells.
    pub well: Rgba,
    pub tile: Rgba,
    pub tile_hover: Rgba,
    pub border: Rgba,
    pub fg: Rgba,
    pub bright: Rgba,
    pub dim: Rgba,
    /// Agents' work, links, the selected tab.
    pub accent: Rgba,
    /// Running and finished.
    pub ok: Rgba,
    /// The user's own work.
    pub warn: Rgba,
    pub bad: Rgba,
    /// Decisions.
    pub violet: Rgba,
    pub orange: Rgba,
    pub blue: Rgba,
    pub teal: Rgba,
    pub font: String,
}

impl Default for Palette {
    fn default() -> Self {
        Palette {
            panel: rgb(0x1e2024),
            well: rgb(0x17191c),
            tile: rgb(0x26292e),
            tile_hover: rgb(0x2f3339),
            border: rgb(0x363b43),
            fg: rgb(0xc5c8c6),
            bright: rgb(0xeceff4),
            dim: rgb(0x7c838e),
            accent: rgb(0x88c0d0),
            ok: rgb(0xa3be8c),
            warn: rgb(0xebcb8b),
            bad: rgb(0xbf616a),
            violet: rgb(0xb48ead),
            orange: rgb(0xd08770),
            blue: rgb(0x81a1c1),
            teal: rgb(0x8fbcbb),
            font: "monospace".into(),
        }
    }
}

impl Palette {
    pub fn from(bar: Option<&BarTheme>, cfg: &Config) -> Palette {
        let mut p = Palette::default();
        let mut font = cfg.theme.font.clone();
        if let Some(t) = bar.filter(|_| cfg.theme.from == "waybar") {
            // Only the bar's hue counts: a floating bar is often transparent.
            let bg = t
                .background
                .as_deref()
                .and_then(parse_color)
                .map(|c| c.with_alpha(1.0));
            let fg = t
                .foreground
                .as_deref()
                .and_then(parse_color)
                .map(|c| c.with_alpha(1.0));
            let black = Rgba(0.0, 0.0, 0.0, 1.0);
            if let Some(bg) = bg {
                p.panel = bg.mix(black, 0.22);
                p.well = bg.mix(black, 0.42);
            }
            if let Some(fg) = fg {
                p.fg = fg;
                p.bright = fg.mix(Rgba(1.0, 1.0, 1.0, 1.0), 0.55);
                p.dim = fg.mix(p.panel, 0.48);
                p.tile = p.panel.mix(fg, 0.055);
                p.tile_hover = p.panel.mix(fg, 0.11);
                p.border = p.panel.mix(fg, 0.14);
            }
            let named = |names: &[&str]| {
                names
                    .iter()
                    .find_map(|n| t.colors.get(*n).and_then(|c| parse_color(c)))
            };
            for (names, slot) in [
                (&["accent", "frost"][..], &mut p.accent),
                (&["green"][..], &mut p.ok),
                (&["yellow"][..], &mut p.warn),
                (&["red"][..], &mut p.bad),
                (&["purple", "magenta", "violet"][..], &mut p.violet),
                (&["orange"][..], &mut p.orange),
                (&["blue"][..], &mut p.blue),
                (&["teal", "cyan"][..], &mut p.teal),
            ] {
                if let Some(c) = named(names) {
                    *slot = c.with_alpha(1.0);
                }
            }
            if font.is_none() {
                font = t.font.clone();
            }
        }
        if let Some(a) = cfg.theme.accent.as_deref().and_then(parse_color) {
            p.accent = a;
        }
        if let Some(f) = font {
            p.font = f.replace('"', "");
        }
        p
    }

    /// The tokens `style.css` is written against.
    pub fn css(&self) -> String {
        let mut out = String::new();
        for (name, c) in [
            ("wb_panel", self.panel),
            ("wb_well", self.well),
            ("wb_tile", self.tile),
            ("wb_tile_hover", self.tile_hover),
            ("wb_border", self.border),
            ("wb_fg", self.fg),
            ("wb_bright", self.bright),
            ("wb_dim", self.dim),
            ("wb_accent", self.accent),
            ("wb_ok", self.ok),
            ("wb_warn", self.warn),
            ("wb_bad", self.bad),
            ("wb_violet", self.violet),
        ] {
            out.push_str(&format!("@define-color {name} {};\n", c.css()));
        }
        out.push_str(&format!(
            "window.waybill {{ font-family: \"{}\", monospace; }}\n",
            self.font
        ));
        out
    }

    /// A project's color: the same one every time, shared by `plos.db` and
    /// `plos.origins` because they are one family.
    pub fn project(&self, name: &str) -> Rgba {
        let family = name.split('.').next().unwrap_or(name);
        let hash = family.bytes().fold(0x811c_9dc5u32, |h, b| {
            (h ^ b as u32).wrapping_mul(0x0100_0193)
        });
        [
            self.accent,
            self.violet,
            self.teal,
            self.orange,
            self.blue,
            self.ok,
        ][(hash % 6) as usize]
    }
}

thread_local! {
    static CURRENT: RefCell<Palette> = RefCell::new(Palette::default());
}

pub fn current() -> Palette {
    CURRENT.with(|p| p.borrow().clone())
}

pub fn set_current(p: Palette) {
    CURRENT.with(|c| *c.borrow_mut() = p);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors() {
        assert_eq!(parse_color("#fff"), Some(Rgba(1.0, 1.0, 1.0, 1.0)));
        assert_eq!(
            parse_color("#282A2E").unwrap().css(),
            "rgba(40, 42, 46, 1.000)"
        );
        assert_eq!(parse_color("#282A2E").unwrap().hex(), "#282a2e");
        assert_eq!(parse_color("rgba(10, 20, 30, 0.5)").unwrap().3, 0.5);
        assert!(parse_color("teal").is_none());
    }

    #[test]
    fn follows_the_bar() {
        let t = waybill_core::waybar::parse_style(
            "@define-color frost #88C0D0;\n* { font-family: \"PragmataPro Liga\", monospace; }\n\
             window#waybar { background: rgba(40, 42, 46, 0); color: #C5C8C6; }",
        );
        let p = Palette::from(Some(&t), &Config::default());
        assert_eq!(p.font, "PragmataPro Liga");
        assert_eq!(
            p.panel.3, 1.0,
            "a transparent bar still gives an opaque window"
        );
        assert!(
            p.panel.0 < 40.0 / 255.0,
            "the window sits darker than the bar"
        );
        assert!(
            p.css()
                .contains("@define-color wb_accent rgba(136, 192, 208, 1.000);")
        );
        let mut cfg = Config::default();
        cfg.theme.from = "none".into();
        assert_eq!(Palette::from(Some(&t), &cfg), Palette::default());
    }

    #[test]
    fn a_project_family_shares_a_color() {
        let p = Palette::default();
        assert_eq!(p.project("plos.db"), p.project("plos.origins"));
        assert_eq!(p.project("atlas"), p.project("atlas"));
    }
}
