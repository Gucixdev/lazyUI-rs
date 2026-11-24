import LazyUi
open LazyUi

/-- The same grid as examples/layout_grid.rs, answered by the exact-rational model. -/
def alphabet : Array (String × Size) :=
  #[("f0", .fixed 0), ("f3", .fixed 3), ("f7", .fixed 7),
    ("p0", .pct 0), ("p33", .pct 33), ("p50", .pct 50), ("p100", .pct 100),
    ("F1", .fill 1), ("F2", .fill 2), ("m2", .min 2), ("m6", .min 6), ("M2", .max 2), ("M6", .max 6)]

def main : IO Unit := do
  let out ← IO.getStdout
  let mut lists : Array (Array Nat) := #[]
  for a in [0:alphabet.size] do
    lists := lists.push #[a]
    for b in [0:alphabet.size] do
      lists := lists.push #[a, b]
      for c in [0:alphabet.size] do
        lists := lists.push #[a, b, c]
  for l in lists do
    let sizes := l.map (fun i => alphabet[i]!.2)
    let spec := " ".intercalate (l.toList.map (fun i => alphabet[i]!.1))
    for w in [0:41] do
      for gap in [0:3] do
        let r := split (w : Int) sizes (gap : Int)
        let cells := " ".intercalate (r.map fun (x, wd) => s!"{x}:{wd}")
        out.putStr s!"{w} {gap} {spec} -> {cells}\n"
