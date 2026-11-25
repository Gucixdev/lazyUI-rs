//! Client of finedesktop's control socket: one line in, one line out (`send`), or a stream of events (`subscribe`).
//! The socket is `$FD_SOCK`, else the newest `$XDG_RUNTIME_DIR/finedesktop-*.sock`. A lazyui program running inside
//! the desktop uses it to follow the theme (`theme NAME` events) and to ask for actions, with nothing linked to finedesktop.
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};

/// Where the running desktop listens, if it does.
pub fn socket() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("FD_SOCK").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(p));
    }
    let dir = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/tmp"));
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            n.starts_with("finedesktop-") && n.ends_with(".sock")
        })
        .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok())
        .map(|e| e.path())
}

/// Run one action (`layout grid`, `theme dracula`, `state`) and return the reply line; an `error: ...` reply is an `Err`.
pub fn send(sock: &Path, action: &str) -> std::io::Result<String> {
    let mut s = UnixStream::connect(sock)?;
    s.set_read_timeout(Some(std::time::Duration::from_secs(5)))?;
    s.write_all(action.trim().as_bytes())?;
    s.write_all(b"\n")?;
    let mut line = String::new();
    BufReader::new(&s).read_line(&mut line)?;
    let line = line.trim_end().to_string();
    if line.starts_with("error:") {
        return Err(std::io::Error::other(line));
    }
    Ok(line)
}

/// One event line: `kind` is the first word (`theme`, `focus`, `layout`, ...), `rest` the remainder.
#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    pub kind: String,
    pub rest: String,
}

impl Event {
    pub fn parse(line: &str) -> Event {
        let line = line.trim();
        match line.split_once(char::is_whitespace) {
            Some((k, r)) => Event { kind: k.to_string(), rest: r.trim().to_string() },
            None => Event { kind: line.to_string(), rest: String::new() },
        }
    }
}

/// A blocking stream of events; ends when the desktop closes the connection.
pub struct Subscription(BufReader<UnixStream>);

pub fn subscribe(sock: &Path, topics: &[&str]) -> std::io::Result<Subscription> {
    let mut s = UnixStream::connect(sock)?;
    s.write_all(format!("subscribe {}\n", topics.join(" ")).trim_end().as_bytes())?;
    s.write_all(b"\n")?;
    let mut r = BufReader::new(s);
    let mut ack = String::new();
    r.read_line(&mut ack)?;
    if ack.trim() != "ok" {
        return Err(std::io::Error::other(format!("subscribe refused: {}", ack.trim())));
    }
    Ok(Subscription(r))
}

impl Iterator for Subscription {
    type Item = Event;
    fn next(&mut self) -> Option<Event> {
        loop {
            let mut line = String::new();
            match self.0.read_line(&mut line) {
                Ok(0) | Err(_) => return None,
                Ok(_) if line.trim().is_empty() => continue,
                Ok(_) => return Some(Event::parse(&line)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;

    fn fake(name: &str, serve: impl FnOnce(UnixStream) + Send + 'static) -> PathBuf {
        let p = std::env::temp_dir().join(format!("lui-fd-{name}-{}.sock", std::process::id()));
        let _ = std::fs::remove_file(&p);
        let l = UnixListener::bind(&p).unwrap();
        std::thread::spawn(move || serve(l.accept().unwrap().0));
        p
    }

    #[test]
    fn send_returns_the_reply_and_turns_error_lines_into_err() {
        let p = fake("send", |mut s| {
            let mut l = String::new();
            BufReader::new(&s).read_line(&mut l).unwrap();
            let r = if l.trim() == "state" { "ok layout=grid\n" } else { "error: unknown\n" };
            s.write_all(r.as_bytes()).unwrap();
        });
        assert_eq!(send(&p, "state").unwrap(), "ok layout=grid");
        let _ = std::fs::remove_file(&p);
        let p = fake("err", |mut s| {
            let mut l = String::new();
            BufReader::new(&s).read_line(&mut l).unwrap();
            s.write_all(b"error: denied by ctl_deny\n").unwrap();
        });
        assert!(send(&p, "x").unwrap_err().to_string().contains("denied"));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn subscription_reads_ack_then_events_until_close() {
        let p = fake("sub", |mut s| {
            let mut l = String::new();
            BufReader::new(&s).read_line(&mut l).unwrap();
            assert_eq!(l.trim(), "subscribe theme layout");
            s.write_all(b"ok\ntheme dracula\n\nlayout grid\n").unwrap();
        });
        let ev: Vec<Event> = subscribe(&p, &["theme", "layout"]).unwrap().collect();
        assert_eq!(ev, vec![Event { kind: "theme".into(), rest: "dracula".into() }, Event { kind: "layout".into(), rest: "grid".into() }]);
        let _ = std::fs::remove_file(&p);
        assert_eq!(Event::parse("minute"), Event { kind: "minute".into(), rest: String::new() });
    }

    #[test]
    fn refused_subscribe_is_an_error() {
        let p = fake("no", |mut s| {
            let mut l = String::new();
            BufReader::new(&s).read_line(&mut l).unwrap();
            s.write_all(b"error: nope\n").unwrap();
        });
        assert!(subscribe(&p, &[]).is_err());
        let _ = std::fs::remove_file(&p);
    }

    /// Needs a running finedesktop: `cargo test --lib fdctl -- --ignored`. `state` only reads.
    #[test]
    #[ignore]
    fn live_desktop_answers_state() {
        let sock = socket().expect("no finedesktop socket found");
        let r = send(&sock, "state").unwrap();
        assert!(!r.is_empty(), "empty reply");
        println!("state: {r}");
    }
}
