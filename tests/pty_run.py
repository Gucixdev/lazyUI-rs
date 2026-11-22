#!/usr/bin/env python3
"""Drive `lazyui run` in a real pseudo-terminal: UI on the pty, events on stdout, model on stdin."""
import os, pty, sys, time, select, subprocess, fcntl, termios, struct, json, signal


import re
def emulate(data, w=40, h=12):
    """Just enough of a terminal: printable text, \\r, CSI n G/A/B/H/J, everything else ignored."""
    g = [[" "] * w for _ in range(h)]; x = y = 0
    text = data.decode("utf-8", "replace"); i = 0
    while i < len(text):
        c = text[i]
        if c == "\x1b" and text[i+1:i+2] == "[":
            m = re.compile(r"[\x30-\x3f]*[\x20-\x2f]*[\x40-\x7e]").match(text, i + 2)
            par, fin = m.group()[:-1], m.group()[-1]; i = m.end()
            n = int(par) if par.isdigit() else 1
            if fin == "G": x = n - 1
            elif fin == "A": y = max(0, y - n)
            elif fin == "B": y = min(h - 1, y + n)
            elif fin == "H": x = y = 0
            elif fin == "J": g = [[" "] * w for _ in range(h)]
            continue
        if c == "\r": x = 0
        elif c == "\n": y = min(h - 1, y + 1)
        elif c >= " " and x < w:
            g[y][x] = c; x += 1
        i += 1
    return ["".join(r).rstrip() for r in g]

BIN = sys.argv[1]; SPEC = sys.argv[2]
m, s = pty.openpty()
fcntl.ioctl(s, termios.TIOCSWINSZ, struct.pack("HHHH", 12, 40, 0, 0))
def pre():
    os.setsid(); fcntl.ioctl(s, termios.TIOCSCTTY, 0)
p = subprocess.Popen([BIN, "run", SPEC, "--hotkey", "f5=refresh", "--every", "0.2=beat"], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                     stderr=subprocess.PIPE, preexec_fn=pre, close_fds=True)
screen = b""
def pump(t=0.4):
    global screen
    end = time.time() + t
    while time.time() < end:
        r, _, _ = select.select([m], [], [], 0.05)
        if r:
            try: screen += os.read(m, 65536)
            except OSError: return
pump(0.6)
assert b"\x1b[?1049h" in screen, ("alt screen not entered", screen[:200], p.poll(), p.stderr.read() if p.poll() is not None else "")
assert b"\x1b[?2004h" in screen
os.write(m, b"\t"); pump()             # focus the search bar
os.write(m, b"et"); pump()
os.write(m, b"\x1b[15~"); pump()       # f5
p.stdin.write(b"cpu=80\n"); p.stdin.flush(); pump(0.5)
os.write(m, b"\x11"); pump(0.4)        # ctrl-q
code = p.wait(timeout=3)
out = p.stdout.read().decode()
evs = [json.loads(l) for l in out.splitlines()]
kinds = [(e["id"], e["kind"]) for e in evs]
print("exit", code, "events", len(evs))
assert code == 0, p.stderr.read()
assert ("q", "change") in kinds and ("app", "refresh") in kinds and ("app", "beat") in kinds and ("app", "quit") in kinds, kinds
assert [e["text"] for e in evs if e["id"] == "q"][-1] == "et"
assert b"\x1b[?1049l" in screen and b"\x1b[?25h" in screen, "terminal not restored"
rows = emulate(screen[screen.index(b"\x1b[2J"):screen.rindex(b"\x1b[?1049l")])
assert any("80%" in r for r in rows), ("model from stdin did not reach the screen", rows)
assert rows[0].startswith(" main  extras"), rows
print("ok")
