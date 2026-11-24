"""lazyui from Python through the C ABI (ctypes, no dependencies). See docs/BINDINGS.md and include/lazyui.h.

    ui = Lazyui("target/debug/liblazyui.so", "col\\n  text text=@hi\\n  buttonbar id=b items=go\\n", 30, 4)
    ui.catalog("key\\ten\\tpl\\nhi\\tHello\\tCzesc\\n"); ui.locale("pl")
    ui.act("b", "activate", "0"); print(ui.snapshot(), ui.events())

One instance per process: the library keeps a single UI. Not thread safe."""
import ctypes as C
import json


class LazyuiError(Exception):
    pass


class Lazyui:
    def __init__(self, lib, spec, cols=80, rows=24):
        self.lib = L = C.CDLL(lib)
        L.lz_out_ptr.restype = C.c_void_p
        L.lz_wake_ms.restype = C.c_double
        L.lz_tick.argtypes = [C.c_double]
        if not L.lz_init(*self._s(spec)):
            raise LazyuiError(self._error())
        self.resize(cols, rows)

    @staticmethod
    def _s(text):
        b = text.encode("utf-8")
        return b, len(b)

    def _out(self, n):
        return C.string_at(self.lib.lz_out_ptr(), n).decode("utf-8") if n else ""

    def _error(self):
        return self._out(self.lib.lz_error())

    def _ok(self, r):
        if not r:
            raise LazyuiError(self._error())

    def resize(self, cols, rows):
        self.lib.lz_resize(cols, rows)

    def catalog(self, tsv):
        self._ok(self.lib.lz_catalog(*self._s(tsv)))

    def locale(self, name):
        """False when no catalog is installed."""
        return bool(self.lib.lz_locale(*self._s(name)))

    def key(self, name):
        return bool(self.lib.lz_key(*self._s(name)))

    def text(self, s):
        self.lib.lz_text(*self._s(s))

    def act(self, id, action, arg=""):
        return bool(self.lib.lz_act(*self._s(id), *self._s(action), *self._s(arg)))

    def set(self, key, value):
        self.lib.lz_set(*self._s(key), *self._s(str(value)))

    def tick(self, seconds):
        self.lib.lz_tick(seconds * 1000.0)

    def snapshot(self):
        """The frame as plain text."""
        self.lib.lz_render()
        return self._out(self.lib.lz_snapshot())

    def tree(self):
        self.lib.lz_render()
        return json.loads(self._out(self.lib.lz_describe()))

    def events(self):
        """Events since the last call, as a list of dicts."""
        self.lib.lz_render()
        return [json.loads(l) for l in self._out(self.lib.lz_events()).splitlines() if l.strip()]
