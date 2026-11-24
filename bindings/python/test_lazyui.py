"""Run: python3 bindings/python/test_lazyui.py  (build first: cargo build -j2 --no-default-features --features web)"""
import os, sys, unittest

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.join(HERE, "..", "..")
sys.path.insert(0, HERE)
from lazyui import Lazyui, LazyuiError

LIB = os.environ.get("LAZYUI_LIB") or next(
    (p for p in (os.path.join(ROOT, "target", d, "liblazyui.so") for d in ("debug", "release")) if os.path.exists(p)), None)
SPEC = "col\n  text text=@hi\n  buttonbar id=b items=\"@go,other\"\n"
TSV = "key\ten\tpl\nhi\tHello\tCzesc\ngo\tGo\tIdz\n"


@unittest.skipUnless(LIB, "liblazyui.so not built")
class T(unittest.TestCase):
    def test_snapshot_act_events_and_translation(self):
        ui = Lazyui(LIB, SPEC, 30, 4)
        self.assertIn("@hi", ui.snapshot())
        ui.catalog(TSV)
        self.assertTrue(ui.locale("pl"))
        s = ui.snapshot()
        self.assertIn("Czesc", s)
        self.assertIn("Idz", s)
        self.assertTrue(ui.act("b", "activate", "1"))
        ev = ui.events()
        self.assertEqual((ev[0]["id"], ev[0]["kind"], ev[0]["index"]), ("b", "activate", 1))
        self.assertEqual(ui.tree()["kind"], "col")

    def test_bad_spec_and_bad_catalog_raise(self):
        with self.assertRaises(LazyuiError):
            Lazyui(LIB, "col\n  nosuchkind\n   bad indent\n", 10, 3)
        ui = Lazyui(LIB, SPEC, 30, 4)
        with self.assertRaises(LazyuiError):
            ui.catalog("this is not a catalog\n")


if __name__ == "__main__":
    unittest.main()
