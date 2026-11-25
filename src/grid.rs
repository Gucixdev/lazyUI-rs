//! `grid`: a virtual grid of tiles, each a picture with a label under it (a folder of thumbnails). `count=N` (or `count=@KEY`, read from the model every frame) tiles of
//! `tile=WxH` cells (the last row of a tile is its label). Only the tiles in view are drawn, so a million is as cheap
//! as ten. Tile `i` shows the picture registered under `{prefix}{i}` (`Ui::set_image`) and the model text `{prefix}{i}`
//! as its label (the number when there is none). While the view moves, a `need` event asks for what is about to be
//! seen: `index` is the first tile, `number` how many (one screenful ahead); the host decodes those and answers.
//! Arrows, page keys, home and end move the selection, the wheel moves it by a row, a click selects and a click on the
//! selected tile or enter activates. `change` and `activate` carry the tile's label and index; `bind=KEY` gets the index.

use crate::canvas::{Canvas, Rect, Style};
use crate::input::{Event, Key, MouseKind};
use crate::native::{Cx, Def, EvCx, Widget};
use crate::ui::Value;
use crate::widgets::Res;

pub static DEFS: &[Def] = &[Def {
    name: "grid",
    doc: "virtual tile grid: count=N tile=14x7 prefix=t (picture and label of tile i are keyed t<i>), bind=KEY; need events ask for the tiles about to show",
    focus: true,
    rows: None,
    live: 0.0,
    make: || Box::new(Grid::default()),
}];

#[derive(Default)]
pub struct Grid {
    count: usize,
    cols: usize,
    rows: usize,
    first_row: usize,
    sel: usize,
    tile: (i32, i32),
    prefix: String,
    /// what the last draw wants loaded, and what the last `need` asked for
    want: (usize, usize),
    asked: Option<(usize, usize)>,
    area: Rect,
}

fn tile_size(cx_attr: Option<&str>) -> (i32, i32) {
    let parsed = cx_attr.and_then(|s| s.split_once('x')).and_then(|(w, h)| Some((w.trim().parse::<i32>().ok()?, h.trim().parse::<i32>().ok()?)));
    let (w, h) = parsed.unwrap_or((14, 7));
    (w.max(2), h.max(2))
}

impl Grid {
    /// Keep the selection in view and work out which tiles want loading; after every change of `sel` or the size.
    fn reflow(&mut self) {
        self.sel = self.sel.min(self.count.saturating_sub(1));
        let sel_row = self.sel / self.cols.max(1);
        if sel_row < self.first_row {
            self.first_row = sel_row;
        } else if sel_row >= self.first_row + self.rows {
            self.first_row = sel_row + 1 - self.rows;
        }
        let total_rows = self.count.div_ceil(self.cols.max(1));
        self.first_row = self.first_row.min(total_rows.saturating_sub(self.rows));
        let first = self.first_row * self.cols;
        let ahead = ((self.first_row + self.rows + 1) * self.cols).min(self.count);
        self.want = (first, ahead.saturating_sub(first));
    }

    fn label(&self, i: usize, model: &dyn Fn(&str) -> Option<String>) -> String {
        model(&format!("{}{i}", self.prefix)).unwrap_or_else(|| i.to_string())
    }

    fn move_to(&mut self, i: i64, cx: &mut EvCx) -> Res {
        let i = i.clamp(0, self.count.saturating_sub(1) as i64) as usize;
        if i == self.sel {
            return Res::Handled;
        }
        self.sel = i;
        self.reflow();
        let name = cx.model_text(&format!("{}{i}", self.prefix)).unwrap_or_else(|| i.to_string());
        cx.emit("change", &name, i as i64, i as f64);
        cx.set_bound(Value::Num(i as f64));
        Res::Changed
    }

    fn activate(&mut self, cx: &mut EvCx) -> Res {
        let name = cx.model_text(&format!("{}{}", self.prefix, self.sel)).unwrap_or_else(|| self.sel.to_string());
        cx.emit("activate", &name, self.sel as i64, self.sel as f64);
        Res::Activate
    }
}

impl Widget for Grid {
    fn draw(&mut self, c: &mut Canvas, r: Rect, cx: &Cx) {
        // `count=N`, or `count=@KEY` to follow a model key (a folder that changes)
        self.count = match cx.attr("count").and_then(|v| v.strip_prefix('@')) {
            Some(k) => cx.model_num(k).unwrap_or(0.0),
            None => cx.num_attr("count", 0.0),
        }
        .max(0.0) as usize;
        self.tile = tile_size(cx.attr("tile"));
        self.prefix = cx.text_attr("prefix", "t");
        self.area = r;
        self.cols = (r.w / self.tile.0).max(1) as usize;
        self.rows = (r.h / self.tile.1).max(1) as usize;
        if let Some(k) = cx.attr("bind").and_then(|k| cx.model_num(k)) {
            if k >= 0.0 {
                self.sel = k as usize;
            }
        }
        self.reflow();
        let t = cx.theme;
        let (tw, th) = self.tile;
        let first = self.first_row * self.cols;
        for i in first..self.count.min(first + self.rows * self.cols) {
            let (col, row) = ((i % self.cols) as i32, (i / self.cols - self.first_row) as i32);
            let tile = Rect::new(r.x + col * tw, r.y + row * th, tw, th);
            let on = i == self.sel;
            let base = if on { t.selected(cx.focused) } else { Style::fg(t.fg).on(t.bg) };
            c.fill(tile, ' ', base);
            let pic = Rect::new(tile.x + 1, tile.y, (tw - 2).max(1), th - 1);
            match cx.image(&format!("{}{i}", self.prefix)) {
                Some(img) => c.image(pic, img),
                None => c.text_fit(pic.x, pic.y + pic.h / 2, pic.w, "...", Style::fg(t.dim).on(base.bg)),
            }
            let label = self.label(i, &|k| cx.model_text(k));
            c.text_fit(tile.x + 1, tile.y + th - 1, tw - 2, &label, base);
        }
    }

    fn tick(&mut self, _dt: f64, cx: &mut EvCx) {
        if self.count > 0 && self.asked != Some(self.want) {
            self.asked = Some(self.want);
            cx.emit("need", "", self.want.0 as i64, self.want.1 as f64);
        }
    }

    fn event(&mut self, r: Rect, ev: &Event, cx: &mut EvCx) -> Res {
        if self.count == 0 {
            return Res::Ignored;
        }
        let (cols, page, sel) = (self.cols as i64, (self.cols * self.rows) as i64, self.sel as i64);
        match ev {
            Event::Key(Key::Left, _) => self.move_to(sel - 1, cx),
            Event::Key(Key::Right, _) => self.move_to(sel + 1, cx),
            Event::Key(Key::Up, _) => self.move_to(sel - cols, cx),
            Event::Key(Key::Down, _) => self.move_to(sel + cols, cx),
            Event::Key(Key::PageUp, _) => self.move_to(sel - page, cx),
            Event::Key(Key::PageDown, _) => self.move_to(sel + page, cx),
            Event::Key(Key::Home, _) => self.move_to(0, cx),
            Event::Key(Key::End, _) => self.move_to(self.count as i64 - 1, cx),
            Event::Key(Key::Enter, _) => self.activate(cx),
            Event::Mouse(MouseKind::WheelUp, ..) => self.move_to(sel - cols, cx),
            Event::Mouse(MouseKind::WheelDown, ..) => self.move_to(sel + cols, cx),
            Event::Mouse(MouseKind::Down(0), x, y, _) if r.contains(*x, *y) => {
                let (col, row) = (((*x - r.x) / self.tile.0) as usize, ((*y - r.y) / self.tile.1) as usize);
                let i = (self.first_row + row) * self.cols + col;
                if col >= self.cols || i >= self.count {
                    return Res::Handled;
                }
                if i == self.sel {
                    self.activate(cx)
                } else {
                    self.move_to(i as i64, cx)
                }
            }
            _ => Res::Ignored,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::canvas::{Caps, Canvas};
    use crate::headless::{run_script, snapshot};
    use crate::media::Image;
    use crate::ui::Ui;

    const SPEC: &str = "col\n  grid id=g count=1000000 tile=8x4 prefix=t bind=cur\n";

    fn solid(c: u8) -> Image {
        Image::new(2, 2, [c, c, c, 255].iter().copied().cycle().take(16).collect()).unwrap()
    }

    #[test]
    fn draws_only_what_is_in_view_and_asks_for_it_one_screenful_ahead() {
        let mut ui = Ui::from_spec(SPEC).unwrap();
        run_script(&mut ui, "size 32 8\nset t0=a.png\nset t5=f.png").unwrap();
        let s = snapshot(&mut ui, 32, 8);
        run_script(&mut ui, "size 32 8\ntick 0.1").unwrap();
        assert!(s.contains("a.png") && s.contains("f.png") && s.contains("..."), "4 columns, 2 rows:\n{s}");
        assert!(!s.contains("8"), "tile 8 is on the third row, out of view:\n{s}");
        let need: Vec<_> = ui.take_events().into_iter().filter(|e| e.kind == "need").collect();
        assert_eq!((need.len(), need[0].index, need[0].number), (1, 0, 12.0), "two rows in view plus one ahead, 4 per row");
        run_script(&mut ui, "size 32 8\ntick 0.1").unwrap();
        assert!(ui.take_events().iter().all(|e| e.kind != "need"), "not asked twice");
    }

    #[test]
    fn count_can_follow_a_model_key() {
        let mut ui = Ui::from_spec("col\n  grid id=g count=@n tile=8x4 prefix=t\n").unwrap();
        run_script(&mut ui, "size 32 8\nset n=0").unwrap();
        assert!(!snapshot(&mut ui, 32, 8).contains("..."));
        run_script(&mut ui, "size 32 8\nset n=3").unwrap();
        assert_eq!(snapshot(&mut ui, 32, 8).matches("...").count(), 3);
    }

    #[test]
    fn registered_pictures_are_put_in_their_tiles() {
        let mut ui = Ui::from_spec(SPEC).unwrap();
        run_script(&mut ui, "size 32 8").unwrap();
        ui.set_image("t1", solid(200));
        ui.set_image("t2", solid(100));
        let mut c = Canvas::new(32, 8, Caps::FULL);
        ui.render(&mut c);
        let at: Vec<_> = c.images().iter().map(|p| (p.rect.x, p.rect.y)).collect();
        assert_eq!(at, vec![(9, 0), (17, 0)], "tile 1 and 2 of the first row, one cell in from the tile edge");
    }

    #[test]
    fn arrows_scroll_and_a_million_tiles_cost_nothing_at_the_end() {
        let mut ui = Ui::from_spec(SPEC).unwrap();
        run_script(&mut ui, "size 32 8\nfocus g\nkey right\nexpect event g change 1\nexpect model cur 1\nkey down\nexpect model cur 5\nkey down\nkey down").unwrap();
        run_script(&mut ui, "size 32 8\ntick 0.1").unwrap();
        let need: Vec<_> = ui.take_events().into_iter().filter(|e| e.kind == "need").collect();
        assert!(need.last().unwrap().index >= 4, "the view followed the selection down: {need:?}");
        run_script(&mut ui, "size 32 8\nkey end\ntick 0.1").unwrap();
        let s = snapshot(&mut ui, 32, 8);
        assert!(s.contains("999999"), "{s}");
        let need: Vec<_> = ui.take_events().into_iter().filter(|e| e.kind == "need").collect();
        let n = need.last().unwrap();
        assert!(n.index >= 999_990 && n.index + n.number as i64 <= 1_000_000, "{n:?}");
        run_script(&mut ui, "size 32 8\nkey home\nexpect model cur 0").unwrap();
    }

    #[test]
    fn enter_and_a_second_click_activate_and_a_click_selects() {
        let mut ui = Ui::from_spec(SPEC).unwrap();
        run_script(&mut ui, "size 32 8\nset t2=c.png\nclick 17 1\nexpect event g change c.png\nclick 17 1\nexpect event g activate c.png").unwrap();
        run_script(&mut ui, "size 32 8\nkey right\nkey enter\nexpect event g activate 3").unwrap();
        run_script(&mut ui, "size 32 8\nclick 31 7\nexpect model cur 7").unwrap();
    }
}

#[cfg(test)]
mod timing {
    use crate::canvas::{Caps, Canvas};
    use crate::ui::Ui;
    use std::time::Instant;

    /// A folder of 100k entries: a frame must cost the visible tiles, not the folder. `cargo test --release grid::timing -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn a_frame_of_a_100k_folder() {
        let mut ui = Ui::from_spec("col\n  grid id=g count=100000 tile=14x7 prefix=t bind=cur\n").unwrap();
        for i in 0..100_000 {
            ui.set_text(&format!("t{i}"), &format!("file-{i}.jpg"));
        }
        let mut c = Canvas::new(200, 60, Caps::FULL);
        let t = Instant::now();
        ui.render(&mut c);
        let first = t.elapsed();
        let t = Instant::now();
        for i in 0..100 {
            ui.set_text("cur", &format!("{}", i * 900));
            ui.render(&mut c);
        }
        println!("first frame {first:?}, 100 scrolled frames {:?}", t.elapsed());
    }
}

#[cfg(test)]
mod timing_lists {
    use crate::canvas::{Caps, Canvas};
    use crate::ui::Ui;
    use std::time::Instant;

    /// 100k rows in a `list` and in a `table` (rows from a model text): first frame and 100 frames moving the cursor.
    /// `cargo test --release grid::timing_lists -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn a_frame_of_100k_rows() {
        let names: Vec<String> = (0..100_000).map(|i| format!("file-{i}.jpg")).collect();
        let mut ui = Ui::from_spec("col\n  list id=l size=fill\n").unwrap();
        let t = Instant::now();
        ui.set_list("l", names.clone());
        let set = t.elapsed();
        let mut c = Canvas::new(120, 50, Caps::FULL);
        let t = Instant::now();
        ui.render(&mut c);
        let first = t.elapsed();
        ui.focus("l");
        let t = Instant::now();
        for _ in 0..100 {
            ui.dispatch(&crate::input::Event::Key(crate::input::Key::PageDown, crate::input::Mods::NONE));
            ui.render(&mut c);
        }
        println!("list: set {set:?}, first frame {first:?}, 100 pgdn frames {:?}", t.elapsed());

        let rows: String = (0..100_000).map(|i| format!("file-{i}.jpg,{}", i * 7)).collect::<Vec<_>>().join(";");
        let mut ui = Ui::from_spec("col\n  table id=t columns=\"name,size\" from=rows size=fill\n").unwrap();
        ui.set_text("rows", &rows);
        let t = Instant::now();
        ui.render(&mut c);
        let first = t.elapsed();
        ui.focus("t");
        let t = Instant::now();
        for _ in 0..100 {
            ui.dispatch(&crate::input::Event::Key(crate::input::Key::PageDown, crate::input::Mods::NONE));
            ui.render(&mut c);
        }
        println!("table: first frame {first:?}, 100 pgdn frames {:?}", t.elapsed());
    }
}

#[cfg(test)]
mod rows_with_tabs {
    use crate::headless::{run_script, snapshot};
    use crate::ui::Ui;

    #[test]
    fn a_tab_in_a_list_row_puts_the_rest_at_the_right_edge() {
        let mut ui = Ui::from_spec("col\n  list id=g size=fill\n").unwrap();
        ui.set_list("g", vec!["Dota 2\tGE-Proton".into(), "plain".into(), "narrow-name-here\tvery-long-right-part".into()]);
        run_script(&mut ui, "size 30 4").unwrap();
        let s = snapshot(&mut ui, 30, 4);
        let rows: Vec<&str> = s.lines().collect();
        assert!(rows[0].contains("Dota 2") && rows[0].trim_end().ends_with("GE-Proton"), "{s}");
        assert!(rows[0].trim_end().len() == 29 || rows[0].trim_end().len() == 30, "{s}");
        assert_eq!(rows[1].trim_end(), "  plain");
        assert!(rows[2].contains("narrow-name-here") && !rows[2].contains("very"), "does not fit: left part only\n{s}");
    }
}
