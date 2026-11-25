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
    /// A finedesktop theme by name: `$FD_CONFIG_DIR/themes/NAME`, else `$XDG_CONFIG_HOME/finedesktop/themes/NAME`
    /// (default `~/.config`). Same file, same colours as the desktop; the built-in ones of finedesktop live in its binary.
    pub fn from_finedesktop(name: &str) -> Result<Theme, String> {
        if name.is_empty() || name.contains('/') || name.contains("..") {
            return Err(format!("bad theme name `{name}`"));
        }
        let dir = match std::env::var_os("FD_CONFIG_DIR").filter(|v| !v.is_empty()) {
            Some(d) => std::path::PathBuf::from(d),
            None => {
                let base = std::env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()).map(std::path::PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| std::path::Path::new(&h).join(".config")));
                base.ok_or("no HOME")?.join("finedesktop")
            }
        };
        let path = dir.join("themes").join(name);
        let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut t = Theme::parse(&text)?;
        t.name = name.to_string();
        Ok(t)
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
            // finedesktop theme files: `border = 2` is a width there, not a colour; gap, animations and the
            // bar colours have no slot here and are skipped, so one file themes both programs
            if matches!(k, "gap" | "animations" | "bar_bg" | "bar_fg") || (k == "border" && v.parse::<i64>().is_ok()) {
                continue;
            }
            let k = match k {
                "border_normal" => "border",
                "border_focus" => "focus",
                "urgent" => "err",
                other => other,
            };
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
    fn a_finedesktop_theme_file_parses_unchanged() {
        let dracula = "bg = #282a36\nfg = #f8f8f2\ndim = #6272a4\naccent = #bd93f9\nerr = #ff5555\nbar_bg = #21222c\nsel_bg = #bd93f9\nsel_fg = #282a36\nborder_normal = #44475a\nborder_focus = #bd93f9\nborder = 2\ngap = 6\nanimations = on\n";
        let t = Theme::parse(dracula).unwrap();
        assert_eq!((t.bg, t.accent, t.err), (Color::Rgb(0x28, 0x2a, 0x36), Color::Rgb(0xbd, 0x93, 0xf9), Color::Rgb(0xff, 0x55, 0x55)));
        assert_eq!((t.border, t.focus), (Color::Rgb(0x44, 0x47, 0x5a), Color::Rgb(0xbd, 0x93, 0xf9)));
        assert_eq!(Theme::parse("border = #112233").unwrap().border, Color::Rgb(0x11, 0x22, 0x33), "a colour border still works");
        assert_eq!(Theme::parse("urgent = #010203").unwrap().err, Color::Rgb(1, 2, 3));
    }

    #[test]
    fn from_finedesktop_reads_the_desktop_folder_and_refuses_paths() {
        let base = std::env::temp_dir().join(format!("lui-fdtheme-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("themes")).unwrap();
        std::fs::write(base.join("themes/mine"), "accent = #0a0b0c\nborder = 3\n").unwrap();
        std::env::set_var("FD_CONFIG_DIR", &base);
        let t = Theme::from_finedesktop("mine").unwrap();
        assert_eq!((t.name.as_str(), t.accent), ("mine", Color::Rgb(10, 11, 12)));
        assert!(Theme::from_finedesktop("../mine").is_err() && Theme::from_finedesktop("nope").is_err());
        std::env::remove_var("FD_CONFIG_DIR");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn theme_errors_have_line_numbers() {
        assert!(Theme::parse("fg = nope").unwrap_err().starts_with("line 1"));
        assert!(Theme::parse("\nwat = #000000").unwrap_err().starts_with("line 2"));
    }
}
