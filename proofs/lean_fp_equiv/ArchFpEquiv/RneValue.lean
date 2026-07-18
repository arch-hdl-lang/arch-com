import ArchFpEquiv.RoundCore

/-!
# R1 — value-level correctness of the rounding kernel

`RoundCore.rne_matches` proves arch's guard/round/sticky decision *equals* the
defined quotient `rneQuot`. That closes the gap between the hardware and the
definition — but `rneQuot` itself was, until this file, trusted by inspection
to BE round-to-nearest-even. Here we discharge that trust with the value-level
characterization (phase R1 of the real-valued IEEE-754 anchor):

* `rneQuot_nearest`  — `rneQuot n sh · 2^sh` is at least as close to `n` as
  `m · 2^sh` for **every** `m : Nat` (the minimizer over the grid);
* `rneQuot_strict`   — off ties, it is the **unique** minimizer;
* `rneQuot_tie_even` — on a tie (`n % 2^sh = 2^(sh-1)`), the result is even;
* `rneQuot_halfulp`  — the rounding error never exceeds half a ULP.

Together: `rneQuot` is exactly the IEEE-754 round-to-nearest-even quotient.
Distances are the `Nat` absolute difference (`(a - b) + (b - a)` with
truncated subtraction — exactly `|a - b|`), so the statements read as ordinary
absolute differences and stay `omega`-native. Everything is core-only; `omega`
closes each
branch once the grid points are related linearly (`Nat.succ_mul`,
`Nat.mul_le_mul_right`) in a single multiplication orientation (`X * 2^sh`).
-/

namespace ArchFp

/-- Absolute distance from the grid point `m · 2^sh` to `n` (the `Nat`
    absolute difference: truncated subtraction in both directions). -/
def gridDist (n m sh : Nat) : Nat := (m * 2 ^ sh - n) + (n - m * 2 ^ sh)

/-- `n` splits on the `2^sh` grid: `n = (n / 2^sh) · 2^sh + n % 2^sh`,
    with the remainder below `2^sh` and `2^sh = 2 · 2^(sh-1)` for `sh ≥ 1`. -/
private theorem kernel_facts (n sh : Nat) (hsh : 1 ≤ sh) :
    n = (n / 2 ^ sh) * 2 ^ sh + n % 2 ^ sh
      ∧ n % 2 ^ sh < 2 ^ sh
      ∧ 2 ^ sh = 2 * 2 ^ (sh - 1) := by
  refine ⟨?_, Nat.mod_lt _ (Nat.pow_pos (by decide)), ?_⟩
  · have h := Nat.div_add_mod n (2 ^ sh)
    rw [Nat.mul_comm] at h
    omega
  · have h : 2 ^ sh = 2 ^ (sh - 1 + 1) := by congr 1; omega
    rw [h, Nat.pow_succ]
    omega

/-- The rounded quotient is `n / 2^sh` or its successor. -/
theorem rneQuot_cases (n sh : Nat) :
    rneQuot n sh = n / 2 ^ sh ∨ rneQuot n sh = n / 2 ^ sh + 1 := by
  rw [rneQuot]; split <;> simp

/-- **Nearest.** For `sh ≥ 1`, no grid point `m · 2^sh` is closer to `n` than
    `rneQuot n sh · 2^sh`. -/
theorem rneQuot_nearest (n sh : Nat) (hsh : 1 ≤ sh) (m : Nat) :
    gridDist n (rneQuot n sh) sh ≤ gridDist n m sh := by
  obtain ⟨hdm, hrlt, h2⟩ := kernel_facts n sh hsh
  rw [gridDist, gridDist, rneQuot]
  have hq1 : (n / 2 ^ sh + 1) * 2 ^ sh = (n / 2 ^ sh) * 2 ^ sh + 2 ^ sh :=
    Nat.succ_mul _ _
  by_cases hup : 2 ^ (sh - 1) < n % 2 ^ sh
      ∨ (n % 2 ^ sh = 2 ^ (sh - 1) ∧ (n / 2 ^ sh) % 2 = 1)
  · rw [if_pos hup]
    have hhr : 2 ^ (sh - 1) ≤ n % 2 ^ sh := by
      rcases hup with h | ⟨h, _⟩ <;> omega
    rcases Nat.lt_or_ge m (n / 2 ^ sh + 1) with hm | hm
    · have hmul : m * 2 ^ sh ≤ (n / 2 ^ sh) * 2 ^ sh :=
        Nat.mul_le_mul_right _ (by omega)
      omega
    · have hmul : (n / 2 ^ sh + 1) * 2 ^ sh ≤ m * 2 ^ sh :=
        Nat.mul_le_mul_right _ hm
      omega
  · rw [if_neg hup]
    simp only [Nat.add_zero]
    have hhr : n % 2 ^ sh ≤ 2 ^ (sh - 1) := by
      rcases Nat.lt_or_ge (2 ^ (sh - 1)) (n % 2 ^ sh) with h | h
      · exact absurd (Or.inl h) hup
      · exact h
    rcases Nat.lt_or_ge m (n / 2 ^ sh + 1) with hm | hm
    · have hmul : m * 2 ^ sh ≤ (n / 2 ^ sh) * 2 ^ sh :=
        Nat.mul_le_mul_right _ (by omega)
      omega
    · have hmul : (n / 2 ^ sh + 1) * 2 ^ sh ≤ m * 2 ^ sh :=
        Nat.mul_le_mul_right _ hm
      omega

/-- **Strict off ties.** Away from the exact half-ULP tie, `rneQuot` is the
    unique nearest grid point: every other `m` is strictly farther. -/
theorem rneQuot_strict (n sh : Nat) (hsh : 1 ≤ sh)
    (htie : n % 2 ^ sh ≠ 2 ^ (sh - 1)) (m : Nat) (hm : m ≠ rneQuot n sh) :
    gridDist n (rneQuot n sh) sh < gridDist n m sh := by
  obtain ⟨hdm, hrlt, h2⟩ := kernel_facts n sh hsh
  rw [gridDist, gridDist] at *
  rw [rneQuot] at hm ⊢
  have hq1 : (n / 2 ^ sh + 1) * 2 ^ sh = (n / 2 ^ sh) * 2 ^ sh + 2 ^ sh :=
    Nat.succ_mul _ _
  have hq2 : (n / 2 ^ sh + 2) * 2 ^ sh
      = (n / 2 ^ sh) * 2 ^ sh + 2 ^ sh + 2 ^ sh := by
    rw [Nat.add_mul]; omega
  by_cases hup : 2 ^ (sh - 1) < n % 2 ^ sh
      ∨ (n % 2 ^ sh = 2 ^ (sh - 1) ∧ (n / 2 ^ sh) % 2 = 1)
  · rw [if_pos hup] at hm ⊢
    have hhr : 2 ^ (sh - 1) < n % 2 ^ sh := by
      rcases hup with h | ⟨h, _⟩
      · exact h
      · exact absurd h htie
    rcases Nat.lt_or_ge m (n / 2 ^ sh + 1) with hmc | hmc
    · have hmul : m * 2 ^ sh ≤ (n / 2 ^ sh) * 2 ^ sh :=
        Nat.mul_le_mul_right _ (by omega)
      omega
    · have hm2 : n / 2 ^ sh + 2 ≤ m := by omega
      have hmul : (n / 2 ^ sh + 2) * 2 ^ sh ≤ m * 2 ^ sh :=
        Nat.mul_le_mul_right _ hm2
      omega
  · rw [if_neg hup] at hm ⊢
    simp only [Nat.add_zero] at hm ⊢
    have hhr : n % 2 ^ sh < 2 ^ (sh - 1) := by
      rcases Nat.lt_or_ge (n % 2 ^ sh) (2 ^ (sh - 1)) with h | h
      · exact h
      · rcases Nat.eq_or_lt_of_le h with h' | h'
        · exact absurd h'.symm htie
        · exact absurd (Or.inl h') hup
    rcases Nat.lt_or_ge m (n / 2 ^ sh) with hmc | hmc
    · have hmul : (m + 1) * 2 ^ sh ≤ (n / 2 ^ sh) * 2 ^ sh :=
        Nat.mul_le_mul_right _ (by omega)
      have hm1 : (m + 1) * 2 ^ sh = m * 2 ^ sh + 2 ^ sh := Nat.succ_mul _ _
      omega
    · have hm1 : n / 2 ^ sh + 1 ≤ m := by omega
      have hmul : (n / 2 ^ sh + 1) * 2 ^ sh ≤ m * 2 ^ sh :=
        Nat.mul_le_mul_right _ hm1
      omega

/-- **Ties to even.** On the exact half-ULP tie the kernel returns an even
    quotient. -/
theorem rneQuot_tie_even (n sh : Nat) (hsh : 1 ≤ sh)
    (htie : n % 2 ^ sh = 2 ^ (sh - 1)) :
    rneQuot n sh % 2 = 0 := by
  obtain ⟨hdm, hrlt, h2⟩ := kernel_facts n sh hsh
  rw [rneQuot]
  by_cases hodd : (n / 2 ^ sh) % 2 = 1
  · rw [if_pos (Or.inr ⟨htie, hodd⟩)]; omega
  · have hno : ¬ (2 ^ (sh - 1) < n % 2 ^ sh
        ∨ (n % 2 ^ sh = 2 ^ (sh - 1) ∧ (n / 2 ^ sh) % 2 = 1)) := by
      intro h
      rcases h with h | ⟨_, h⟩
      · omega
      · exact hodd h
    rw [if_neg hno]; omega

/-- **Half-ULP bound.** The rounding error never exceeds half a ULP. -/
theorem rneQuot_halfulp (n sh : Nat) (hsh : 1 ≤ sh) :
    gridDist n (rneQuot n sh) sh ≤ 2 ^ (sh - 1) := by
  obtain ⟨hdm, hrlt, h2⟩ := kernel_facts n sh hsh
  rw [gridDist, rneQuot]
  have hq1 : (n / 2 ^ sh + 1) * 2 ^ sh = (n / 2 ^ sh) * 2 ^ sh + 2 ^ sh :=
    Nat.succ_mul _ _
  by_cases hup : 2 ^ (sh - 1) < n % 2 ^ sh
      ∨ (n % 2 ^ sh = 2 ^ (sh - 1) ∧ (n / 2 ^ sh) % 2 = 1)
  · rw [if_pos hup]
    have hhr : 2 ^ (sh - 1) ≤ n % 2 ^ sh := by
      rcases hup with h | ⟨h, _⟩ <;> omega
    omega
  · rw [if_neg hup]
    simp only [Nat.add_zero]
    have hhr : n % 2 ^ sh ≤ 2 ^ (sh - 1) := by
      rcases Nat.lt_or_ge (2 ^ (sh - 1)) (n % 2 ^ sh) with h | h
      · exact absurd (Or.inl h) hup
      · exact h
    omega

end ArchFp
