//! Colour slots as data. One place re-colours a whole UI; a theme is also a plain text file (`slot = #rrggbb`).

use crate::canvas::{rgb, Color, Style, BOLD, DIM, REVERSE};

#[derive(Clone, Debug)]
pub struct Theme {
    pub name: String,
    pub fg: Color,
    pub bg: Color,
    pub dim: Color,
    pub accent: Color,
    pub border: Color,
    pub focus: Color,
    pub ok: Color,
    pub warn: Color,
    pub err: Color,
    pub sel_fg: Color,
    pub sel_bg: Color,
}

impl Theme {
    /// His lazytui look: green on the terminal's own background.
    pub fn green() -> Theme {
        Theme {
            name: "green".into(),
            fg: rgb(205, 230, 205),
            bg: Color::Default,
            dim: rgb(105, 135, 110),
            accent: rgb(90, 225, 125),
            border: rgb(55, 135, 80),
            focus: rgb(130, 255, 160),
            ok: rgb(90, 225, 125),
            warn: rgb(235, 200, 80),
            err: rgb(240, 90, 90),
            sel_fg: rgb(10, 20, 12),
            sel_bg: rgb(90, 225, 125),
        }
    }
    pub fn matrix() -> Theme {
        Theme {
            name: "matrix".into(),
            fg: rgb(0, 255, 70),
            bg: rgb(0, 0, 0),
            dim: rgb(0, 110, 30),
            accent: rgb(180, 255, 190),
            border: rgb(0, 150, 45),
            focus: rgb(180, 255, 190),
            ok: rgb(0, 255, 70),
            warn: rgb(200, 255, 0),
            err: rgb(255, 60, 60),
            sel_fg: rgb(0, 0, 0),
            sel_bg: rgb(0, 255, 70),
        }
    }
    pub fn light() -> Theme {
        Theme {
            name: "light".into(),
            fg: rgb(30, 34, 40),
            bg: rgb(250, 250, 247),
            dim: rgb(130, 135, 140),
            accent: rgb(30, 110, 200),
            border: rgb(160, 168, 178),
            focus: rgb(30, 110, 200),
            ok: rgb(30, 150, 80),
            warn: rgb(190, 130, 20),
            err: rgb(200, 50, 50),
            sel_fg: rgb(255, 255, 255),
            sel_bg: rgb(30, 110, 200),
        }
    }
    pub fn by_name(n: &str) -> Option<Theme> {
        match n {
            "green" => Some(Theme::green()),
            "matrix" => Some(Theme::matrix()),
            "light" => Some(Theme::light()),
            _ => None,
        }
    }
    /// `slot = #rrggbb` lines on top of a preset (`base = matrix`); `#` starts a comment only at the start of a line.
    pub fn parse(text: &str) -> Result<Theme, String> {
        let mut t = Theme::green();
        for (n, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (k, v) = line.split_once('=').ok_or_else(|| format!("line {}: expected `slot = value`", n + 1))?;
            let (k, v) = (k.trim(), v.trim());
            if k == "base" {
                t = Theme::by_name(v).ok_or_else(|| format!("line {}: unknown base theme `{}`", n + 1, v))?;
                continue;
            }
            let c = parse_color(v).ok_or_else(|| format!("line {}: `{}` is not a colour (#rrggbb or default)", n + 1, v))?;
            match k {
                "name" => t.name = v.to_string(),
                "fg" => t.fg = c,
                "bg" => t.bg = c,
                "dim" => t.dim = c,
                "accent" => t.accent = c,
                "border" => t.border = c,
                "focus" => t.focus = c,
                "ok" => t.ok = c,
                "warn" => t.warn = c,
                "err" => t.err = c,
                "sel_fg" => t.sel_fg = c,
                "sel_bg" => t.sel_bg = c,
                _ => return Err(format!("line {}: unknown slot `{}`", n + 1, k)),
            }
        }
        Ok(t)
    }

    pub fn base(&self) -> Style {
        Style::new(self.fg, self.bg)
    }
    pub fn dim_style(&self) -> Style {
        Style::new(self.dim, self.bg)
    }
    pub fn accent_style(&self) -> Style {
        Style::new(self.accent, self.bg)
    }
    pub fn border_style(&self, focused: bool) -> Style {
        Style::new(if focused { self.focus } else { self.border }, self.bg)
    }
    pub fn title_style(&self, focused: bool) -> Style {
        Style::new(if focused { self.focus } else { self.accent }, self.bg).with(BOLD)
    }
    pub fn selected(&self, focused: bool) -> Style {
        if focused {
            Style::new(self.sel_fg, self.sel_bg)
        } else {
            Style::new(self.fg, self.bg).with(REVERSE | DIM)
        }
    }
}

pub fn parse_color(s: &str) -> Option<Color> {
    let s = s.trim();
    if s == "default" {
        return Some(Color::Default);
    }
    let h = s.strip_prefix('#')?;
    if h.len() != 6 || !h.is_ascii() {
        return None;
    }
    let p = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
    Some(Color::Rgb(p(0)?, p(2)?, p(4)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_text_overrides_preset() {
        let t = Theme::parse("base = matrix\naccent = #ff0000\n# comment\n").unwrap();
        assert_eq!(t.accent, Color::Rgb(255, 0, 0));
        assert_eq!(t.bg, Color::Rgb(0, 0, 0));
    }

    #[test]
    fn theme_errors_have_line_numbers() {
        assert!(Theme::parse("fg = nope").unwrap_err().starts_with("line 1"));
        assert!(Theme::parse("\nwat = #000000").unwrap_err().starts_with("line 2"));
    }
}
