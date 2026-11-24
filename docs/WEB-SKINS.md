# lazyui web skins

## Web skins

Two ways to show the same engine in a browser tab, picked by what you mount:

- `mount(canvas, ...)` in `web/lazyui.js`: the character grid, looks like the terminal.
- `mountSkin(div, ...)` in `web/skin.js`: a DOM app - cards, segmented controls, SVG charts, icons, with a choice of
  looks (below). The engine still owns state, focus, keys and events; the skin reads
  `lz_describe` (the active elements with attributes and live state, widgets add their own data through
  `Widget::describe`) and sends semantic actions back with `lz_act` (`focus select activate text toggle sort reset`).

`web/index.html` takes `?skin=grid`, `?look=`, `?scheme=`, `?motion=`, `?dock=0`, `?spec=`, `?set=key=value`; the chip row in
the corner flips looks live. `examples/showcase.lui` has every
widget. `node web/browser-test.mjs` drives the skin in headless Chromium (no npm packages).

### Looks and motion

A look is a block of design tokens in `web/looks.js` (radius, font, palette, shadow, blur), a spring personality and a
list of effects; the skin's structure CSS reads only tokens, so a new look is a few variables. Seven ship:
`terminal` (green phosphor CRT, scanlines, blocky bars), `classic` (1995 bevels and title bars), `flat`, `modern`,
`glass` (frosted, 3D tilt), `neon` (glowing edges), `aurora` (animated sky, film grain). `mountSkin(div, {look: "glass",
scheme: "auto"})`, `ui.look("neon")` at runtime; `scheme` applies to the looks that have both (modern, flat, glass).
Old `flavor: "mac"|"win"` still works as `modern`/`flat`.

Everything that moves is a spring (`web/spring.js`, no dependencies, one rAF loop that sleeps at rest): bars and gauges
chase their value and overshoot or not depending on the preset (`stiff snappy gentle bouncy wobbly`), counters count,
line charts morph point by point, panels enter in a staggered wave, the active segment and the selected row are one
pill that slides, buttons squash on press, panels lean toward the cursor and a light follows it. `prefers-reduced-motion`
snaps everything. Effects are flags on the root (`data-fx`: `spot tilt thumb hl blobs grain scan glow`) and can be
overridden: `mountSkin(div, {look: "glass", fx: ["spot"]})`. `node web/spring.test.mjs` tests the maths; `node
web/shot.mjs out.png 'look=aurora&spec=showcase.lui'` screenshots a look in real time.

### sysbar

One line of vitals: `sysbar items=cpu,mem,swap,disk,net,ping,fps every=1 disk=/ ping=1.1.1.1:53`. The widget keeps its
numbers in the model (`sys.cpu sys.mem sys.swap sys.disk` percent, `sys.rx sys.tx` bytes/s, `sys.ping` ms, `sys.fps`), so
the same keys can be read by the program or fed from outside. Natively it samples `/proc` and `statvfs` once a second and
pings with a TCP connect on a background thread (`ping=off` to disable); `fps` is frames drawn per second. In the browser
there is no `/proc`: the skin measures fps (animation frames) and the round trip to the page's origin, and anything else
comes from `ui.feed({cpu: 12, mem: 40})`.

### worldmap, globe, minimap

Ported from lazytui. The planet is shaded per pixel in `world.rs` (a 0.5 degree raster of 177 countries, generated from
lazytui's `worlddata.h` by `tools/gen_world` into `worlddata.rs`, do not edit): soft coasts, climate / political / heat /
grey colouring, borders, graticule, the night side from the sun's position, and on the globe sun, specular, rim and
atmosphere glow. `worldmap`: wheel or `+ -` zoom (to 64x), drag pans, click picks a country (`bind=KEY` gets its ISO
code, event `pick`), `0` resets; `mode=climate|political|grey borders=0 night=1 grid=1 labels=1 lang=pl|en select=ISO
heat="PL:38,DE:83"` (or `heatbind=KEY lo hi ramp=c1,c2,c3`) `sun=DEG|auto`. `globe` turns with arrows or drag, tilt up to
85 degrees, same attributes. `sun=auto` reads `sys.utc` (seconds; the browser skin feeds it, natively the clock is used).
`minimap` is a page as a density strip with the viewport marked: `values offset page marks seek=KEY`, click or drag seeks.

The terminal draws the pixels as half blocks (two pixels per cell, ASCII fallback is a background fill); the browser
asks the engine for RGBA (`lz_pixels`, `Widget::pixels`) and puts it on a `<canvas>`, with the country names as DOM text
on top so they stay sharp.

### radar, rain, scramble, tagcloud, webchart, piechart

The fancy ones from lazytui, all pure functions of attributes and the Ui clock (`src/fancy.rs`). `radar blips="bearing:range:label;..."
speed=deg/s` sweeps and lights blips that fade over a rotation; `rain seed speed` is matrix rain; `scramble text progress=0..1|auto`
resolves text out of random glyphs; `tagcloud words weights`; `webchart values labels max rings`; `piechart values labels colors
donut=1 legend=0`. In the terminal round shapes are braille wires and half-block fills; in the browser they are SVG/CSS (radar
sweep and blip fade are CSS animations with delays computed from the bearing) and, for the two that never rest, `web/fancy.js`
runs one frame loop that sleeps when no rain or scramble is on the page.

### volumebar, pacmanbar, heatdither, corebars, ratepanel

More lazytui bars (`src/bars.rs`). `volumebar label bind=KEY value max=1 step=0.05 vertical=1 muted=KEY` is interactive: arrows,
PgUp/PgDn, Home/End, `m` mutes, click/drag/wheel on the bar, click the percent to mute; it emits `change` and `mute` and writes
`bind`/`muted`. In the browser it is a styled range slider with a mute button. `pacmanbar name done total rate eta` is pacman's own
`-Sy` line with the same arithmetic (the mouth chomps as the hash advances; a chomping Pac-Man eats the dots on the web).
`heatdither` is the heat grid as ordered-dither braille. `corebars labels values infos` and `ratepanel labels values peaks` are
monitor rows with a load bar (green/yellow/red by percent).

### colorpicker, timepicker, datepicker, gradientpicker

The pickers (`src/pickers.rs`) keep their own state, read `value=` once (or `bind=KEY` whenever the model changes it) and write
their text value back to `bind`: `#rrggbb`, `HH:MM[:SS]`, `YYYY-MM-DD`, `0:#000000,1:#ffffff`; each emits `change` with that
text. `colorpicker`: arrows move saturation/value, `[` `]` the hue, `r` resets, the mouse drags the plane and hue bar.
`timepicker seconds=1 look=fields|spinbox|h12`: left/right pick a field, up/down or the wheel change it. `datepicker`: arrows move
days/weeks, PgUp/PgDn months, `[` `]` years, click a day or the `‹ ›` arrows (own calendar maths, any year). `gradientpicker`:
tab picks a stop, left/right move it, click adds one (coloured by the gradient there), drag moves, del removes, `r` resets;
`act color #hex` recolours the selected stop, so a colorpicker can feed it. The browser draws the same state as a CSS gradient
plane, spinner buttons, a month grid and draggable stop dots.
