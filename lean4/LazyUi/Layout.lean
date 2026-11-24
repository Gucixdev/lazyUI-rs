/-!
Model of `src/layout.rs` (`split`) in exact rationals, and the theorems about its last step: the rounding of edges
that makes neighbours touch and the widths add up. The solver (`solve`) is only modelled here, its properties are
checked by the oracle (`lean4/run`) against the Rust code over a grid of inputs and against the optimum from `ampl/`.
No Mathlib: core Lean only.
-/
namespace LazyUi

inductive Size where
  | fixed (n : Int) | pct (p : Int) | fill (w : Nat) | min (n : Int) | max (n : Int)
  deriving Repr, Inhabited

def Size.weight : Size → Rat
  | .fill w => (w : Rat)
  | _ => 1

def Size.flex : Size → Bool
  | .fill _ | .min _ | .max _ => true
  | _ => false

def rmin (a b : Rat) : Rat := if a ≤ b then a else b
def rmax (a b : Rat) : Rat := if a ≤ b then b else a

/-- Lengths along the main axis, before rounding: the same steps as `solve` in layout.rs. -/
def solve (total : Int) (sizes : Array Size) : Array Rat := Id.run do
  let total : Rat := ((max total 0 : Int) : Rat)
  let n := sizes.size
  let mut out : Array Rat := Array.replicate n 0
  let mut left := total
  for i in [0:n] do
    let want? : Option Rat := match sizes[i]! with
      | .fixed k => some (((max k 0 : Int)) : Rat)
      | .pct p => some (((Rat.floor (total * (((max 0 (min p 100)) : Int) : Rat) / 100) : Int)) : Rat)
      | _ => none
    match want? with
    | some w =>
      out := out.set! i (rmin w left)
      left := left - out[i]!
    | none => pure ()
  let flex := (List.range n).filter (fun i => sizes[i]!.flex)
  let mut frozen : Array Bool := Array.replicate n false
  for _ in [0:flex.length + 1] do
    let live := flex.filter (fun i => !frozen[i]!)
    let wsum : Rat := live.foldl (fun a i => a + sizes[i]!.weight) 0
    if wsum ≤ 0 then break
    let share (i : Nat) : Rat := rmax left 0 * sizes[i]!.weight / wsum
    let violator := flex.find? fun i =>
      !frozen[i]! &&
        match sizes[i]! with
        | .min k => share i < (((max k 0 : Int)) : Rat)
        | .max k => share i > (((max k 0 : Int)) : Rat)
        | _ => false
    match violator with
    | some i =>
      let bound : Rat := match sizes[i]! with
        | .min k | .max k => (((max k 0 : Int)) : Rat)
        | _ => 0
      let g := rmin bound (rmax left 0)
      out := out.set! i g
      left := left - g
      frozen := frozen.set! i true
    | none =>
      for i in flex do
        if !frozen[i]! then out := out.set! i (rmax left 0 * sizes[i]!.weight / wsum)
      break
  return out

/-- f64 `round` of a non-negative number: half goes up. -/
def rnd (q : Rat) : Int := Rat.floor (q + 1 / 2)

/-- Rectangles `(x, w)` along one axis starting at 0, as `split` makes them. -/
def split (main : Int) (sizes : Array Size) (gap : Int) : List (Int × Int) :=
  let n := sizes.size
  if n == 0 then [] else
  let gap := max gap 0
  let gaps := min (gap * ((n : Int) - 1)) (max main 0)
  let lens := solve (main - gaps) sizes
  let gapEff : Rat := if n > 1 then (gaps : Rat) / ((n : Rat) - 1) else 0
  let m := max main 0
  let rec go (i : Nat) (acc : Rat) (fuel : Nat) : List (Int × Int) :=
    match fuel with
    | 0 => []
    | fuel + 1 =>
      if i < n then
        let l := lens[i]!
        let start := acc + gapEff * (i : Rat)
        let s := min (rnd start) m
        let e := min (rnd (start + l)) m
        (s, max (e - s) 0) :: go (i + 1) (acc + l) fuel
      else []
  go 0 0 n

/-! ### What is proved: the rounding step.

Positions are `p / d` with `d > 0`, rounded as `(2p + d) / (2d)` (floor division), which is `rnd` of `p/d`.
-/

/-- `rnd (p / d)` on integers. -/
def rndI (p d : Int) : Int := (2 * p + d) / (2 * d)

theorem rndI_mono {p q d : Int} (hd : 0 < d) (h : p ≤ q) : rndI p d ≤ rndI q d := by
  unfold rndI
  exact Int.ediv_le_ediv (by omega) (by omega)

theorem rndI_nonneg {p d : Int} (hd : 0 < d) (hp : 0 ≤ p) : 0 ≤ rndI p d := by
  unfold rndI
  exact Int.ediv_nonneg (by omega) (by omega)

/-- Rounding never moves a position by more than one half: `|rnd(x) - x| ≤ 1/2`, in integers: `2d·r ≤ 2p + d < 2d·r + 2d`. -/
theorem rndI_error {p d : Int} (hd : 0 < d) :
    2 * d * rndI p d ≤ 2 * p + d ∧ 2 * p + d < 2 * d * rndI p d + 2 * d := by
  unfold rndI
  have h2 : 0 < 2 * d := by omega
  have h := Int.mul_ediv_add_emod (2 * p + d) (2 * d)
  have h0 := Int.emod_nonneg (2 * p + d) (show 2 * d ≠ 0 by omega)
  have h1 := Int.emod_lt_of_pos (2 * p + d) h2
  generalize 2 * d * ((2 * p + d) / (2 * d)) = k at h ⊢
  generalize (2 * p + d) % (2 * d) = r at *
  omega

/-- A rectangle between two rounded edges, clamped to `[0, m]` like the Rust code: never negative, never wider than `m`. -/
def rect (m s e : Int) : Int × Int :=
  let s' := min s m
  let e' := min e m
  (s', max (e' - s') 0)

theorem rect_width_nonneg (m s e : Int) : 0 ≤ (rect m s e).2 := by
  unfold rect; simp; omega

theorem rect_width_le (m s e : Int) (hm : 0 ≤ m) (hs : 0 ≤ s) : (rect m s e).2 ≤ m := by
  unfold rect; simp; omega

theorem rect_inside (m s e : Int) (hm : 0 ≤ m) (hs : 0 ≤ s) :
    0 ≤ (rect m s e).1 ∧ (rect m s e).1 + (rect m s e).2 ≤ m := by
  unfold rect; simp; omega

/-- Touching: when the end position of one rectangle is the start position of the next (gap 0), the rectangles meet
exactly and their widths add up to the distance between the outer edges, whatever the rounding does. -/
theorem rect_touch (m a b c : Int) (hab : a ≤ b) (hbc : b ≤ c) (_hm : 0 ≤ m) :
    (rect m a b).1 + (rect m a b).2 = (rect m b c).1 ∧
    (rect m a b).2 + (rect m b c).2 = min c m - min a m := by
  unfold rect; simp; omega

end LazyUi
