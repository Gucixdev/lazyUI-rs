//! The planet as pure functions: countries as a raster, the flat map and the globe as pixel shaders, the zoom/pan view, label
//! placement. No drawing target here: a shader answers "what colour is this pixel", and the caller decides what a pixel is -
//! half a terminal cell (the TUI) or a real pixel of a `<canvas>` (the web skin). Pixels are square: a cell is 1 wide, 2 tall.
//! Data: Natural Earth 1:110m countries (public domain), generated into `worlddata.rs` by `tools/gen_world`.

use crate::worlddata::{COUNTRIES, RINGS, VERTS};
use std::f64::consts::PI;
use std::sync::OnceLock;

pub const GW: usize = 720; // 0.5 degree cells: column 0 starts at 180W, row 0 at 90N
pub const GH: usize = 360;
const SEA: u8 = 255;

pub fn count() -> usize {
    COUNTRIES.len()
}

struct Grid {
    id: Vec<u8>,
    /// the land/sea classification differs somewhere in the 3x3 neighbourhood: the bilinear land fraction is not just 0 or 1
    soft: Vec<bool>,
}

fn vert(i: usize) -> (f64, f64) {
    (VERTS[2 * i] as f64 / 100.0, VERTS[2 * i + 1] as f64 / 100.0)
}

fn rings_of(ci: usize) -> &'static [(u32, u32)] {
    let c = &COUNTRIES[ci];
    &RINGS[c.ring0 as usize..(c.ring0 + c.nrings) as usize]
}

/// Scanline fill, even-odd over all rings of a country, so holes work.
fn fill_country(id: &mut [u8], ci: usize) {
    let rings = rings_of(ci);
    let (mut ymin, mut ymax) = (f64::MAX, f64::MIN);
    for &(s, n) in rings {
        for i in 0..n as usize {
            let y = vert(s as usize + i).1;
            ymin = ymin.min(y);
            ymax = ymax.max(y);
        }
    }
    let row0 = (((90.0 - ymax) * 2.0).floor() as i64 - 1).max(0) as usize;
    let row1 = (((90.0 - ymin) * 2.0).ceil() as i64 + 1).min(GH as i64 - 1) as usize;
    let mut xs: Vec<f64> = Vec::new();
    for row in row0..=row1 {
        let lat = 90.0 - (row as f64 + 0.5) / 2.0;
        xs.clear();
        for &(s, n) in rings {
            let (s, n) = (s as usize, n as usize);
            let mut j = n - 1;
            for i in 0..n {
                let (x0, y0) = vert(s + j);
                let (x1, y1) = vert(s + i);
                j = i;
                if (y0 > lat) == (y1 > lat) {
                    continue;
                }
                xs.push(x0 + (lat - y0) * (x1 - x0) / (y1 - y0));
            }
        }
        xs.sort_by(|a, b| a.total_cmp(b));
        for k in (0..xs.len().saturating_sub(1)).step_by(2) {
            let a = (((xs[k] + 180.0) * 2.0 - 0.5).ceil() as i64).max(0);
            let b = (((xs[k + 1] + 180.0) * 2.0 - 0.5).floor() as i64).min(GW as i64 - 1);
            for col in a..=b {
                id[row * GW + col as usize] = ci as u8;
            }
        }
    }
}

fn grid() -> &'static Grid {
    static G: OnceLock<Grid> = OnceLock::new();
    G.get_or_init(|| {
        let mut id = vec![SEA; GW * GH];
        for ci in 0..COUNTRIES.len() {
            fill_country(&mut id, ci);
        }
        let mut soft = vec![false; GW * GH];
        for y in 0..GH {
            for x in 0..GW {
                let land = id[y * GW + x] != SEA;
                'n: for dy in -1i64..=1 {
                    for dx in -1i64..=1 {
                        let yy = (y as i64 + dy).clamp(0, GH as i64 - 1) as usize;
                        let xx = (x as i64 + dx).rem_euclid(GW as i64) as usize;
                        if (id[yy * GW + xx] != SEA) != land {
                            soft[y * GW + x] = true;
                            break 'n;
                        }
                    }
                }
            }
        }
        Grid { id, soft }
    })
}

fn wrap_lon(lon: f64) -> f64 {
    if (-180.0..180.0).contains(&lon) {
        lon
    } else {
        lon % 360.0
    }
}

/// The country at a point, or None over sea.
pub fn id(lon: f64, lat: f64) -> Option<usize> {
    if !lon.is_finite() || !lat.is_finite() {
        return None;
    }
    let lon = wrap_lon(lon);
    let x = (((lon + 180.0) * 2.0).floor() as i64).rem_euclid(GW as i64) as usize;
    let y = (((90.0 - lat) * 2.0).floor() as i64).clamp(0, GH as i64 - 1) as usize;
    let v = grid().id[y * GW + x];
    (v != SEA).then_some(v as usize)
}

/// Bilinear land fraction: soft coasts.
pub fn land(lon: f64, lat: f64) -> f64 {
    if !lon.is_finite() || !lat.is_finite() {
        return 0.0;
    }
    let g = grid();
    let lon = wrap_lon(lon);
    let nx = (((lon + 180.0) * 2.0).floor() as i64).rem_euclid(GW as i64) as usize;
    let ny = (((90.0 - lat) * 2.0).floor() as i64).clamp(0, GH as i64 - 1) as usize;
    if !g.soft[ny * GW + nx] {
        return (g.id[ny * GW + nx] != SEA) as u8 as f64;
    }
    let fx = (lon + 180.0) * 2.0 - 0.5;
    let fy = (90.0 - lat) * 2.0 - 0.5;
    let (x0, y0) = (fx.floor(), fy.floor());
    let (tx, ty) = (fx - x0, fy - y0);
    let mut v = 0.0;
    for j in 0..2 {
        for i in 0..2 {
            let xx = ((x0 as i64 + i).rem_euclid(GW as i64)) as usize;
            let yy = (y0 as i64 + j).clamp(0, GH as i64 - 1) as usize;
            if g.id[yy * GW + xx] != SEA {
                v += (if i == 1 { tx } else { 1.0 - tx }) * (if j == 1 { ty } else { 1.0 - ty });
            }
        }
    }
    v
}

pub fn find_iso(iso: &str) -> Option<usize> {
    let up = iso.trim().to_ascii_uppercase();
    COUNTRIES.iter().position(|c| c.iso == up)
}

/// What a skin shows about a country.
pub struct Info {
    pub iso: &'static str,
    pub en: &'static str,
    pub pl: &'static str,
    pub continent: &'static str,
    pub pop_m: f32,
    pub gdp_bn: f32,
}

pub fn info(i: usize) -> Option<Info> {
    let c = COUNTRIES.get(i)?;
    Some(Info { iso: c.iso, en: c.en, pl: c.pl, continent: crate::worlddata::CONTINENTS[c.continent as usize], pop_m: c.pop_m, gdp_bn: c.gdp_bn })
}

// ---- colouring ---------------------------------------------------------------------------------------------------------

pub type Rgb = [f64; 3];

/// How the surface is coloured; one value for both the map and the globe.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Look {
    /// a colour per country instead of climate
    pub political: bool,
    pub grey: bool,
    pub borders: bool,
    pub night: bool,
    pub grid: bool,
    pub sel: Option<usize>,
    /// choropleth: a value per country (None = no data), shown on a ramp from `lo` to `hi`
    pub heat: Vec<Option<f64>>,
    pub lo: f64,
    pub hi: f64,
    pub ramp: Vec<Rgb>,
    /// longitude under the sun for the night side of the flat map
    pub sun_lon: f64,
}

impl Look {
    pub fn has_heat(&self) -> bool {
        self.heat.iter().any(|h| h.is_some())
    }
}

fn clamp1(v: f64) -> f64 {
    if v > 0.0 {
        v.min(1.0)
    } else {
        0.0
    }
}

fn hash2(a: f64, b: f64) -> f64 {
    let h = (a * 12.9898 + b * 78.233).sin() * 43758.5453;
    h - h.floor()
}

/// cheap integer noise for the land texture (the shader runs 100k+ times a frame)
fn noise2(a: i32, b: i32) -> f64 {
    let mut h = (a as u32).wrapping_mul(374761393).wrapping_add((b as u32).wrapping_mul(668265263));
    h = (h ^ (h >> 13)).wrapping_mul(1274126177);
    ((h ^ (h >> 16)) & 0xFFFF) as f64 / 65536.0
}

fn hsv(h: f64, s: f64, v: f64) -> Rgb {
    let h = h - h.floor();
    let c = v * s;
    let x = c * (1.0 - ((h * 6.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match (h * 6.0) as i32 % 6 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    [(r + m) * 255.0, (g + m) * 255.0, (b + m) * 255.0]
}

const RAMP: [Rgb; 4] = [[38.0, 62.0, 146.0], [46.0, 172.0, 160.0], [242.0, 220.0, 62.0], [228.0, 58.0, 40.0]];

fn ramp_rgb(look: &Look, f: f64) -> Rgb {
    let ramp: &[Rgb] = if look.ramp.len() > 1 { &look.ramp } else { &RAMP };
    let n = ramp.len();
    let f = clamp1(f) * (n - 1) as f64;
    let i = (f.floor() as usize).min(n - 2);
    let t = f - i as f64;
    [0, 1, 2].map(|k| ramp[i][k] + (ramp[i + 1][k] - ramp[i][k]) * t)
}

fn surface0(look: &Look, id: Option<usize>, lon: f64, lat: f64, land: f64) -> Rgb {
    let alat = lat.abs();
    let Some(id) = id.filter(|_| land > 0.5) else {
        let mut rgb = [12.0, 62.0, 150.0];
        if alat > 70.0 {
            let ice = clamp1((alat - 70.0) / 6.0);
            for c in &mut rgb {
                *c = *c * (1.0 - ice) + 232.0 * ice;
            }
        }
        return rgb;
    };
    let mut rgb;
    if look.has_heat() {
        rgb = match look.heat.get(id).copied().flatten() {
            Some(v) => ramp_rgb(look, if look.hi > look.lo { (v - look.lo) / (look.hi - look.lo) } else { 0.0 }),
            None => [74.0, 80.0, 78.0], // no data
        };
    } else if look.political {
        rgb = hsv(hash2(id as f64, 3.1), 0.42 + 0.25 * hash2(id as f64, 9.7), 0.62 + 0.3 * hash2(id as f64, 1.3));
    } else {
        let z = (alat - 24.0) / 9.0;
        let desert = (-z * z).exp() * 0.75;
        let ice = clamp1((alat - 62.0) / 12.0);
        let n = noise2((lon * 1.7).floor() as i32, (lat * 1.7).floor() as i32) * 0.16;
        let gr = [40.0 + 25.0 * n, 112.0 + 40.0 * n, 52.0 + 20.0 * n];
        let tan = [176.0, 150.0, 92.0];
        rgb = [0.0; 3];
        for k in 0..3 {
            rgb[k] = gr[k] * (1.0 - desert) + tan[k] * desert;
            rgb[k] = rgb[k] * (1.0 - ice) + 238.0 * ice;
        }
    }
    if Some(id) == look.sel {
        for c in &mut rgb {
            *c = *c * 0.62 + 255.0 * 0.38;
        }
    }
    rgb
}

fn surface(look: &Look, id: Option<usize>, lon: f64, lat: f64, land: f64) -> Rgb {
    let mut rgb = surface0(look, id, lon, lat, land);
    if look.grey {
        let mut y = 0.299 * rgb[0] + 0.587 * rgb[1] + 0.114 * rgb[2];
        y = if id.is_none() || land <= 0.5 { y * 0.5 } else { 70.0 + y * 0.75 };
        rgb = [y; 3];
    }
    rgb
}

/// a thin border: this point and one a pixel away belong to different countries
fn on_border(id: Option<usize>, lon: f64, lat: f64, dx: f64, dy: f64) -> bool {
    let Some(id) = id else { return false };
    [(lon + dx, lat), (lon, lat + dy), (lon - dx, lat), (lon, lat - dy)].iter().any(|&(a, b)| matches!(self::id(a, b), Some(o) if o != id))
}

fn border(look: &Look, id: Option<usize>, on: bool, rgb: &mut Rgb) {
    if !on {
        return;
    }
    let sel = id.is_some() && id == look.sel;
    for c in rgb.iter_mut() {
        *c = if sel { 255.0 } else { *c * 0.42 + 8.0 };
    }
}

// ---- the flat map ------------------------------------------------------------------------------------------------------

/// Zoom 1 fits the whole 2:1 map; more magnifies around (clon, clat), which sits in the middle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub zoom: f64,
    pub clon: f64,
    pub clat: f64,
}

impl Default for View {
    fn default() -> View {
        View { zoom: 1.0, clon: 0.0, clat: 0.0 }
    }
}

const ZMAX: f64 = 64.0;

struct Geom {
    x0: f64,
    y0: f64,
    mw: f64,
    mh: f64,
}

fn geom(pw: f64, ph: f64, v: &View) -> Geom {
    let zoom = if v.zoom >= 1.0 { v.zoom.min(512.0) } else { 1.0 };
    let fin = |x: f64, m: f64| if x.is_finite() { x.clamp(-m, m) } else { 0.0 };
    let (clon, clat) = (fin(v.clon, 180.0), fin(v.clat, 90.0));
    let mw = pw.min(2.0 * ph) * zoom;
    let mh = mw / 2.0;
    Geom { x0: pw / 2.0 - (clon + 180.0) / 360.0 * mw, y0: ph / 2.0 - (90.0 - clat) / 180.0 * mh, mw, mh }
}

impl View {
    pub fn clamped(mut self) -> View {
        if !(self.zoom >= 1.0) {
            self.zoom = 1.0;
        }
        self.zoom = self.zoom.min(ZMAX);
        let (mlon, mlat) = (180.0 * (1.0 - 1.0 / self.zoom), 90.0 * (1.0 - 1.0 / self.zoom)); // at zoom 1 the map stays put
        self.clon = if self.clon.is_finite() { self.clon.clamp(-mlon, mlon) } else { 0.0 };
        self.clat = if self.clat.is_finite() { self.clat.clamp(-mlat, mlat) } else { 0.0 };
        self
    }

    /// Zoom by `factor` keeping the point under pixel (px, py) where it is.
    pub fn zoom_at(&mut self, pw: f64, ph: f64, factor: f64, px: f64, py: f64) {
        let before = lonlat_at(pw, ph, self, px, py);
        self.zoom *= factor;
        *self = self.clamped();
        if let (Some((bl, bt)), Some((al, at))) = (before, lonlat_at(pw, ph, self, px, py)) {
            self.clon += bl - al;
            self.clat += bt - at;
            *self = self.clamped();
        }
    }

    /// Grab: the point under pixel `from` follows the pointer to `to`.
    pub fn grab(&mut self, pw: f64, ph: f64, from: (f64, f64), to: (f64, f64)) {
        if let (Some((l0, t0)), Some((l1, t1))) = (lonlat_at(pw, ph, self, from.0, from.1), lonlat_at(pw, ph, self, to.0, to.1)) {
            self.clon += l0 - l1;
            self.clat += t0 - t1;
            *self = self.clamped();
        }
    }
}

/// Longitude and latitude under a pixel of the (zoomed) flat map; None outside the map.
pub fn lonlat_at(pw: f64, ph: f64, v: &View, px: f64, py: f64) -> Option<(f64, f64)> {
    let g = geom(pw, ph, v);
    let lo = (px - g.x0) / g.mw * 360.0 - 180.0;
    let la = 90.0 - (py - g.y0) / g.mh * 180.0;
    ((-180.0..=180.0).contains(&lo) && (-90.0..=90.0).contains(&la)).then_some((lo, la))
}

pub fn map_country_at(pw: f64, ph: f64, v: &View, px: f64, py: f64) -> Option<usize> {
    let (lo, la) = lonlat_at(pw, ph, v, px, py)?;
    if land(lo, la) > 0.5 {
        id(lo, la)
    } else {
        None
    }
}

/// The colour of one pixel of the flat map; None outside it. `night` shades by `look.sun_lon`.
pub fn map_pixel(look: &Look, v: &View, pw: f64, ph: f64, px: f64, py: f64) -> Option<Rgb> {
    let g = geom(pw, ph, v);
    if px < g.x0 || px >= g.x0 + g.mw || py < g.y0 || py >= g.y0 + g.mh {
        return None;
    }
    let lon = (px - g.x0) / g.mw * 360.0 - 180.0;
    let lat = 90.0 - (py - g.y0) / g.mh * 180.0;
    let ld = land(lon, lat);
    let cid = if ld > 0.5 { id(lon, lat) } else { None };
    let mut base = surface(look, cid, lon, lat, ld);
    if look.borders {
        let d = 360.0 / g.mw * 0.3;
        border(look, cid, on_border(cid, lon, lat, d, d), &mut base);
    }
    let mut lit = 1.0;
    if look.night {
        let z = (lat * PI / 180.0).cos() * ((lon - look.sun_lon) * PI / 180.0).cos();
        lit = 0.16 + 0.84 * clamp1(z * 4.0 + 0.5);
    }
    if look.grid {
        let (a, b) = ((lon % 30.0).abs(), (lat % 30.0).abs());
        let (a, b) = (a.min(30.0 - a), b.min(30.0 - b));
        if a < 0.45 || b < 0.45 {
            for c in &mut base {
                *c = *c * 0.65 + 220.0 * 0.35;
            }
        }
    }
    Some([base[0] * lit, base[1] * lit, base[2] * lit])
}

// ---- the globe ---------------------------------------------------------------------------------------------------------

/// An orthographic sphere: the longitude in the middle, the tilt of the north pole towards the viewer, the sun, a zoom.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Globe {
    pub lon0: f64,
    pub tilt: f64,
    /// 0 = the sun over the viewer's upper left; positive turns it right and behind
    pub sun_deg: f64,
    pub zoom: f64,
}

impl Default for Globe {
    fn default() -> Globe {
        Globe { lon0: 0.0, tilt: 15.0, sun_deg: 0.0, zoom: 1.0 }
    }
}

struct Gl {
    cx: f64,
    cy: f64,
    r: f64,
    lon0: f64,
    sint: f64,
    cost: f64,
    l: [f64; 3],
}

const ATM: f64 = 1.07;

impl Globe {
    pub fn clamped(mut self) -> Globe {
        let f = |x: f64| if x.is_finite() { x } else { 0.0 };
        self.lon0 = (f(self.lon0) + 540.0).rem_euclid(360.0) - 180.0;
        self.tilt = f(self.tilt).clamp(-85.0, 85.0);
        self.sun_deg = f(self.sun_deg);
        self.zoom = if self.zoom >= 1.0 { self.zoom.min(8.0) } else { 1.0 };
        self
    }

    fn gl(&self, pw: f64, ph: f64) -> Gl {
        let t = self.tilt * PI / 180.0;
        let a = self.sun_deg * PI / 180.0;
        let (x, y, z) = ((a - 0.9).sin(), 0.32, (a - 0.9).cos());
        let l = (x * x + y * y + z * z).sqrt();
        Gl { cx: pw / 2.0, cy: ph / 2.0, r: pw.min(ph) / 2.0 / ATM * self.zoom, lon0: self.lon0, sint: t.sin(), cost: t.cos(), l: [x / l, y / l, z / l] }
    }

    /// Degrees of rotation per pixel of drag (about one pixel of surface at the centre).
    pub fn deg_per_px(&self, pw: f64, ph: f64) -> f64 {
        57.3 / self.gl(pw, ph).r
    }
}

fn unproject(g: &Gl, px: f64, py: f64) -> Option<(f64, f64, f64)> {
    let (nx, ny) = ((px - g.cx) / g.r, -(py - g.cy) / g.r);
    let d2 = nx * nx + ny * ny;
    if d2 > 1.0 {
        return None;
    }
    let nz = (1.0 - d2).sqrt();
    let p = ny * g.cost + nz * g.sint;
    let q = -ny * g.sint + nz * g.cost;
    let lat = p.clamp(-1.0, 1.0).asin() * 180.0 / PI;
    let lon = (nx.atan2(q) * 180.0 / PI + g.lon0 + 540.0).rem_euclid(360.0) - 180.0;
    Some((lon, lat, nz))
}

/// lon/lat to pixel plus depth; None on the far side
fn project(g: &Gl, lon: f64, lat: f64) -> Option<(f64, f64, f64)> {
    let (dl, la) = ((lon - g.lon0) * PI / 180.0, lat * PI / 180.0);
    let (p, a, q) = (la.sin(), la.cos() * dl.sin(), la.cos() * dl.cos());
    let z = p * g.sint + q * g.cost;
    if z <= 0.0 {
        return None;
    }
    Some((g.cx + a * g.r, g.cy - (p * g.cost - q * g.sint) * g.r, z))
}

pub fn globe_country_at(gl: &Globe, pw: f64, ph: f64, px: f64, py: f64) -> Option<usize> {
    let (lon, lat, _) = unproject(&gl.gl(pw, ph), px, py)?;
    if land(lon, lat) > 0.5 {
        id(lon, lat)
    } else {
        None
    }
}

/// Where a point of the planet lands in pixels (None on the far side).
pub fn globe_project(gl: &Globe, pw: f64, ph: f64, lon: f64, lat: f64) -> Option<(f64, f64)> {
    project(&gl.gl(pw, ph), lon, lat).map(|(x, y, _)| (x, y))
}

/// One pixel of the globe: colour and opacity (the atmosphere glow fades out; 0 = nothing there).
pub fn globe_pixel(look: &Look, gl: &Globe, pw: f64, ph: f64, px: f64, py: f64) -> Option<(Rgb, f64)> {
    let g = gl.gl(pw, ph);
    let (nx, ny) = ((px - g.cx) / g.r, -(py - g.cy) / g.r);
    let d2 = nx * nx + ny * ny;
    if d2 > ATM * ATM {
        return None;
    }
    let d = d2.sqrt();
    if d > 1.0 {
        // the glow: brighter on the lit side, fading outward
        let f = 1.0 - (d - 1.0) / (ATM - 1.0);
        let lit = clamp1(0.25 + (nx / d * g.l[0] + ny / d * g.l[1]) * 0.9);
        return Some(([70.0, 140.0, 255.0], f * f * lit * 0.85));
    }
    let (lon, lat, nz) = unproject(&g, px, py)?;
    let ld = land(lon, lat);
    let cid = if ld > 0.5 { id(lon, lat) } else { None };
    let mut base = surface(look, cid, lon, lat, ld);
    if look.borders {
        let dpp = 57.3 / (g.r * nz.max(0.25)) * 0.3;
        let cl = (lat * PI / 180.0).cos().max(0.2);
        border(look, cid, on_border(cid, lon, lat, dpp / cl, dpp), &mut base);
    }
    if look.grid {
        let (a, b) = ((lon % 30.0).abs(), (lat % 30.0).abs());
        let (a, b) = (a.min(30.0 - a), b.min(30.0 - b));
        let w = 1.2 / (g.r * (nz + 0.25));
        if a < w * 5.0 || b < w * 5.0 {
            for c in &mut base {
                *c = *c * 0.65 + 220.0 * 0.35;
            }
        }
    }
    let lam = nx * g.l[0] + ny * g.l[1] + nz * g.l[2];
    let lit = 0.06 + 0.98 * clamp1(lam * 1.15 + 0.05);
    let mut spec = 0.0;
    if cid.is_none() && lam > 0.0 {
        let (hx, hy, hz) = (g.l[0], g.l[1], g.l[2] + 1.0);
        let hl = (hx * hx + hy * hy + hz * hz).sqrt();
        spec = ((nx * hx + ny * hy + nz * hz) / hl).max(0.0).powf(60.0) * 0.4;
    }
    let rim = (1.0 - nz).powi(3) * (0.15 + 0.85 * clamp1(lam + 0.3));
    let mut out = [0.0; 3];
    for k in 0..3 {
        out[k] = (base[k] * lit + spec * 255.0 + rim * [60.0, 120.0, 200.0][k]).min(255.0);
    }
    Some((out, 1.0))
}

// ---- pixels ------------------------------------------------------------------------------------------------------------

/// The flat map as RGBA bytes, `w` x `h` square pixels; alpha 0 outside the map.
pub fn render_map(look: &Look, v: &View, w: usize, h: usize, out: &mut Vec<u8>) {
    out.clear();
    out.reserve(w * h * 4);
    let (pw, ph) = (w as f64, h as f64);
    for y in 0..h {
        for x in 0..w {
            match map_pixel(look, v, pw, ph, x as f64 + 0.5, y as f64 + 0.5) {
                Some(c) => out.extend_from_slice(&[c[0] as u8, c[1] as u8, c[2] as u8, 255]),
                None => out.extend_from_slice(&[0, 0, 0, 0]),
            }
        }
    }
}

/// The globe as RGBA bytes; the glow is real alpha.
pub fn render_globe(look: &Look, gl: &Globe, w: usize, h: usize, out: &mut Vec<u8>) {
    out.clear();
    out.reserve(w * h * 4);
    let (pw, ph) = (w as f64, h as f64);
    for y in 0..h {
        for x in 0..w {
            match globe_pixel(look, gl, pw, ph, x as f64 + 0.5, y as f64 + 0.5) {
                Some((c, a)) => out.extend_from_slice(&[c[0] as u8, c[1] as u8, c[2] as u8, (a * 255.0) as u8]),
                None => out.extend_from_slice(&[0, 0, 0, 0]),
            }
        }
    }
}

// ---- labels ------------------------------------------------------------------------------------------------------------

pub struct Label {
    /// centre, in the same pixels as the map
    pub x: f64,
    pub y: f64,
    pub text: String,
}

/// Country names that fit without overlapping, most important first. `to_px` projects a point (None = not visible),
/// `zoom` decides which countries are big enough to name (a cell-sized unit: map width / 14, globe radius / 5), `cw` and
/// `lh` are the size of a character in the pixels of the map.
pub fn labels(to_px: &dyn Fn(f64, f64) -> Option<(f64, f64)>, zoom: f64, polish: bool, w: f64, h: f64, cw: f64, lh: f64) -> Vec<Label> {
    let mut order: Vec<usize> = (0..COUNTRIES.len()).filter(|&i| (COUNTRIES[i].minlabel as f64) <= zoom).collect();
    order.sort_by(|&a, &b| {
        let (x, y) = (&COUNTRIES[a], &COUNTRIES[b]);
        x.minlabel.total_cmp(&y.minlabel).then(y.pop_m.total_cmp(&x.pop_m))
    });
    let mut placed: Vec<(f64, f64, f64)> = Vec::new(); // x0, x1, y
    let mut out = Vec::new();
    for i in order {
        let c = &COUNTRIES[i];
        if matches!(c.iso, "ATA" | "ATF" | "FLK") {
            continue;
        }
        let Some((px, py)) = to_px(c.lx as f64, c.ly as f64) else { continue };
        let full = if polish { c.pl } else { c.en };
        for name in [if full.chars().count() <= 16 { full } else { c.iso }, c.iso] {
            let tw = name.chars().count() as f64 * cw;
            let (x0, x1) = (px - tw / 2.0, px + tw / 2.0);
            if x0 < 0.0 || x1 > w || py < 0.0 || py + lh > h || placed.len() >= 400 {
                continue;
            }
            if placed.iter().any(|&(a, b, y)| (y - py).abs() < lh && x0 - cw < b && x1 + cw > a) {
                continue;
            }
            placed.push((x0, x1, py));
            out.push(Label { x: px, y: py, text: name.to_string() });
            break;
        }
    }
    out
}

pub fn map_labels(v: &View, pw: f64, ph: f64, polish: bool, cw: f64, lh: f64) -> Vec<Label> {
    let g = geom(pw, ph, v);
    labels(
        &|lon, lat| {
            let (x, y) = (g.x0 + (lon + 180.0) / 360.0 * g.mw, g.y0 + (90.0 - lat) / 180.0 * g.mh);
            Some((x, y))
        },
        g.mw / cw / 14.0,
        polish,
        pw,
        ph,
        cw,
        lh,
    )
}

pub fn globe_labels(gl: &Globe, pw: f64, ph: f64, polish: bool, cw: f64, lh: f64) -> Vec<Label> {
    let g = gl.gl(pw, ph);
    labels(&|lon, lat| project(&g, lon, lat).filter(|p| p.2 >= 0.5).map(|p| (p.0, p.1)), g.r / cw / 5.0, polish, pw, ph, cw, lh)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn iso(lon: f64, lat: f64) -> &'static str {
        id(lon, lat).map(|i| COUNTRIES[i].iso).unwrap_or("-")
    }

    #[test]
    fn known_places_are_in_the_right_country() {
        assert_eq!(iso(19.0, 52.0), "POL");
        assert_eq!(iso(2.3, 48.8), "FRA");
        assert_eq!(iso(-100.0, 40.0), "USA");
        assert_eq!(iso(135.0, -25.0), "AUS");
        assert_eq!(iso(-30.0, 30.0), "-", "mid atlantic is sea");
        assert_eq!(iso(f64::NAN, 0.0), "-");
        assert_eq!(iso(19.0 + 360.0, 52.0), "POL", "longitude wraps");
    }

    #[test]
    fn land_is_soft_only_on_coasts() {
        assert_eq!(land(19.0, 52.0), 1.0);
        assert_eq!(land(-30.0, 30.0), 0.0);
        let coast = land(18.8, 54.6);
        assert!((0.0..=1.0).contains(&coast));
    }

    #[test]
    fn iso_lookup_and_info() {
        let p = find_iso("pol").unwrap();
        let i = info(p).unwrap();
        assert_eq!((i.iso, i.pl, i.continent), ("POL", "Polska", "Europe"));
        assert!(find_iso("XXX").is_none() && info(9999).is_none());
    }

    #[test]
    fn map_zoom_keeps_the_point_under_the_cursor() {
        let mut v = View::default();
        let (pw, ph) = (160.0, 80.0);
        let (px, py) = (100.0, 25.0);
        let before = lonlat_at(pw, ph, &v, px, py).unwrap();
        v.zoom_at(pw, ph, 4.0, px, py);
        let after = lonlat_at(pw, ph, &v, px, py).unwrap();
        assert!((before.0 - after.0).abs() < 1e-6 && (before.1 - after.1).abs() < 1e-6, "{before:?} {after:?}");
        assert!(v.zoom > 3.9);
        v.zoom_at(pw, ph, 1e-9, px, py);
        assert_eq!(v, View::default(), "zooming all the way out re-centres");
    }

    #[test]
    fn grab_moves_the_map_with_the_pointer() {
        let mut v = View { zoom: 4.0, clon: 0.0, clat: 0.0 };
        let (pw, ph) = (160.0, 80.0);
        let p0 = lonlat_at(pw, ph, &v, 80.0, 40.0).unwrap();
        v.grab(pw, ph, (80.0, 40.0), (90.0, 40.0));
        let p1 = lonlat_at(pw, ph, &v, 90.0, 40.0).unwrap();
        assert!((p0.0 - p1.0).abs() < 1e-6, "the grabbed point is under the pointer");
        assert!(v.clon < 0.0, "dragging right moves the view west");
    }

    #[test]
    fn map_pixels_outside_the_map_are_none_and_sea_is_blue() {
        let look = Look { borders: true, ..Look::default() };
        let v = View::default();
        assert!(map_pixel(&look, &v, 200.0, 200.0, 100.0, 5.0).is_none(), "above the 2:1 map");
        let sea = map_pixel(&look, &v, 200.0, 100.0, 83.0, 33.0).unwrap(); // atlantic
        assert!(sea[2] > sea[0] && sea[2] > sea[1]);
    }

    #[test]
    fn globe_centre_is_the_longitude_and_far_side_is_hidden() {
        let g = Globe { lon0: 19.0, tilt: 0.0, ..Globe::default() };
        let (pw, ph) = (100.0, 100.0);
        let (lon, lat, nz) = unproject(&g.gl(pw, ph), 50.0, 50.0).unwrap();
        assert!((lon - 19.0).abs() < 1e-6 && lat.abs() < 1e-6 && (nz - 1.0).abs() < 1e-9);
        assert!(globe_project(&g, pw, ph, 19.0 + 180.0, 0.0).is_none());
        let (x, y) = globe_project(&g, pw, ph, 19.0, 0.0).unwrap();
        assert!((x - 50.0).abs() < 1e-6 && (y - 50.0).abs() < 1e-6);
        assert!(globe_pixel(&Look::default(), &g, pw, ph, 0.5, 0.5).is_none(), "the corner is outside the glow");
    }

    #[test]
    fn globe_picks_the_country_it_shows() {
        let g = Globe { lon0: 19.0, tilt: 52.0, ..Globe::default() }; // the tilt is the latitude in the middle
        let (pw, ph) = (200.0, 200.0);
        let (x, y) = globe_project(&g, pw, ph, 19.0, 52.0).unwrap();
        assert_eq!(globe_country_at(&g, pw, ph, x, y).map(|i| COUNTRIES[i].iso), Some("POL"));
    }

    #[test]
    fn heat_colours_countries_and_greys_the_rest() {
        let mut heat = vec![None; count()];
        heat[find_iso("POL").unwrap()] = Some(1.0);
        let look = Look { heat, lo: 0.0, hi: 1.0, ..Look::default() };
        let pol = surface(&look, find_iso("POL"), 19.0, 52.0, 1.0);
        let fra = surface(&look, find_iso("FRA"), 2.0, 47.0, 1.0);
        assert_eq!(fra, [74.0, 80.0, 78.0]);
        assert!(pol[0] > 200.0 && pol[2] < 100.0, "top of the ramp is red: {pol:?}");
    }

    #[test]
    fn labels_do_not_overlap_and_grow_with_zoom() {
        let v = View::default();
        let small = map_labels(&v, 120.0, 60.0, false, 1.0, 2.0);
        let big = map_labels(&v, 480.0, 240.0, false, 1.0, 2.0);
        assert!(big.len() > small.len() && !small.is_empty());
        for (i, a) in big.iter().enumerate() {
            for b in &big[i + 1..] {
                let (wa, wb) = (a.text.chars().count() as f64, b.text.chars().count() as f64);
                let clash = (a.y - b.y).abs() < 2.0 && (a.x - b.x).abs() < (wa + wb) / 2.0;
                assert!(!clash, "{} / {}", a.text, b.text);
            }
        }
    }

    #[test]
    fn rgba_has_four_bytes_a_pixel() {
        let mut out = Vec::new();
        render_map(&Look::default(), &View::default(), 20, 10, &mut out);
        assert_eq!(out.len(), 20 * 10 * 4);
        render_globe(&Look::default(), &Globe::default(), 12, 12, &mut out);
        assert_eq!(out.len(), 12 * 12 * 4);
        assert_eq!(out[3], 0, "corner transparent");
    }
}
