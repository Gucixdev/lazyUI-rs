//! Headless mode: no terminal, a virtual clock. Frames are text, the tree is JSON, scripts drive the Ui and assert.
//! This is what the tests, the `snap`/`tree`/`script` commands and agents use.

use crate::canvas::{Caps, Canvas};
use crate::input::{parse_key, Event, Mods, MouseKind};
use crate::ui::Ui;

/// Step the virtual clock until every spring is at rest (or `max` seconds passed).
pub fn settle(ui: &mut Ui, w: i32, h: i32, max: f64) {
    let mut c = Canvas::new(w, h, Caps::ASCII);
    ui.render(&mut c); // springs are created on first render
    let mut t = 0.0;
    while ui.animating() && t < max {
        ui.tick(1.0 / 60.0);
        ui.render(&mut c);
        t += 1.0 / 60.0;
    }
}

/// One settled frame as plain text (ASCII glyphs: stable across fonts and terminals).
pub fn snapshot(ui: &mut Ui, w: i32, h: i32) -> String {
    settle(ui, w, h, 10.0);
    let mut c = Canvas::new(w, h, Caps::ASCII);
    ui.render(&mut c);
    c.to_text()
}

pub fn tree(ui: &mut Ui, w: i32, h: i32) -> String {
    settle(ui, w, h, 10.0);
    ui.tree_json()
}

/// Run a script. Returns what `snap`/`tree`/`events` printed, or `Err("line N: ...")` at the first failed expectation.
///
/// ```text
/// size 40 10        # canvas size (default 80 24)
/// set cpu=42        # model value
/// key ctrl-r        # key by name
/// type hello        # characters one by one
/// click 3 4         # left click at column 3 row 4
/// wheel 3 4 down    # wheel
/// paste some text
/// tick 0.5          # advance the virtual clock
/// hotkey ctrl-q quit
/// focus q
/// snap              # print the settled frame
/// tree              # print the tree JSON
/// events            # print and clear the event queue as JSON lines
/// expect text "50%"          # frame contains
/// expect no-text "error"
/// expect event q change ab   # in the event queue (text optional); consumed
/// expect focus q
/// expect model cpu 42
/// expect row 3 "> alpha"     # frame line equals (trailing spaces ignored)
/// ```
pub fn run_script(ui: &mut Ui, script: &str) -> Result<String, String> {
    let (mut w, mut h) = (80, 24);
    let mut out = String::new();
    let mut pending = Vec::new();
    for (n, raw) in script.lines().enumerate() {
        let n = n + 1;
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (cmd, rest) = line.split_once(' ').unwrap_or((line, ""));
        let rest = rest.trim();
        let fail = |m: String| format!("line {n}: {m}");
        let args = words(rest).map_err(|e| fail(e))?;
        match cmd {
            "size" => {
                let (a, b) = two_ints(&args).ok_or_else(|| fail("size W H".into()))?;
                w = a.clamp(1, 1000);
                h = b.clamp(1, 1000);
                ui.dispatch(&Event::Resize(w, h));
            }
            "set" => {
                let (k, v) = rest.split_once('=').ok_or_else(|| fail("set key=value".into()))?;
                ui.set_auto(k.trim(), v.trim());
            }
            "key" => {
                for a in &args {
                    let (k, m) = parse_key(a).ok_or_else(|| fail(format!("unknown key `{a}`")))?;
                    relayout_if_needed(ui, w, h);
                    ui.dispatch(&Event::Key(k, m));
                }
            }
            "type" => {
                relayout_if_needed(ui, w, h);
                for ch in rest.chars() {
                    ui.dispatch(&Event::ch(ch));
                }
            }
            "paste" => {
                relayout_if_needed(ui, w, h);
                ui.dispatch(&Event::Paste(rest.to_string()));
            }
            "click" => {
                let (x, y) = two_ints(&args).ok_or_else(|| fail("click X Y".into()))?;
                relayout_if_needed(ui, w, h);
                ui.dispatch(&Event::Mouse(MouseKind::Down(0), x, y, Mods::NONE));
                ui.dispatch(&Event::Mouse(MouseKind::Up(0), x, y, Mods::NONE));
            }
            "wheel" => {
                let (x, y) = two_ints(&args).ok_or_else(|| fail("wheel X Y up|down".into()))?;
                let k = if args.get(2).map(|s| s.as_str()) == Some("up") { MouseKind::WheelUp } else { MouseKind::WheelDown };
                relayout_if_needed(ui, w, h);
                ui.dispatch(&Event::Mouse(k, x, y, Mods::NONE));
            }
            "tick" => {
                let s: f64 = args.first().and_then(|a| a.parse().ok()).ok_or_else(|| fail("tick SECONDS".into()))?;
                let mut left = s.clamp(0.0, 3600.0);
                while left > 0.0 {
                    let d = left.min(0.05);
                    ui.tick(d);
                    left -= d;
                }
            }
            "hotkey" => {
                if args.len() != 2 || !ui.hotkey(&args[0], &args[1]) {
                    return Err(fail("hotkey KEY EVENT (key names like ctrl-q, f5, enter)".into()));
                }
            }
            "every" => {
                let s: f64 = args.first().and_then(|a| a.parse().ok()).ok_or_else(|| fail("every SECONDS EVENT".into()))?;
                ui.every(s, args.get(1).map(|s| s.as_str()).unwrap_or("tick"));
            }
            "focus" => {
                relayout_if_needed(ui, w, h);
                let id = args.first().ok_or_else(|| fail("focus ID".into()))?;
                if !ui.focus(id) {
                    return Err(fail(format!("no focusable element `{id}`")));
                }
            }
            "snap" => {
                out.push_str(&snapshot(ui, w, h));
            }
            "tree" => {
                out.push_str(&tree(ui, w, h));
                out.push('\n');
            }
            "events" => {
                for e in pending.drain(..).chain(ui.take_events()) {
                    out.push_str(&e.to_json());
                    out.push('\n');
                }
            }
            "expect" => {
                let (what, a) = args.split_first().ok_or_else(|| fail("expect KIND ...".into()))?;
                match what.as_str() {
                    "text" | "no-text" => {
                        let needle = a.first().ok_or_else(|| fail(format!("expect {what} \"...\"")))?;
                        let f = snapshot(ui, w, h);
                        let has = f.contains(needle.as_str());
                        if has != (what == "text") {
                            return Err(fail(format!("expected the frame to {}contain `{needle}`:\n{f}", if what == "text" { "" } else { "not " })));
                        }
                    }
                    "row" => {
                        let r: usize = a.first().and_then(|s| s.parse().ok()).ok_or_else(|| fail("expect row N \"text\"".into()))?;
                        let want = a.get(1).map(|s| s.as_str()).unwrap_or("");
                        let f = snapshot(ui, w, h);
                        let got = f.lines().nth(r).unwrap_or("");
                        if got.trim_end() != want.trim_end() {
                            return Err(fail(format!("row {r} is `{got}`, expected `{want}`")));
                        }
                    }
                    "focus" => {
                        relayout_if_needed(ui, w, h);
                        let want = a.first().map(|s| s.as_str()).unwrap_or("");
                        if ui.focused() != Some(want) {
                            return Err(fail(format!("focus is {:?}, expected `{want}`", ui.focused())));
                        }
                    }
                    "model" => {
                        let (k, v) = (a.first().map(|s| s.as_str()).unwrap_or(""), a.get(1).map(|s| s.as_str()).unwrap_or(""));
                        let got = ui.text_of(k);
                        if got != v {
                            return Err(fail(format!("model `{k}` is `{got}`, expected `{v}`")));
                        }
                    }
                    "event" => {
                        let (id, kind) = (a.first().map(|s| s.as_str()).unwrap_or(""), a.get(1).map(|s| s.as_str()).unwrap_or(""));
                        let text = a.get(2);
                        pending.extend(ui.take_events());
                        let pos = pending.iter().position(|e| e.id == id && e.kind == kind && text.map_or(true, |t| &e.text == t));
                        match pos {
                            Some(p) => {
                                pending.remove(p);
                            }
                            None => {
                                let seen: Vec<String> = pending.iter().map(|e| e.to_json()).collect();
                                return Err(fail(format!("no event {id} {kind}; queue: [{}]", seen.join(","))));
                            }
                        }
                    }
                    other => return Err(fail(format!("unknown expectation `{other}`"))),
                }
            }
            other => return Err(fail(format!("unknown command `{other}`"))),
        }
    }
    Ok(out)
}

fn relayout_if_needed(ui: &mut Ui, w: i32, h: i32) {
    if ui.size() != (w, h) || ui.dirty {
        let mut c = Canvas::new(w, h, Caps::ASCII);
        ui.render(&mut c);
    }
}

fn two_ints(a: &[String]) -> Option<(i32, i32)> {
    Some((a.first()?.parse().ok()?, a.get(1)?.parse().ok()?))
}

/// Split on spaces; double quotes keep a phrase together.
fn words(s: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let (mut q, mut tok) = (false, false);
    for c in s.chars() {
        match c {
            '"' => {
                q = !q;
                tok = true;
            }
            ' ' if !q => {
                if tok {
                    out.push(std::mem::take(&mut cur));
                    tok = false;
                }
            }
            c => {
                cur.push(c);
                tok = true;
            }
        }
    }
    if q {
        return Err("a quote is not closed".into());
    }
    if tok {
        out.push(cur);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEC: &str = "col\n  searchbar id=q prompt=\"> \" bind=query\n  list id=files items=\"alpha,beta,gamma\" filter=query\n  progressbar bind=load max=10 size=1\n";

    #[test]
    fn script_drives_and_asserts() {
        let mut ui = Ui::from_spec(SPEC).unwrap();
        let s = "size 30 6\nexpect focus q\ntype ta\nexpect event q change ta\nexpect model query ta\nexpect text \"beta\"\nexpect no-text \"alpha\"\nkey tab\nexpect focus files\nkey down\nkey enter\nexpect event files activate\nset load=5\nexpect text \"50%\"\nsnap\n";
        let out = run_script(&mut ui, s).unwrap();
        assert!(out.contains("> ta") && out.contains("> beta"), "{out}");
    }

    #[test]
    fn failures_name_the_line() {
        let mut ui = Ui::from_spec(SPEC).unwrap();
        let e = run_script(&mut ui, "size 20 4\n\nexpect text \"nope\"\n").unwrap_err();
        assert!(e.starts_with("line 3: expected the frame to contain `nope`"), "{e}");
        assert!(run_script(&mut ui, "frobnicate").unwrap_err().contains("unknown command"));
        assert!(run_script(&mut ui, "key hyper-x").unwrap_err().starts_with("line 1: unknown key"));
        assert!(run_script(&mut ui, "expect event q activate").unwrap_err().contains("no event q activate"));
    }

    #[test]
    fn timers_and_progress_settle_on_the_virtual_clock() {
        let mut ui = Ui::from_spec(SPEC).unwrap();
        run_script(&mut ui, "set load=10\nevery 1 beat\ntick 3.5\nexpect event app beat\nexpect event app beat\nexpect event app beat\nexpect text \"100%\"").unwrap();
        // the same script gives the same frame twice: determinism
        let a = run_script(&mut ui, "snap").unwrap();
        let b = run_script(&mut ui, "snap").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn tree_and_events_commands_print_json() {
        let mut ui = Ui::from_spec(SPEC).unwrap();
        let o = run_script(&mut ui, "size 30 6\ntype a\ntree\nevents\n").unwrap();
        assert!(o.contains("\"kind\":\"searchbar\"") && o.contains("\"value\":\"a\""), "{o}");
        assert!(o.contains("{\"id\":\"q\",\"kind\":\"change\",\"text\":\"a\""), "{o}");
    }
}
