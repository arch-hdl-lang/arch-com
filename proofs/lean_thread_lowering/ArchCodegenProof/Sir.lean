/-!
# Sized IR shared by the ARCH-Core and SystemVerilog-subset semantics

Both source languages of the codegen validator lower into this small
expression language. Every operator carries its result width explicitly, so
there is no context-dependent sizing left to get wrong: the IEEE 1800 §11.6
context rules live in `Sv.lean`, the ARCH typing rules in `Core.lean`, and
each is proved to lower faithfully into `Sir`.

The file also contains the normalizer `Sir.norm` and its soundness theorem
`Sir.eval_norm`. The validator compares *normalized* lowerings
syntactically, so `eval_norm` is what turns "the two trees are equal" into
"the two programs compute the same function".

Values are natural numbers. A signal or operator of width `w` always denotes
a value below `2 ^ w`; `mask w v = v % 2 ^ w` is the only truncation.
-/

namespace Arch.CodegenProof

abbrev Sig := Nat
abbrev Env := Sig → Nat

def mask (w v : Nat) : Nat := v % 2 ^ w

theorem mask_lt (w v : Nat) : mask w v < 2 ^ w :=
  Nat.mod_lt _ (Nat.two_pow_pos w)

theorem mask_mask_of_le {w w' : Nat} (h : w ≤ w') (v : Nat) :
    mask w (mask w' v) = mask w v :=
  Nat.mod_mod_of_dvd v (Nat.pow_dvd_pow 2 h)

theorem mask_mask (w v : Nat) : mask w (mask w v) = mask w v :=
  Nat.mod_mod _ _

theorem mask_of_lt {w v : Nat} (h : v < 2 ^ w) : mask w v = v :=
  Nat.mod_eq_of_lt h

theorem mask_mask_of_ge {w w' : Nat} (h : w' ≤ w) (v : Nat) :
    mask w (mask w' v) = mask w' v :=
  mask_of_lt (Nat.lt_of_lt_of_le (mask_lt w' v) (Nat.pow_le_pow_right (by decide) h))

/-! ## Fixed-width operators -/

/-- Modular subtraction at width `w`. `y * (2^w - 1)` is `-y` modulo `2^w`;
this form avoids truncated natural subtraction. `subW_add_cancel` and
`subW_of_le` below pin down that it is ordinary wrap-around subtraction. -/
def subW (w x y : Nat) : Nat := mask w (x + y * (2 ^ w - 1))

/-- Bitwise complement at width `w`. -/
def notW (w x : Nat) : Nat := 2 ^ w - 1 - mask w x

theorem subW_of_le {w x y : Nat} (h : y ≤ x) : subW w x y = mask w (x - y) := by
  unfold subW mask
  have hp : 1 ≤ 2 ^ w := Nat.two_pow_pos w
  have hm : y ≤ y * 2 ^ w := Nat.le_mul_of_pos_right y hp
  have : x + y * (2 ^ w - 1) = (x - y) + 2 ^ w * y := by
    rw [Nat.mul_sub_one, Nat.mul_comm (2 ^ w) y]; omega
  rw [this, Nat.add_mul_mod_self_left]

theorem subW_add_cancel (w x y : Nat) : mask w (subW w x y + y) = mask w x := by
  unfold subW mask
  have hp : 1 ≤ 2 ^ w := Nat.two_pow_pos w
  have hm : y ≤ y * 2 ^ w := Nat.le_mul_of_pos_right y hp
  rw [Nat.mod_add_mod]
  have : x + y * (2 ^ w - 1) + y = x + 2 ^ w * y := by
    rw [Nat.mul_sub_one, Nat.mul_comm (2 ^ w) y]; omega
  rw [this, Nat.add_mul_mod_self_left]

inductive BinOp where
  | add | sub | mul | and | or | xor
deriving DecidableEq, Repr

def BinOp.apply (w : Nat) : BinOp → Nat → Nat → Nat
  | .add, x, y => mask w (x + y)
  | .sub, x, y => subW w x y
  | .mul, x, y => mask w (x * y)
  | .and, x, y => mask w (x &&& y)
  | .or,  x, y => mask w (x ||| y)
  | .xor, x, y => mask w (x ^^^ y)

inductive CmpOp where
  | eq | ne | lt | le | gt | ge
deriving DecidableEq, Repr

def CmpOp.apply : CmpOp → Nat → Nat → Bool
  | .eq, x, y => x == y
  | .ne, x, y => x != y
  | .lt, x, y => decide (x < y)
  | .le, x, y => decide (x ≤ y)
  | .gt, x, y => decide (x > y)
  | .ge, x, y => decide (x ≥ y)

def b2n (b : Bool) : Nat := if b then 1 else 0

/-- Two's-complement reading of the low `w` bits of `v`. -/
def toInt (w v : Nat) : Int := if 0 < w ∧ 2 ^ (w - 1) ≤ v then (v : Int) - 2 ^ w else v

def cmpInt : CmpOp → Int → Int → Bool
  | .eq, x, y => x == y
  | .ne, x, y => x != y
  | .lt, x, y => decide (x < y)
  | .le, x, y => decide (x ≤ y)
  | .gt, x, y => decide (x > y)
  | .ge, x, y => decide (x ≥ y)

theorem b2n_lt (b : Bool) : b2n b < 2 ^ 1 := by cases b <;> decide

/-- Sign-extend (or truncate) the low `src` bits of `v` to width `w`. -/
def sextVal (w src v : Nat) : Nat :=
  let v' := mask src v
  if 0 < src ∧ 2 ^ (src - 1) ≤ v' then mask w (v' + (2 ^ (max w src) - 2 ^ src))
  else mask w v'

/-- Smallest `k` with `n < 2 ^ k`. Structural (fuel = `n`) so the kernel can
evaluate it during certificate replay. -/
def bitlen (n : Nat) : Nat := go n n
where
  go : Nat → Nat → Nat
    | 0, _ => 0
    | f + 1, n => if n = 0 then 0 else go f (n / 2) + 1

theorem bitlen_go_lt : ∀ (f n : Nat), n ≤ f → n < 2 ^ bitlen.go f n
  | 0, n, h => by simp [bitlen.go]; omega
  | f + 1, n, h => by
    unfold bitlen.go
    split
    · simp; omega
    · have ih := bitlen_go_lt f (n / 2) (by omega)
      rw [Nat.pow_succ]; omega

theorem lt_two_pow_bitlen (n : Nat) : n < 2 ^ bitlen n :=
  bitlen_go_lt n n (Nat.le_refl n)

/-! ## Expressions -/

inductive Sir where
  | var (x : Sig) (w : Nat)
  | lit (n : Nat)
  | resize (w : Nat) (e : Sir)
  | sext (w src : Nat) (e : Sir)
  | bin (op : BinOp) (w : Nat) (a b : Sir)
  | bnot (w : Nat) (a : Sir)
  | cmp (op : CmpOp) (a b : Sir)
  /-- Signed comparison of two `w`-bit operands. -/
  | scmp (op : CmpOp) (w : Nat) (a b : Sir)
  | mux (w : Nat) (c a b : Sir)
  | lnot (a : Sir)
  | land (a b : Sir)
  | lor (a b : Sir)
  | slice (hi lo : Nat) (a : Sir)
deriving DecidableEq, Repr

namespace Sir

def eval (σ : Env) : Sir → Nat
  | .var x w => mask w (σ x)
  | .lit n => n
  | .resize w e => mask w (e.eval σ)
  | .sext w s e => sextVal w s (e.eval σ)
  | .bin op w a b => op.apply w (a.eval σ) (b.eval σ)
  | .bnot w a => notW w (a.eval σ)
  | .cmp op a b => b2n (op.apply (a.eval σ) (b.eval σ))
  | .scmp op w a b => b2n (cmpInt op (toInt w (a.eval σ)) (toInt w (b.eval σ)))
  | .mux w c a b => if c.eval σ ≠ 0 then mask w (a.eval σ) else mask w (b.eval σ)
  | .lnot a => b2n (a.eval σ == 0)
  | .land a b => b2n (a.eval σ != 0 && b.eval σ != 0)
  | .lor a b => b2n (a.eval σ != 0 || b.eval σ != 0)
  | .slice hi lo a => mask (hi + 1 - lo) (a.eval σ / 2 ^ lo)

def width : Sir → Nat
  | .var _ w => w
  | .lit n => bitlen n
  | .resize w _ => w
  | .sext w _ _ => w
  | .bin _ w _ _ => w
  | .bnot w _ => w
  | .cmp .. | .scmp .. => 1
  | .mux w .. => w
  | .lnot _ => 1
  | .land .. => 1
  | .lor .. => 1
  | .slice hi lo _ => hi + 1 - lo

theorem binOp_lt (op : BinOp) (w x y : Nat) : op.apply w x y < 2 ^ w := by
  cases op <;> simp only [BinOp.apply, subW] <;> exact mask_lt _ _

theorem notW_lt (w x : Nat) : notW w x < 2 ^ w := by
  have := Nat.two_pow_pos w; unfold notW; omega

theorem sextVal_lt (w s v : Nat) : sextVal w s v < 2 ^ w := by
  simp only [sextVal]; split <;> exact mask_lt _ _

theorem eval_lt (σ : Env) : ∀ e : Sir, e.eval σ < 2 ^ e.width
  | .var .. | .resize .. | .slice .. => mask_lt _ _
  | .lit n => lt_two_pow_bitlen n
  | .sext .. => sextVal_lt _ _ _
  | .bin .. => binOp_lt _ _ _ _
  | .bnot .. => notW_lt _ _
  | .cmp .. | .scmp .. | .lnot _ | .land .. | .lor .. => b2n_lt _
  | .mux w c a b => by
    simp only [eval, width]; split <;> exact mask_lt _ _

/-! ## Low-bit lemmas

Every fixed-width operator only observes its operands modulo `2^w`, and
truncating its result to a narrower width is the same as computing at that
narrower width in the first place. These two facts justify `narrow` and the
push-down rules of `simpResize`. -/

/-- `2^w' - 1 = (2^w - 1) + 2^w * j` for some `j` when `w ≤ w'`. -/
theorem pow_sub_one_split {w w' : Nat} (h : w ≤ w') :
    ∃ j, 2 ^ w' - 1 = (2 ^ w - 1) + 2 ^ w * j := by
  obtain ⟨d, rfl⟩ := Nat.exists_eq_add_of_le h
  refine ⟨2 ^ d - 1, ?_⟩
  have hp : 1 ≤ 2 ^ w := Nat.two_pow_pos w
  have hq : 1 ≤ 2 ^ d := Nat.two_pow_pos d
  rw [Nat.pow_add, Nat.mul_sub_one]
  have : 2 ^ w ≤ 2 ^ w * 2 ^ d := Nat.le_mul_of_pos_right _ hq
  omega

/-- Key congruence: replacing `2^w' - 1` by `2^w - 1` as the "minus one"
multiplier does not change the residue modulo `2^w`. -/
theorem mask_neg_mul {w w' : Nat} (h : w ≤ w') (u z : Nat) :
    mask w (u + z * (2 ^ w' - 1)) = mask w (u + z * (2 ^ w - 1)) := by
  obtain ⟨j, hj⟩ := pow_sub_one_split h
  unfold mask
  rw [hj, Nat.mul_add, ← Nat.add_assoc, Nat.mul_left_comm, Nat.add_mul_mod_self_left]

theorem notW_eq_neg (w x : Nat) : notW w x = mask w ((x + 1) * (2 ^ w - 1)) := by
  unfold notW
  have hp : 1 ≤ 2 ^ w := Nat.two_pow_pos w
  have hr : mask w x < 2 ^ w := mask_lt w x
  have hmod : mask w ((x + 1) * (2 ^ w - 1)) = mask w ((mask w x + 1) * (2 ^ w - 1)) := by
    unfold mask
    rw [Nat.mul_mod, Nat.mul_mod (x % 2 ^ w + 1)]
    congr 2
    exact (Nat.mod_add_mod x _ 1).symm
  rw [hmod]
  generalize mask w x = r at hr ⊢
  have : (r + 1) * (2 ^ w - 1) = (2 ^ w - 1 - r) + 2 ^ w * r := by
    rw [Nat.add_mul, Nat.one_mul, Nat.mul_sub_one, Nat.mul_comm r (2 ^ w)]
    have : r ≤ 2 ^ w * r := Nat.le_mul_of_pos_left r hp
    omega
  unfold mask
  rw [this, Nat.add_mul_mod_self_left, Nat.mod_eq_of_lt (by omega)]

theorem binOp_low (op : BinOp) (w x y : Nat) :
    op.apply w x y = op.apply w (mask w x) (mask w y) := by
  cases op <;> simp only [BinOp.apply, subW, mask]
  · rw [Nat.add_mod (x % _), Nat.mod_mod, Nat.mod_mod, ← Nat.add_mod]
  · rw [Nat.add_mod (x % _), Nat.mod_mod, Nat.mul_mod (y % _), Nat.mod_mod,
      ← Nat.mul_mod, ← Nat.add_mod]
  · rw [Nat.mul_mod (x % _), Nat.mod_mod, Nat.mod_mod, ← Nat.mul_mod]
  · rw [Nat.and_mod_two_pow, Nat.and_mod_two_pow, Nat.mod_mod, Nat.mod_mod]
  · rw [Nat.or_mod_two_pow, Nat.or_mod_two_pow, Nat.mod_mod, Nat.mod_mod]
  · rw [Nat.xor_mod_two_pow, Nat.xor_mod_two_pow, Nat.mod_mod, Nat.mod_mod]

theorem binOp_push (op : BinOp) {w w' : Nat} (h : w ≤ w') (x y : Nat) :
    mask w (op.apply w' x y) = op.apply w x y := by
  cases op <;> simp only [BinOp.apply, subW] <;> rw [mask_mask_of_le h]
  exact mask_neg_mul h x y

theorem notW_low (w x : Nat) : notW w x = notW w (mask w x) := by
  simp [notW, mask_mask]

theorem notW_push {w w' : Nat} (h : w ≤ w') (x : Nat) :
    mask w (notW w' x) = notW w x := by
  rw [notW_eq_neg, notW_eq_neg, mask_mask_of_le h]
  have := mask_neg_mul h 0 (x + 1)
  simpa using this

/-! ## Normalizer -/

/-- Simplify an operand that is consumed modulo `2^w`. -/
def narrow (w : Nat) : Sir → Sir
  | .lit n => .lit (mask w n)
  | .var x w' => if w ≤ w' then .var x w else .var x w'
  | .resize w' e => if w ≤ w' then narrow w e else .resize w' e
  | .bin op w' a b => if w ≤ w' then .bin op w (narrow w a) (narrow w b) else .bin op w' a b
  | .bnot w' a => if w ≤ w' then .bnot w (narrow w a) else .bnot w' a
  | .mux w' c a b => if w ≤ w' then .mux w c (narrow w a) (narrow w b) else .mux w' c a b
  | e => e

/-- Natural result width of a low-bit operator on operands of the given
widths: no operand values can produce a set bit at or above it. `-` wraps,
so it has none. -/
def natWidth : BinOp → Nat → Nat → Option Nat
  | .add, wa, wb => some (max wa wb + 1)
  | .mul, wa, wb => some (wa + wb)
  | .and, wa, wb | .or, wa, wb | .xor, wa, wb => some (max wa wb)
  | .sub, _, _ => none

/-- Canonical width for a binary node: the smaller of its declared width and
its natural width. -/
def shrinkBin (op : BinOp) (w : Nat) (a b : Sir) : Sir :=
  match natWidth op a.width b.width with
  | some n => if n ≤ w then .bin op n a b else .bin op w a b
  | none => .bin op w a b

def shrinkMux (w : Nat) (c a b : Sir) : Sir :=
  if max a.width b.width ≤ w then .mux (max a.width b.width) c a b else .mux w c a b

/-- Simplify `resize w e`, assuming `e` is already normalized. -/
def simpResize (w : Nat) : Sir → Sir
  | .lit n => .lit (mask w n)
  | .var x w' => if w ≤ w' then .var x w else .var x w'
  | .resize w' e => if w ≤ w' then simpResize w e else .resize w' e
  | .bin op w' a b => if w ≤ w' then .bin op w (narrow w a) (narrow w b) else .bin op w' a b
  | .bnot w' a => if w ≤ w' then .bnot w (narrow w a) else .bnot w' a
  | .mux w' c a b => if w ≤ w' then .mux w c (narrow w a) (narrow w b) else .mux w' c a b
  | e => if e.width ≤ w then e else .resize w e

/-- Comparisons of two literals fold to their 0/1 result. -/
def foldCmp (op : CmpOp) : Sir → Sir → Sir
  | .lit n, .lit m => .lit (b2n (op.apply n m))
  | a, b => .cmp op a b

def foldSCmp (op : CmpOp) (w : Nat) : Sir → Sir → Sir
  | .lit n, .lit m => .lit (b2n (cmpInt op (toInt w n) (toInt w m)))
  | a, b => .scmp op w a b

/-- Simplify `sext w s e`, assuming `e` is already normalized. -/
def simpSext (w s : Nat) : Sir → Sir
  | .lit n => .lit (sextVal w s n)
  | e => if e.width < s then simpResize w e else .sext w s e

def norm : Sir → Sir
  | .var x w => .var x w
  | .lit n => .lit n
  | .resize w e => simpResize w (norm e)
  | .sext w s e => simpSext w s (norm e)
  | .bin op w a b => shrinkBin op w (narrow w (norm a)) (narrow w (norm b))
  | .bnot w a => .bnot w (narrow w (norm a))
  | .cmp op a b => foldCmp op (norm a) (norm b)
  | .scmp op w a b => foldSCmp op w (norm a) (norm b)
  | .mux w c a b => shrinkMux w (norm c) (narrow w (norm a)) (narrow w (norm b))
  | .lnot a => .lnot (norm a)
  | .land a b => .land (norm a) (norm b)
  | .lor a b => .lor (norm a) (norm b)
  | .slice hi lo a => .slice hi lo (norm a)

theorem mask_narrow (σ : Env) (w : Nat) : ∀ e : Sir,
    mask w ((narrow w e).eval σ) = mask w (e.eval σ)
  | .lit n => by simp [narrow, eval, mask_mask]
  | .var x w' => by
    simp only [narrow]; split
    · simp only [eval]; rw [mask_mask, mask_mask_of_le (by omega)]
    · rfl
  | .resize w' e => by
    simp only [narrow]
    split
    · rw [mask_narrow σ w e]; simp only [eval]; rw [mask_mask_of_le (by omega)]
    · rfl
  | .bin op w' a b => by
    simp only [narrow]; split
    · simp only [eval]
      rw [mask_of_lt (binOp_lt _ _ _ _), binOp_push op (by omega), binOp_low op w,
        mask_narrow σ w a, mask_narrow σ w b, ← binOp_low]
    · rfl
  | .bnot w' a => by
    simp only [narrow]; split
    · simp only [eval]
      rw [mask_of_lt (notW_lt _ _), notW_push (by omega), notW_low w, mask_narrow σ w a,
        ← notW_low]
    · rfl
  | .mux w' c a b => by
    simp only [narrow]; split
    · simp only [eval]
      split
      · rw [mask_mask, mask_narrow σ w a, mask_mask_of_le (by omega)]
      · rw [mask_mask, mask_narrow σ w b, mask_mask_of_le (by omega)]
    · rfl
  | .sext .. | .cmp .. | .scmp .. | .lnot _ | .land .. | .lor .. | .slice .. => by simp [narrow]

theorem eval_mask_of_width_le (σ : Env) {e : Sir} {w : Nat} (h : e.width ≤ w) :
    mask w (e.eval σ) = e.eval σ :=
  mask_of_lt (Nat.lt_of_lt_of_le (eval_lt σ e) (Nat.pow_le_pow_right (by decide) h))

theorem eval_simpResize (σ : Env) (w : Nat) : ∀ e : Sir,
    (simpResize w e).eval σ = mask w (e.eval σ)
  | .lit n => by simp [simpResize, eval]
  | .var x w' => by
    simp only [simpResize]; split
    · simp only [eval]; rw [mask_mask_of_le (by omega)]
    · simp only [eval]; rw [mask_mask_of_ge (by omega)]
  | .resize w' e => by
    simp only [simpResize]; split
    · rw [eval_simpResize σ w e]; simp only [eval]; rw [mask_mask_of_le (by omega)]
    · simp only [eval]; rw [mask_mask_of_ge (by omega)]
  | .bin op w' a b => by
    simp only [simpResize]; split
    · simp only [eval]
      rw [binOp_push op (by omega), binOp_low op w, mask_narrow, mask_narrow, ← binOp_low]
    · simp only [eval]; rw [mask_of_lt (Nat.lt_of_lt_of_le (binOp_lt _ _ _ _)
        (Nat.pow_le_pow_right (by decide) (by omega)))]
  | .bnot w' a => by
    simp only [simpResize]; split
    · simp only [eval]; rw [notW_push (by omega), notW_low w, mask_narrow, ← notW_low]
    · simp only [eval]; rw [mask_of_lt (Nat.lt_of_lt_of_le (notW_lt _ _)
        (Nat.pow_le_pow_right (by decide) (by omega)))]
  | .mux w' c a b => by
    simp only [simpResize]; split
    · simp only [eval]
      split
      · rw [mask_narrow σ w a, mask_mask_of_le (by omega)]
      · rw [mask_narrow σ w b, mask_mask_of_le (by omega)]
    · have := eval_mask_of_width_le σ (e := .mux w' c a b) (w := w) (by simp [width]; omega)
      rw [this]
  | .sext .. | .cmp .. | .scmp .. | .lnot _ | .land .. | .lor .. | .slice .. => by
    simp only [simpResize]; split
    · next h => rw [eval_mask_of_width_le σ h]
    · rfl

theorem sextVal_of_narrow (σ : Env) (w s : Nat) (e : Sir) (h : e.width < s) :
    (simpResize w e).eval σ = sextVal w s (e.eval σ) := by
  rw [eval_simpResize]
  have hlt := eval_lt σ e
  have hle : 2 ^ e.width ≤ 2 ^ (s - 1) := Nat.pow_le_pow_right (by decide) (by omega)
  have hs' : 2 ^ (s - 1) ≤ 2 ^ s := Nat.pow_le_pow_right (by decide) (by omega)
  simp only [sextVal]
  have hm : mask s (e.eval σ) = e.eval σ := mask_of_lt (by omega)
  rw [hm, if_neg (by omega)]

theorem eval_simpSext (σ : Env) (w s : Nat) (e : Sir) :
    (simpSext w s e).eval σ = sextVal w s (e.eval σ) := by
  cases e with
  | lit n => simp [simpSext, eval]
  | _ =>
    simp only [simpSext]; split
    · next h => exact sextVal_of_narrow σ w s _ h
    · rfl

theorem pow_le_pow_of_le {a b : Nat} (h : a ≤ b) : 2 ^ a ≤ 2 ^ b :=
  Nat.pow_le_pow_right (by decide) h

theorem binOp_natural (op : BinOp) {wa wb n : Nat} (hn : natWidth op wa wb = some n)
    {x y : Nat} (hx : x < 2 ^ wa) (hy : y < 2 ^ wb) (w : Nat) (hw : n ≤ w) :
    op.apply n x y = op.apply w x y := by
  have hxm : x < 2 ^ max wa wb := Nat.lt_of_lt_of_le hx (pow_le_pow_of_le (Nat.le_max_left _ _))
  have hym : y < 2 ^ max wa wb := Nat.lt_of_lt_of_le hy (pow_le_pow_of_le (Nat.le_max_right _ _))
  cases op <;> simp only [natWidth, Option.some.injEq, reduceCtorEq] at hn <;> subst hn <;>
    simp only [BinOp.apply]
  · have h : x + y < 2 ^ (max wa wb + 1) := by rw [Nat.pow_succ]; omega
    rw [mask_of_lt h, mask_of_lt (Nat.lt_of_lt_of_le h (pow_le_pow_of_le hw))]
  · have h : x * y < 2 ^ (wa + wb) := by rw [Nat.pow_add]; exact Nat.mul_lt_mul'' hx hy
    rw [mask_of_lt h, mask_of_lt (Nat.lt_of_lt_of_le h (pow_le_pow_of_le hw))]
  · have h : x &&& y < 2 ^ max wa wb := Nat.and_lt_two_pow x hym
    rw [mask_of_lt h, mask_of_lt (Nat.lt_of_lt_of_le h (pow_le_pow_of_le hw))]
  · have h : x ||| y < 2 ^ max wa wb := Nat.or_lt_two_pow hxm hym
    rw [mask_of_lt h, mask_of_lt (Nat.lt_of_lt_of_le h (pow_le_pow_of_le hw))]
  · have h : x ^^^ y < 2 ^ max wa wb := Nat.xor_lt_two_pow hxm hym
    rw [mask_of_lt h, mask_of_lt (Nat.lt_of_lt_of_le h (pow_le_pow_of_le hw))]

theorem eval_shrinkBin (σ : Env) (op : BinOp) (w : Nat) (a b : Sir) :
    (shrinkBin op w a b).eval σ = (Sir.bin op w a b).eval σ := by
  unfold shrinkBin
  split
  · next n hn =>
    split
    · next hw => simp only [eval]; exact binOp_natural op hn (eval_lt σ a) (eval_lt σ b) w hw
    · rfl
  · rfl

theorem eval_shrinkMux (σ : Env) (w : Nat) (c a b : Sir) :
    (shrinkMux w c a b).eval σ = (Sir.mux w c a b).eval σ := by
  unfold shrinkMux
  split
  · next hw =>
    have ha : a.eval σ < 2 ^ max a.width b.width :=
      Nat.lt_of_lt_of_le (eval_lt σ a) (pow_le_pow_of_le (Nat.le_max_left _ _))
    have hb : b.eval σ < 2 ^ max a.width b.width :=
      Nat.lt_of_lt_of_le (eval_lt σ b) (pow_le_pow_of_le (Nat.le_max_right _ _))
    simp only [eval]
    split
    · rw [mask_of_lt ha, mask_of_lt (Nat.lt_of_lt_of_le ha (pow_le_pow_of_le hw))]
    · rw [mask_of_lt hb, mask_of_lt (Nat.lt_of_lt_of_le hb (pow_le_pow_of_le hw))]
  · rfl

theorem eval_foldCmp (σ : Env) (op : CmpOp) (a b : Sir) :
    (foldCmp op a b).eval σ = (Sir.cmp op a b).eval σ := by
  unfold foldCmp; split <;> rfl

theorem eval_foldSCmp (σ : Env) (op : CmpOp) (w : Nat) (a b : Sir) :
    (foldSCmp op w a b).eval σ = (Sir.scmp op w a b).eval σ := by
  unfold foldSCmp; split <;> rfl

theorem eval_norm (σ : Env) : ∀ e : Sir, (norm e).eval σ = e.eval σ
  | .var .. | .lit _ => rfl
  | .resize w e => by simp only [norm, eval]; rw [eval_simpResize, eval_norm σ e]
  | .sext w s e => by simp only [norm, eval]; rw [eval_simpSext, eval_norm σ e]
  | .bin op w a b => by
    simp only [norm]; rw [eval_shrinkBin]; simp only [eval]
    rw [binOp_low, mask_narrow, mask_narrow, eval_norm σ a, eval_norm σ b, ← binOp_low]
  | .bnot w a => by
    simp only [norm, eval]; rw [notW_low, mask_narrow, eval_norm σ a, ← notW_low]
  | .cmp op a b => by
    simp only [norm]; rw [eval_foldCmp]; simp only [eval, eval_norm σ a, eval_norm σ b]
  | .scmp op w a b => by
    simp only [norm]; rw [eval_foldSCmp]; simp only [eval, eval_norm σ a, eval_norm σ b]
  | .mux w c a b => by
    simp only [norm]; rw [eval_shrinkMux]; simp only [eval]
    rw [eval_norm σ c, mask_narrow, mask_narrow, eval_norm σ a, eval_norm σ b]
  | .lnot a => by simp only [norm, eval]; rw [eval_norm σ a]
  | .land a b => by simp only [norm, eval]; rw [eval_norm σ a, eval_norm σ b]
  | .lor a b => by simp only [norm, eval]; rw [eval_norm σ a, eval_norm σ b]
  | .slice hi lo a => by simp only [norm, eval]; rw [eval_norm σ a]

end Sir

end Arch.CodegenProof
