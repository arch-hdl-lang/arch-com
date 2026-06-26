/-!
# Tier 2, part 3 — the arithmetic heart of round-to-nearest-even

The residual of `arch_round48_correct` (`Equiv.lean`) is the rounding *direction*
on inexact inputs. Stripped of the bit plumbing, that direction is one integer
fact: arch's **guard / round / sticky** decision computes round-to-nearest-even.
This file proves exactly that, in pure `Nat` arithmetic (core only — no Mathlib;
the Mathlib olean cache is egress-blocked in this environment anyway).

Setup: dropping the low `sh` bits of `n` keeps the quotient `q = n / 2^sh` with
dropped remainder `r = n % 2^sh`; the half-ULP boundary is `half = 2^(sh-1)`.
arch rounds up iff `guard ∧ (sticky ∨ q odd)`, where `guard ⟺ half ≤ r` (the top
dropped bit) and `sticky ⟺ r % half ≠ 0` (any lower dropped bit). The IEEE rule
rounds up iff `r > half`, or `r = half` with `q` odd (ties to even).

`rne_matches` proves arch's `q + (guard ∧ (sticky ∨ odd))` equals the IEEE rule,
combining the two ingredients below. This is the op-independent rounding kernel
the bit-level proof reduces to (via the `BitVec`→`Nat` shift bridge already
established by `msb_index_bound`).
-/

namespace ArchFp

/-- Under the guard precondition `half ≤ r < 2·half`, arch's sticky test
    (`r % half ≠ 0`) is exactly `r ≠ half`. -/
theorem sticky_reduce (r half : Nat) (hg : half ≤ r) (hr : r < 2 * half) :
    (r % half ≠ 0) ↔ (r ≠ half) := by
  rcases Nat.eq_or_lt_of_le hg with h | h
  · subst h; simp [Nat.mod_self]
  · have hrh : r - half < half := by omega
    have hmod : r % half = r - half := by
      rw [Nat.mod_eq_sub_mod (Nat.le_of_lt h), Nat.mod_eq_of_lt hrh]
    rw [hmod]; omega

/-- The nearest-even decision as pure logic: `guard ∧ (sticky' ∨ q odd)` (with
    `sticky'` already reduced to `r ≠ half`) iff `r > half ∨ (r = half ∧ q odd)`. -/
theorem rne_decision (q r half : Nat) :
    (half ≤ r ∧ (r ≠ half ∨ q % 2 = 1))
      ↔ (half < r ∨ (r = half ∧ q % 2 = 1)) := by
  constructor
  · rintro ⟨hg, hs⟩
    rcases Nat.lt_or_ge half r with h | h
    · exact Or.inl h
    · have heq : r = half := Nat.le_antisymm h hg
      rcases hs with h1 | h1
      · exact absurd heq h1
      · exact Or.inr ⟨heq, h1⟩
  · rintro (h | ⟨h1, h2⟩)
    · exact ⟨Nat.le_of_lt h, Or.inl (by omega)⟩
    · exact ⟨by omega, Or.inr h2⟩

/-- arch's guard/sticky round-up bit for dropping `sh` low bits of `n`. -/
def guardStickyUp (n sh : Nat) : Bool :=
  (2 ^ (sh - 1) ≤ n % 2 ^ sh)
    && ((n % 2 ^ sh % 2 ^ (sh - 1) ≠ 0) || ((n / 2 ^ sh) % 2 = 1))

/-- IEEE round-to-nearest-even quotient when dropping `sh` low bits of `n`. -/
def rneQuot (n sh : Nat) : Nat :=
  n / 2 ^ sh
    + (if 2 ^ (sh - 1) < n % 2 ^ sh
          ∨ (n % 2 ^ sh = 2 ^ (sh - 1) ∧ (n / 2 ^ sh) % 2 = 1) then 1 else 0)

/-- **The rounding kernel.** For `sh ≥ 1`, arch's guard/round/sticky decision
    rounds `n / 2^sh` to nearest-even: `q + (guard ∧ (sticky ∨ odd q)) = rneQuot`. -/
theorem rne_matches (n sh : Nat) (hsh : 1 ≤ sh) :
    n / 2 ^ sh + (if guardStickyUp n sh then 1 else 0) = rneQuot n sh := by
  have hhpos : 0 < 2 ^ (sh - 1) := Nat.pow_pos (by decide : 0 < 2)
  have h2 : 2 ^ (sh - 1) * 2 = 2 ^ sh := by
    rw [← Nat.pow_succ]; congr 1; omega
  have hspos : 0 < 2 ^ sh := Nat.pow_pos (by decide : 0 < 2)
  have hr : n % 2 ^ sh < 2 * 2 ^ (sh - 1) := by
    have := Nat.mod_lt n hspos; omega
  -- reduce arch's Bool decision to the IEEE predicate
  have key : (guardStickyUp n sh = true) ↔
      (2 ^ (sh - 1) < n % 2 ^ sh
        ∨ (n % 2 ^ sh = 2 ^ (sh - 1) ∧ (n / 2 ^ sh) % 2 = 1)) := by
    rw [guardStickyUp]
    simp only [Bool.and_eq_true, Bool.or_eq_true, decide_eq_true_eq]
    constructor
    · rintro ⟨hg, hs⟩
      have hs' : n % 2 ^ sh ≠ 2 ^ (sh - 1) ∨ (n / 2 ^ sh) % 2 = 1 := by
        rcases hs with h | h
        · exact Or.inl ((sticky_reduce _ _ hg hr).mp h)
        · exact Or.inr h
      exact (rne_decision _ _ _).mp ⟨hg, hs'⟩
    · intro h
      obtain ⟨hg, hs'⟩ := (rne_decision (n / 2 ^ sh) (n % 2 ^ sh) (2 ^ (sh - 1))).mpr h
      refine ⟨hg, ?_⟩
      rcases hs' with h | h
      · exact Or.inl ((sticky_reduce _ _ hg hr).mpr h)
      · exact Or.inr h
  rw [rneQuot]
  by_cases hb : guardStickyUp n sh
  · rw [if_pos hb, if_pos (key.mp hb)]
  · rw [if_neg hb]
    have : ¬ (2 ^ (sh - 1) < n % 2 ^ sh
        ∨ (n % 2 ^ sh = 2 ^ (sh - 1) ∧ (n / 2 ^ sh) % 2 = 1)) := fun h => hb (key.mpr h)
    rw [if_neg this]

end ArchFp
