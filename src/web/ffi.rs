//! C-ABI shell over `Web` for the browser. One instance per module; strings cross as (ptr, len) in memory the JS side
//! got from `lz_alloc`. Every function that can fail returns 0/1 and leaves the reason in `lz_error`.

use super::Web;
use std::cell::RefCell;

thread_local! {
    static APP: RefCell<Option<Web>> = const { RefCell::new(None) };
    static OUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    static WIDGETS: RefCell<crate::components::Widgets> = RefCell::new(crate::components::Widgets::builtin());
    static ERR: RefCell<String> = const { RefCell::new(String::new()) };
}

fn with<R>(f: impl FnOnce(&mut Web) -> R, none: R) -> R {
    APP.with(|a| match a.borrow_mut().as_mut() {
        Some(w) => f(w),
        None => none,
    })
}

fn fail(e: String) -> i32 {
    ERR.with(|x| *x.borrow_mut() = e);
    0
}

unsafe fn s<'a>(p: *const u8, n: usize) -> &'a str {
    std::str::from_utf8(std::slice::from_raw_parts(p, n)).unwrap_or("")
}

/// Register a component before `lz_init` (name and text as UTF-8). 1 when accepted, else the reason is in `lz_error`.
///
/// # Safety
/// Both pairs are readable UTF-8.
#[no_mangle]
pub unsafe extern "C" fn lz_widget(np: *const u8, nn: usize, tp: *const u8, tn: usize) -> i32 {
    let name = s(np, nn);
    match WIDGETS.with(|w| w.borrow_mut().add_source(name, s(tp, tn), crate::components::Scope::Global, "page")) {
        Ok(()) => 1,
        Err(e) => fail(e),
    }
}

#[no_mangle]
pub extern "C" fn lz_alloc(n: usize) -> *mut u8 {
    let mut v = Vec::<u8>::with_capacity(n.max(1));
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

/// # Safety
/// `p`/`n` must come from `lz_alloc(n)`.
#[no_mangle]
pub unsafe extern "C" fn lz_free(p: *mut u8, n: usize) {
    drop(Vec::from_raw_parts(p, 0, n.max(1)));
}

/// # Safety
/// `p`/`n` is a readable UTF-8 spec.
#[no_mangle]
pub unsafe extern "C" fn lz_init(p: *const u8, n: usize) -> i32 {
    match Web::with_widgets(s(p, n), WIDGETS.with(|w| w.borrow().clone())) {
        Ok(w) => {
            APP.with(|a| *a.borrow_mut() = Some(w));
            1
        }
        Err(e) => fail(e),
    }
}

/// # Safety
/// `p`/`n` is a readable UTF-8 spec.
#[no_mangle]
pub unsafe extern "C" fn lz_reload(p: *const u8, n: usize) -> i32 {
    with(|w| w.reload(s(p, n)).map(|_| 1).unwrap_or_else(fail), fail("not initialised".into()))
}

#[no_mangle]
pub extern "C" fn lz_resize(w: i32, h: i32) {
    with(|a| a.resize(w, h), ())
}

/// # Safety
/// `p`/`n` is a readable key name.
#[no_mangle]
pub unsafe extern "C" fn lz_key(p: *const u8, n: usize) -> i32 {
    with(|w| w.key(s(p, n)) as i32, 0)
}

/// # Safety
/// `p`/`n` is readable UTF-8.
#[no_mangle]
pub unsafe extern "C" fn lz_text(p: *const u8, n: usize) {
    with(|w| w.text(s(p, n)), ())
}

/// # Safety
/// `p`/`n` is readable UTF-8.
#[no_mangle]
pub unsafe extern "C" fn lz_paste(p: *const u8, n: usize) {
    with(|w| w.paste(s(p, n)), ())
}

#[no_mangle]
pub extern "C" fn lz_mouse(kind: u32, button: u32, x: i32, y: i32, mods: u32) -> i32 {
    with(|w| w.mouse(kind, button as u8, x, y, mods) as i32, 0)
}

#[no_mangle]
pub extern "C" fn lz_tick(dt_ms: f64) {
    with(|w| w.tick(dt_ms), ())
}

#[no_mangle]
pub extern "C" fn lz_wake_ms() -> f64 {
    with(|w| w.wake_ms(), -1.0)
}

/// # Safety
/// Both pairs are readable UTF-8.
#[no_mangle]
pub unsafe extern "C" fn lz_set(kp: *const u8, kn: usize, vp: *const u8, vn: usize) {
    with(|w| w.set(s(kp, kn), s(vp, vn)), ())
}

/// # Safety
/// `p`/`n` is a readable theme name.
#[no_mangle]
pub unsafe extern "C" fn lz_theme(p: *const u8, n: usize) -> i32 {
    with(|w| w.theme(s(p, n)) as i32, 0)
}

/// Draws; 1 when the frame changed. Read it with `lz_frame_ptr`/`lz_frame_len` (u32 words).
#[no_mangle]
pub extern "C" fn lz_render() -> i32 {
    with(|w| w.render() as i32, 0)
}

#[no_mangle]
pub extern "C" fn lz_frame_ptr() -> *const u32 {
    with(|w| w.frame().as_ptr(), std::ptr::null())
}

#[no_mangle]
pub extern "C" fn lz_frame_len() -> usize {
    with(|w| w.frame().len(), 0)
}

/// Pending events as JSON lines; read with `lz_out_ptr`/`lz_out_len` before the next call that fills it.
#[no_mangle]
pub extern "C" fn lz_events() -> usize {
    let e = with(|w| w.take_events().to_string(), String::new());
    OUT.with(|o| {
        *o.borrow_mut() = e.into_bytes();
        o.borrow().len()
    })
}

#[no_mangle]
pub extern "C" fn lz_error() -> usize {
    ERR.with(|e| OUT.with(|o| {
        *o.borrow_mut() = e.borrow().clone().into_bytes();
        o.borrow().len()
    }))
}

#[no_mangle]
pub extern "C" fn lz_out_ptr() -> *const u8 {
    OUT.with(|o| o.borrow().as_ptr())
}
