//! libxkbcommon loaded at run time with `dlopen`, so the library builds and runs without it (a panel then has a
//! pointer and no keyboard). The compositor sends the keymap as text; xkb turns scancodes into keysyms and text.
use crate::input::{Event, Key, Mods};
use std::ffi::{c_char, c_int, c_void, CString};
use std::io::{Error, Result};

type P = *mut c_void;
const FORMAT_TEXT_V1: u32 = 1;
const STATE_MODS_EFFECTIVE: u32 = 8;

struct Fns {
    context_new: unsafe extern "C" fn(u32) -> P,
    context_unref: unsafe extern "C" fn(P),
    keymap_from_string: unsafe extern "C" fn(P, *const c_char, u32, u32) -> P,
    keymap_unref: unsafe extern "C" fn(P),
    state_new: unsafe extern "C" fn(P) -> P,
    state_unref: unsafe extern "C" fn(P),
    update_mask: unsafe extern "C" fn(P, u32, u32, u32, u32, u32, u32) -> u32,
    one_sym: unsafe extern "C" fn(P, u32) -> u32,
    get_utf8: unsafe extern "C" fn(P, u32, *mut c_char, usize) -> c_int,
    mod_active: unsafe extern "C" fn(P, *const c_char, u32) -> c_int,
    sym_to_utf32: unsafe extern "C" fn(u32) -> u32,
    keymap_from_names: unsafe extern "C" fn(P, *const c_void, u32) -> P,
    keymap_as_string: unsafe extern "C" fn(P, u32) -> *mut c_char,
}

pub struct Xkb {
    f: Fns,
    ctx: P,
    keymap: P,
    state: P,
}

fn sym<T: Copy>(lib: *mut c_void, name: &str) -> Result<T> {
    let c = CString::new(name).unwrap();
    let p = unsafe { libc::dlsym(lib, c.as_ptr()) };
    if p.is_null() {
        return Err(Error::other(format!("libxkbcommon has no {name}")));
    }
    // the symbol's type is the one declared in Fns for that name
    Ok(unsafe { std::mem::transmute_copy::<*mut c_void, T>(&p) })
}

impl Xkb {
    pub fn load() -> Result<Xkb> {
        let lib = unsafe { libc::dlopen(c"libxkbcommon.so.0".as_ptr(), libc::RTLD_NOW) };
        if lib.is_null() {
            return Err(Error::other("libxkbcommon.so.0 is not installed: no keyboard"));
        }
        let f = Fns {
            context_new: sym(lib, "xkb_context_new")?,
            context_unref: sym(lib, "xkb_context_unref")?,
            keymap_from_string: sym(lib, "xkb_keymap_new_from_string")?,
            keymap_unref: sym(lib, "xkb_keymap_unref")?,
            state_new: sym(lib, "xkb_state_new")?,
            state_unref: sym(lib, "xkb_state_unref")?,
            update_mask: sym(lib, "xkb_state_update_mask")?,
            one_sym: sym(lib, "xkb_state_key_get_one_sym")?,
            get_utf8: sym(lib, "xkb_state_key_get_utf8")?,
            mod_active: sym(lib, "xkb_state_mod_name_is_active")?,
            sym_to_utf32: sym(lib, "xkb_keysym_to_utf32")?,
            keymap_from_names: sym(lib, "xkb_keymap_new_from_names")?,
            keymap_as_string: sym(lib, "xkb_keymap_get_as_string")?,
        };
        let ctx = unsafe { (f.context_new)(0) };
        if ctx.is_null() {
            return Err(Error::other("xkb_context_new failed"));
        }
        Ok(Xkb { f, ctx, keymap: std::ptr::null_mut(), state: std::ptr::null_mut() })
    }

    fn drop_keymap(&mut self) {
        unsafe {
            if !self.state.is_null() {
                (self.f.state_unref)(self.state);
            }
            if !self.keymap.is_null() {
                (self.f.keymap_unref)(self.keymap);
            }
        }
        self.state = std::ptr::null_mut();
        self.keymap = std::ptr::null_mut();
    }

    /// Replace the keymap with the compositor's text (format `xkb_v1`, NUL-terminated).
    pub fn set_keymap(&mut self, text: &[u8]) -> Result<()> {
        let c = CString::new(text.iter().copied().take_while(|&b| b != 0).collect::<Vec<u8>>()).map_err(Error::other)?;
        let km = unsafe { (self.f.keymap_from_string)(self.ctx, c.as_ptr(), FORMAT_TEXT_V1, 0) };
        if km.is_null() {
            return Err(Error::other("libxkbcommon could not parse the compositor's keymap"));
        }
        self.drop_keymap();
        self.keymap = km;
        self.state = unsafe { (self.f.state_new)(km) };
        Ok(())
    }

    pub fn has_keymap(&self) -> bool {
        !self.state.is_null()
    }

    /// The default keymap of the machine (rules, layout from the environment) as `xkb_v1` text. Tests use it.
    pub fn default_keymap_text(&self) -> Result<Vec<u8>> {
        let km = unsafe { (self.f.keymap_from_names)(self.ctx, std::ptr::null(), 0) };
        if km.is_null() {
            return Err(Error::other("no default xkb keymap (xkeyboard-config missing?)"));
        }
        let p = unsafe { (self.f.keymap_as_string)(km, FORMAT_TEXT_V1) };
        let out = if p.is_null() { Vec::new() } else { unsafe { std::ffi::CStr::from_ptr(p).to_bytes_with_nul().to_vec() } };
        unsafe {
            libc::free(p as *mut _);
            (self.f.keymap_unref)(km);
        }
        Ok(out)
    }

    pub fn update_mask(&mut self, dep: u32, lat: u32, locked: u32, group: u32) -> Mods {
        if self.state.is_null() {
            return Mods::NONE;
        }
        unsafe { (self.f.update_mask)(self.state, dep, lat, locked, group, 0, 0) };
        let on = |name: &[u8]| unsafe { (self.f.mod_active)(self.state, name.as_ptr() as *const c_char, STATE_MODS_EFFECTIVE) > 0 };
        Mods { shift: on(b"Shift\0"), ctrl: on(b"Control\0"), alt: on(b"Mod1\0") }
    }

    /// A pressed evdev key code as a lazyui event, with the modifiers last reported.
    pub fn key(&self, evdev: u32, mods: Mods) -> Option<Event> {
        if self.state.is_null() {
            return None;
        }
        let code = evdev + 8;
        let s = unsafe { (self.f.one_sym)(self.state, code) };
        let mut buf = [0 as c_char; 16];
        let n = unsafe { (self.f.get_utf8)(self.state, code, buf.as_mut_ptr(), buf.len()) };
        let text: String = if n > 0 { String::from_utf8_lossy(&buf[..n as usize].iter().map(|&b| b as u8).collect::<Vec<u8>>()).into_owned() } else { String::new() };
        let ch = char::from_u32(unsafe { (self.f.sym_to_utf32)(s) }).filter(|&c| c != '\0');
        map(s, &text, ch, mods)
    }
}

impl Drop for Xkb {
    fn drop(&mut self) {
        self.drop_keymap();
        unsafe { (self.f.context_unref)(self.ctx) };
    }
}

/// Keysym, the text it types and the character of the keysym itself (lowercase for chords), to a lazyui key event.
/// Same rules as the winit mapping: a chord with ctrl or alt is the key itself with its modifiers; plain typing is
/// whatever the layout produced.
pub fn map(sym: u32, text: &str, sym_char: Option<char>, mods: Mods) -> Option<Event> {
    let named = |k: Key, m: Mods| Some(Event::Key(k, m));
    match sym {
        0xff0d | 0xff8d => named(Key::Enter, mods),
        0xfe20 => named(Key::BackTab, Mods { shift: false, ..mods }),
        0xff09 if mods.shift => named(Key::BackTab, Mods { shift: false, ..mods }),
        0xff09 => named(Key::Tab, mods),
        0xff08 => named(Key::Backspace, mods),
        0xffff => named(Key::Delete, mods),
        0xff63 => named(Key::Insert, mods),
        0xff1b => named(Key::Esc, mods),
        0xff52 => named(Key::Up, mods),
        0xff54 => named(Key::Down, mods),
        0xff51 => named(Key::Left, mods),
        0xff53 => named(Key::Right, mods),
        0xff50 => named(Key::Home, mods),
        0xff57 => named(Key::End, mods),
        0xff55 => named(Key::PageUp, mods),
        0xff56 => named(Key::PageDown, mods),
        0x20 => named(Key::Char(' '), Mods { shift: false, ..mods }),
        0xffbe..=0xffc9 => named(Key::F((sym - 0xffbe + 1) as u8), mods),
        _ if mods.ctrl || mods.alt => {
            let c = sym_char?.to_lowercase().next()?;
            Some(Event::Key(Key::Char(c), Mods { shift: false, ..mods }))
        }
        _ => {
            let c = text.chars().next().filter(|c| !c.is_control())?;
            Some(Event::Key(Key::Char(c), Mods::NONE))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_keys_and_chords_follow_the_winit_rules() {
        assert_eq!(map(0xff0d, "\r", None, Mods::NONE), Some(Event::Key(Key::Enter, Mods::NONE)));
        assert_eq!(map(0xff09, "\t", None, Mods::SHIFT), Some(Event::Key(Key::BackTab, Mods::NONE)));
        assert_eq!(map(0xffc1, "", None, Mods::CTRL), Some(Event::Key(Key::F(4), Mods::CTRL)));
        assert_eq!(map(0x20, " ", Some(' '), Mods::SHIFT), Some(Event::Key(Key::Char(' '), Mods::NONE)));
        // ctrl-c: the text is a control code, the chord is the letter
        assert_eq!(map(0x63, "\u{3}", Some('c'), Mods::CTRL), Some(Event::Key(Key::Char('c'), Mods::CTRL)));
        assert_eq!(map(0x41, "A", Some('A'), Mods { alt: true, shift: true, ctrl: false }), Some(Event::Key(Key::Char('a'), Mods::ALT)));
        // plain typing is the produced text, shift already applied; a key without text (shift itself) is nothing
        assert_eq!(map(0x41, "A", Some('A'), Mods::SHIFT), Some(Event::Key(Key::Char('A'), Mods::NONE)));
        assert_eq!(map(0xffe1, "", None, Mods::SHIFT), None);
    }

    #[test]
    fn the_real_library_types_letters_with_shift_and_chords_with_ctrl() {
        let Ok(mut x) = Xkb::load() else { return };
        let Ok(text) = x.default_keymap_text() else { return };
        if text.is_empty() {
            return;
        }
        x.set_keymap(&text).unwrap();
        assert!(x.has_keymap());
        let a = 30; // KEY_A in evdev
        let none = x.update_mask(0, 0, 0, 0);
        assert_eq!(none, Mods::NONE);
        let lower = x.key(a, none);
        assert!(lower.is_some(), "the key A types something in any layout");
        println!("A -> {lower:?}");
        let shift = x.update_mask(1, 0, 0, 0);
        assert_eq!(shift, Mods::SHIFT);
        let upper = x.key(a, shift);
        let ctrl = x.update_mask(4, 0, 0, 0);
        assert_eq!(ctrl, Mods::CTRL);
        let chord = x.key(a, ctrl);
        // the default layout might not be latin; the three must at least differ the right way when it is
        if lower == Some(Event::Key(Key::Char('a'), Mods::NONE)) {
            assert_eq!(upper, Some(Event::Key(Key::Char('A'), Mods::NONE)));
            assert_eq!(chord, Some(Event::Key(Key::Char('a'), Mods::CTRL)));
        }
        assert!(x.set_keymap(b"not a keymap\0").is_err());
        assert!(x.has_keymap(), "a bad keymap leaves the old one in place");
    }
}
