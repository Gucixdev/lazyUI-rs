/* lazyui C ABI. Build the library with
 *     cargo build --release --no-default-features --features web           (liblazyui.so / .a / .dylib / .dll)
 *     cargo build --release --target wasm32-unknown-unknown --no-default-features --features web   (lazyui.wasm)
 * Same functions in both. One UI instance per process (or per wasm module), single-threaded: call from one thread only.
 *
 * Strings go in as (pointer, length) UTF-8, no NUL needed. Functions that return text fill an internal buffer and return its
 * length; read it with lz_out_ptr() before the next call that fills it. Failures return 0 and leave the reason in lz_error().
 * Native callers can pass their own memory directly; lz_alloc/lz_free exist for wasm hosts that must put bytes into linear memory.
 */
#ifndef LAZYUI_H
#define LAZYUI_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* memory (wasm hosts) */
uint8_t *lz_alloc(size_t n);
void lz_free(uint8_t *p, size_t n);

/* setup */
int32_t lz_widget(const uint8_t *name, size_t name_len, const uint8_t *text, size_t text_len); /* register a component before lz_init; 1 = ok */
int32_t lz_init(const uint8_t *spec, size_t len);   /* parse a .lui spec, create the UI; 1 = ok */
int32_t lz_reload(const uint8_t *spec, size_t len); /* new spec, model values kept; a bad spec keeps the old UI; 1 = ok */
int32_t lz_theme(const uint8_t *name, size_t len);  /* 1 = known theme */
void lz_resize(int32_t cols, int32_t rows);

/* i18n: a TSV catalog (format: see src/i18n.rs) and the active locale; texts written as "@key" in the spec are translated */
int32_t lz_catalog(const uint8_t *tsv, size_t len); /* 1 = ok; passing an empty catalog switches translation off */
int32_t lz_locale(const uint8_t *locale, size_t len);

/* input */
int32_t lz_key(const uint8_t *name, size_t len);    /* "enter", "tab", "ctrl-q", "a"; 1 = something used it */
void lz_text(const uint8_t *s, size_t len);         /* typed characters */
void lz_paste(const uint8_t *s, size_t len);
int32_t lz_mouse(uint32_t kind, uint32_t button, int32_t x, int32_t y, uint32_t mods); /* kind: 0 down 1 up 2 drag 3 move 4 wheel-up 5 wheel-down; mods: 1 ctrl 2 alt 4 shift */
int32_t lz_act(const uint8_t *id, size_t id_len, const uint8_t *action, size_t action_len, const uint8_t *arg, size_t arg_len);
                                                    /* semantic: focus select activate text toggle sort reset */
void lz_set(const uint8_t *key, size_t key_len, const uint8_t *value, size_t value_len); /* model value */

/* time: call lz_tick with the milliseconds since the last call; lz_wake_ms says when to call next (-1 = only on input) */
void lz_tick(double dt_ms);
double lz_wake_ms(void);

/* output */
int32_t lz_render(void);                            /* draw; 1 when the frame changed */
const uint32_t *lz_frame_ptr(void);                 /* cols*rows cells, 4 words each: char, fg, bg, mods */
size_t lz_frame_len(void);                          /* length in words */
size_t lz_snapshot(void);                           /* the frame as plain text into the out buffer */
size_t lz_events(void);                             /* pending events, one JSON object per line, into the out buffer */
size_t lz_describe(void);                           /* the element tree as JSON, into the out buffer */
size_t lz_error(void);                              /* last error text, into the out buffer */
const uint8_t *lz_out_ptr(void);                    /* the out buffer */

/* widget pictures (maps) */
size_t lz_pixels(const uint8_t *id, size_t id_len, int32_t w, int32_t h); /* RGBA byte count; bytes at lz_pix_ptr */
const uint8_t *lz_pix_ptr(void);
const uint8_t *lz_aux_ptr(void);
size_t lz_aux_len(void);

#ifdef __cplusplus
}
#endif
#endif
