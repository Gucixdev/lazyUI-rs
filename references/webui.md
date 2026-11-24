# webui - what matters

`webui-dev/webui` (MIT, "Copyright (c) 2025 Hassan Draga"): a C library, v2.5.0-beta.4 at the time of reading (`WEBUI_VERSION` in
`include/webui.h`; last push 2026-10-28). Read: `README.md`, `include/webui.h` (the whole API is that one header, with doc comments),
`examples/C`. Not checked: the online docs at webui.me, the protocol source in `src/`, behaviour on a real machine (nothing was built or run).

## Game changers

1. **The installed browser is the window.** No embedded Chromium, no Node, no WebView SDK: the README's runtime table lists
   "Only A Web Browser" for WebUI against Tauri (WebView2 / WebKitGTK), Qt (bundles), Electron and NW.js (embedded Chromium + Node.js).
   Browsers listed as supported on Linux: Firefox, Chrome, Edge, Chromium, Yandex, Brave, Vivaldi (Safari and Opera not available/coming).
2. **Four calls make an app.** The minimal C program is `webui_new_window()`, `webui_show(win, "<html>...</html>")`, `webui_wait()`.
   `webui_show` takes inline HTML, a file name or a URL. The page loads `webui.js` from the library's own tiny server.
3. **Backend functions bound by name.** `webui_bind(win, "myFunction", fn)` makes the JS side able to call it; the event carries typed
   getters (`webui_get_string`, `webui_get_string_at`, ...) and `webui_set_context`/`webui_get_context` hang user data on a binding.
   The other way: `webui_run(win, js)` fires a script, `webui_script(win, js, timeout, buffer, len)` runs one and returns the answer.
4. **Fast binary WebSocket protocol** between library and page (README feature list). Details of the protocol: not checked.
5. **Private browser profile** by default for safety (README: "Uses private profile for safety"); `webui_set_profile` overrides.
6. **Wide language reach through one C ABI**: wrappers listed for Python, Go, Zig, Nim, V, Rust, Deno, Bun, Swift, Odin, Pascal and more.
7. **Optional WebView** (`webui_show_wv`; WebView2, GTK WebView, WKWebView) when a chrome-less window is wanted, same API.
8. Knobs seen in the header: `webui_set_kiosk`, `webui_set_hide`, `webui_set_port`, `webui_set_public` (reachable from other hosts),
   `webui_set_root_folder`, `webui_set_file_handler`, optional TLS (`webui_set_tls_certificate`), `multi_client` config.

## What we take

| webui idea | in lazyui |
|---|---|
| "the browser you already have is the GUI" | the web backend already is that: `web/run` serves the wasm engine, any browser opens it. A later `lazyui run --browser` could serve it on `127.0.0.1` and open the default browser, no GUI crates (`gui` stays optional) |
| bind a backend function by name, call it from the page | our equivalent is the event/model protocol: `lz_events` out, `set model value` in. Same shape, but typed on our side, so nothing like `webui_get_string` is needed |
| `webui_script` = run code in the page, get the answer | headless `snap` and `tree` already answer "what does the UI show"; only the live-browser case would need a round trip |
| private profile, kiosk, hide | when a browser launch is added: fresh temp profile, `--app=URL` style window, no history/cookies leak |
| one call to show, one to wait | the 15-line example in the README: `run(&mut app, Opts::default())` |

## What we leave

- The C library itself, its server and its WebSocket protocol: our page talks to a wasm module that runs in the page, so there is no
  backend process to bridge to. If a native-process backend is wanted later, a ~200-line own server (std `TcpListener`) is enough.
- JS in the page as the UI: webui wants you to write HTML/CSS/JS. We paint the same widget tree, from Rust or a spec, on a canvas or
  as DOM from `tree_json`; the user writes no front-end.
- Browser detection tables for Windows/macOS (we are Linux first).
- Wrappers for 20 languages: the `lazyui` binary plus JSON lines on stdout is our language-neutral door.

## Licence

MIT (`LICENSE` in the repo and the README footer). Copying with notice would be allowed, but there is nothing to copy: only the idea is used.
