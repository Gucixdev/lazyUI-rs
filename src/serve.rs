//! `lazyui serve`: the Ui over HTTP for agents and scripts. Plain std, one request at a time, 127.0.0.1 only.
//!
//! Every request needs the token (`Authorization: Bearer TOKEN` or `X-Token: TOKEN`) and a `Host` of localhost, so a web
//! page in the browser can neither guess the port into use nor reach it through DNS rebinding.
//!
//! ```text
//! GET  /          this help
//! GET  /tree      the element tree as JSON (ids, kinds, rects, state)
//! GET  /describe  the tree the DOM skin gets, as JSON
//! GET  /snap      the settled frame as text
//! GET  /events    the event queue as JSON lines (and clears it)
//! POST /do        body = a script (the same commands as `lazyui script`: key, type, click, act, set, tick, expect...);
//!                 the answer is what the script prints; a failed line is a 400 with the reason
//! ```

use crate::headless::run_script;
use crate::ui::Ui;
use std::io::{BufRead, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

const MAX_LINE: usize = 4096;
const MAX_HEADERS: usize = 16 * 1024;
const MAX_BODY: usize = 1 << 20;

pub struct Request {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

pub struct Response {
    pub status: u16,
    pub ctype: &'static str,
    pub body: String,
}

impl Request {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

fn read_line(r: &mut impl BufRead, limit: usize) -> Result<String, (u16, String)> {
    let mut buf = Vec::new();
    let n = r.take(limit as u64 + 1).read_until(b'\n', &mut buf).map_err(|e| (400, format!("read: {e}")))?;
    if n == 0 {
        return Err((400, "empty request".into()));
    }
    if buf.len() > limit {
        return Err((431, "line too long".into()));
    }
    Ok(String::from_utf8_lossy(&buf).trim_end_matches(['\r', '\n']).to_string())
}

/// One HTTP/1.x request: the request line, the headers, and the body when `Content-Length` says there is one.
pub fn parse_request(r: &mut impl BufRead) -> Result<Request, (u16, String)> {
    let line = read_line(r, MAX_LINE)?;
    let mut parts = line.split(' ');
    let (method, path) = match (parts.next(), parts.next()) {
        (Some(m), Some(p)) if !m.is_empty() && p.starts_with('/') => (m.to_string(), p.split('?').next().unwrap_or(p).to_string()),
        _ => return Err((400, "bad request line".into())),
    };
    let mut headers = Vec::new();
    let mut total = 0;
    loop {
        let h = read_line(r, MAX_LINE)?;
        if h.is_empty() {
            break;
        }
        total += h.len();
        if total > MAX_HEADERS {
            return Err((431, "headers too large".into()));
        }
        if let Some((k, v)) = h.split_once(':') {
            headers.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    let mut req = Request { method, path, headers, body: String::new() };
    if let Some(len) = req.header("content-length") {
        let len: usize = len.parse().map_err(|_| (400, "bad Content-Length".to_string()))?;
        if len > MAX_BODY {
            return Err((413, "body too large".into()));
        }
        let mut b = vec![0u8; len];
        r.read_exact(&mut b).map_err(|e| (400, format!("body: {e}")))?;
        req.body = String::from_utf8(b).map_err(|_| (400, "body is not UTF-8".to_string()))?;
    }
    Ok(req)
}

fn same(a: &str, b: &str) -> bool {
    // same time for equal-length inputs, whatever the first difference
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |d, (x, y)| d | (x ^ y)) == 0
}

fn host_ok(host: &str) -> bool {
    let h = host.trim();
    let name = if let Some(rest) = h.strip_prefix('[') { rest.split(']').next().unwrap_or("") } else { h.split(':').next().unwrap_or("") };
    matches!(name, "127.0.0.1" | "localhost" | "::1")
}

fn text(status: u16, body: impl Into<String>) -> Response {
    Response { status, ctype: "text/plain; charset=utf-8", body: body.into() }
}

fn script(ui: &mut Ui, size: (i32, i32), body: &str, ctype: &'static str) -> Response {
    match run_script(ui, &format!("size {} {}\n{body}", size.0, size.1)) {
        Ok(out) => Response { status: 200, ctype, body: out },
        Err(e) => text(400, format!("{e}\n")),
    }
}

/// What to answer: pure, so it is tested without a socket.
pub fn handle(ui: &mut Ui, size: (i32, i32), token: &str, req: &Request) -> Response {
    if !req.header("host").is_some_and(host_ok) {
        return text(403, "Host must be localhost or 127.0.0.1\n");
    }
    let given = req
        .header("authorization")
        .and_then(|a| a.strip_prefix("Bearer "))
        .or_else(|| req.header("x-token"))
        .unwrap_or("");
    if token.is_empty() || !same(given.trim(), token) {
        return text(401, "token needed: Authorization: Bearer TOKEN\n");
    }
    const JSON: &str = "application/json";
    match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/") => text(200, concat!("lazyui serve\n", "GET /tree /describe /snap /events, POST /do (script in the body)\n")),
        ("GET", "/tree") => script(ui, size, "tree", JSON),
        ("GET", "/describe") => script(ui, size, "describe", JSON),
        ("GET", "/snap") => script(ui, size, "snap", "text/plain; charset=utf-8"),
        ("GET", "/events") => script(ui, size, "events", "application/x-ndjson"),
        ("POST", "/do") => script(ui, size, &req.body, "text/plain; charset=utf-8"),
        (_, "/" | "/tree" | "/describe" | "/snap" | "/events" | "/do") => text(405, "method not allowed\n"),
        _ => text(404, "not found\n"),
    }
}

pub fn write_response(w: &mut impl Write, r: &Response) -> std::io::Result<()> {
    let reason = match r.status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        413 => "Payload Too Large",
        431 => "Request Header Fields Too Large",
        _ => "Error",
    };
    write!(
        w,
        "HTTP/1.1 {} {reason}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{}",
        r.status,
        r.ctype,
        r.body.len(),
        r.body
    )?;
    w.flush()
}

fn one(ui: &mut Ui, size: (i32, i32), token: &str, stream: TcpStream) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
    let mut rd = std::io::BufReader::new(&stream);
    let res = match parse_request(&mut rd) {
        Ok(req) => handle(ui, size, token, &req),
        Err((code, why)) => text(code, format!("{why}\n")),
    };
    let mut w = &stream;
    let _ = write_response(&mut w, &res);
}

/// Answer requests one after the other; `max` stops after that many (tests), `None` runs until the process ends.
pub fn serve(ui: &mut Ui, listener: &TcpListener, size: (i32, i32), token: &str, max: Option<usize>) {
    let mut n = 0;
    for stream in listener.incoming() {
        if let Ok(s) = stream {
            one(ui, size, token, s);
        }
        n += 1;
        if max.is_some_and(|m| n >= m) {
            return;
        }
    }
}

/// 16 random bytes as hex from the system; None where there is no such source (pass a token yourself then).
pub fn random_token() -> Option<String> {
    let mut b = [0u8; 16];
    std::fs::File::open("/dev/urandom").ok()?.read_exact(&mut b).ok()?;
    Some(b.iter().map(|x| format!("{x:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn ui() -> Ui {
        Ui::from_spec("col\n  searchbar id=q bind=query\n  text id=t text=hello\n").unwrap()
    }
    fn req(method: &str, path: &str, headers: &[(&str, &str)], body: &str) -> Request {
        Request {
            method: method.into(),
            path: path.into(),
            headers: headers.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            body: body.into(),
        }
    }
    const OK: &[(&str, &str)] = &[("Host", "127.0.0.1:7878"), ("Authorization", "Bearer s3cret")];

    #[test]
    fn parses_a_post_with_a_body_and_ignores_the_query() {
        let raw = "POST /do?x=1 HTTP/1.1\r\nHost: localhost\r\nContent-Length: 8\r\n\r\ntype abc";
        let r = parse_request(&mut Cursor::new(raw)).unwrap();
        assert_eq!((r.method.as_str(), r.path.as_str(), r.body.as_str()), ("POST", "/do", "type abc"));
        assert_eq!(r.header("HOST"), Some("localhost"));
    }

    #[test]
    fn refuses_bad_and_oversized_requests() {
        assert_eq!(parse_request(&mut Cursor::new("nonsense\r\n\r\n")).err().unwrap().0, 400);
        assert_eq!(parse_request(&mut Cursor::new("POST /do HTTP/1.1\r\nContent-Length: 99999999\r\n\r\n")).err().unwrap().0, 413);
        let long = format!("GET /{} HTTP/1.1\r\n\r\n", "a".repeat(5000));
        assert_eq!(parse_request(&mut Cursor::new(long)).err().unwrap().0, 431);
        assert_eq!(parse_request(&mut Cursor::new("")).err().unwrap().0, 400);
    }

    #[test]
    fn token_and_host_are_checked_first() {
        let mut u = ui();
        assert_eq!(handle(&mut u, (30, 5), "s3cret", &req("GET", "/tree", &[("Host", "127.0.0.1")], "")).status, 401);
        assert_eq!(handle(&mut u, (30, 5), "s3cret", &req("GET", "/tree", &[("Host", "127.0.0.1"), ("X-Token", "nope")], "")).status, 401);
        assert_eq!(handle(&mut u, (30, 5), "s3cret", &req("GET", "/tree", &[("Host", "evil.example"), ("X-Token", "s3cret")], "")).status, 403);
        assert_eq!(handle(&mut u, (30, 5), "s3cret", &req("GET", "/tree", &[("X-Token", "s3cret")], "")).status, 403);
        assert_eq!(handle(&mut u, (30, 5), "", &req("GET", "/tree", &[("Host", "localhost"), ("X-Token", "")], "")).status, 401);
        assert_eq!(handle(&mut u, (30, 5), "s3cret", &req("GET", "/tree", &[("Host", "[::1]:80"), ("X-Token", "s3cret")], "")).status, 200);
    }

    #[test]
    fn tree_snap_and_a_script_over_http() {
        let mut u = ui();
        let t = handle(&mut u, (30, 5), "s3cret", &req("GET", "/tree", OK, ""));
        assert_eq!((t.status, t.ctype), (200, "application/json"));
        assert!(t.body.contains("\"q\"") && t.body.contains("hello"), "{}", t.body);
        let d = handle(&mut u, (30, 5), "s3cret", &req("POST", "/do", OK, "focus q\ntype ab\nexpect model query ab\nevents"));
        assert_eq!(d.status, 200, "{}", d.body);
        assert!(d.body.contains("\"change\""), "{}", d.body);
        let s = handle(&mut u, (30, 5), "s3cret", &req("GET", "/snap", OK, ""));
        assert!(s.body.contains("> ab"), "{}", s.body);
    }

    #[test]
    fn a_failed_line_is_a_400_and_unknown_things_are_404_405() {
        let mut u = ui();
        let r = handle(&mut u, (30, 5), "s3cret", &req("POST", "/do", OK, "expect text \"not there\""));
        assert_eq!(r.status, 400);
        assert!(r.body.contains("line 2"), "{}", r.body); // line 1 is the size line the server adds
        assert_eq!(handle(&mut u, (30, 5), "s3cret", &req("GET", "/nope", OK, "")).status, 404);
        assert_eq!(handle(&mut u, (30, 5), "s3cret", &req("POST", "/tree", OK, "")).status, 405);
    }

    #[test]
    fn a_real_socket_round_trip() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let client = std::thread::spawn(move || {
            let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
            write!(s, "GET /snap HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nX-Token: tok\r\n\r\n").unwrap();
            let mut out = String::new();
            s.read_to_string(&mut out).unwrap();
            out
        });
        let mut u = ui();
        serve(&mut u, &l, (30, 5), "tok", Some(1));
        let out = client.join().unwrap();
        assert!(out.starts_with("HTTP/1.1 200 OK"), "{out}");
        assert!(out.contains("hello"), "{out}");
    }
}
