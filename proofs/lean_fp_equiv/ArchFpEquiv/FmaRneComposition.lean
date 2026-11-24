import ArchFpEquiv.SmtLibRoundingBridge

/-!
# Exact FMA arithmetic composed with the full RNE result contract

This theorem removes the old non-overflow premise from the finite nonzero,
non-cancelling composition: overflow is a specified result, not excluded.
The exact value is the independently stated rational product-plus-sum.
The SMT-LIB oracle comparison remains external; no solver axiom is imported.
-/
namespace ArchFp

set_option maxRecDepth 10000

/-- Exact cancellation in the exact-wide reference is positive zero. Both
sides inline the same product, so this is a structural bit-vector proof. -/
theorem fma_ref_exact_cancel (a b c : BitVec 32)
    (ha : finiteNonzero a = true) (hb : finiteNonzero b = true)
    (hc : finiteNonzero c = true) (hm : arch_fma_mag a b c = 0#470) :
    arch_fma_f32_ref a b c = 0#32 := by
  unfold finiteNonzero isNaN isInf isZero expField fracField
    arch_fma_f32_ref arch_fma_mag at *
  bv_decide (config := { timeout := 600 })

/-- No non-cancellation hypothesis: exact cancellation is covered as +0. -/
theorem arch_fma_exact_cancellation (a b c : BitVec 32)
    (ha : finiteNonzero a = true) (hb : finiteNonzero b = true)
    (hc : finiteNonzero c = true) (hz : fmaExact a b c = 0) :
    arch_fma_f32 a b c = 0#32 := by
  rw [arch_fma_f32_eq_ref]
  exact fma_ref_exact_cancel a b c ha hb hc
    ((fma_mag_zero_iff a b c ha hb hc).mpr hz)

/-- The exact arithmetic identity, range needed by the independent rounder
comparison, and complete RNE contract for the emitted bounded FMA. -/
theorem arch_fma_exact_rne_composition (a b c : BitVec 32)
    (ha : finiteNonzero a = true) (hb : finiteNonzero b = true)
    (hc : finiteNonzero c = true) (hnz : fmaExact a b c ≠ 0) :
    (arch_fma_mag a b c).toNat * 2 ^ ((arch_fma_elo a b c).toInt + 298).toNat =
        (fmaExact a b c).natAbs ∧
    textbookFmaExact a b c = (fmaExact a b c : Rat) / 2 ^ 298 ∧
    -298 <= (arch_fma_elo a b c).toInt ∧
    (arch_fma_elo a b c).toInt <= 208 ∧
    RneF32Result (arch_fma_sign a b c == 1#1)
      (arch_fma_mag a b c).toNat (arch_fma_elo a b c).toInt
      (arch_fma_f32 a b c) := by
  have hm : arch_fma_mag a b c ≠ 0#470 := by
    intro hz
    exact hnz ((fma_mag_zero_iff a b c ha hb hc).mp hz)
  obtain ⟨hlo, hhi⟩ := fma_elo_bounds a b c ha hb hc
  refine ⟨fma_mag_exact a b c ha hb hc, textbookFmaExact_eq_scaled a b c,
    hlo, hhi, ?_⟩
  rw [arch_fma_f32_eq_ref, fma_reduce_ref a b c ha hb hc hm]
  exact arch_round470_rne_spec _ _ _ hlo hhi

/-- The RNE sign is the sign of the exact mathematical sum, not just a
datapath field. In particular this includes correctly signed underflow zero. -/
theorem arch_fma_exact_sign_rne (a b c : BitVec 32)
    (ha : finiteNonzero a = true) (hb : finiteNonzero b = true)
    (hc : finiteNonzero c = true) (hnz : fmaExact a b c ≠ 0) :
    RneF32Result (decide (fmaExact a b c < 0))
      (arch_fma_mag a b c).toNat (arch_fma_elo a b c).toInt
      (arch_fma_f32 a b c) := by
  have contract := (arch_fma_exact_rne_composition a b c ha hb hc hnz).2.2.2.2
  rw [fma_sign_exact a b c ha hb hc hnz] at contract
  by_cases hneg : fmaExact a b c < 0
  · simpa [hneg] using contract
  · simpa [hneg] using contract

/-- Together, the two cases cover every triple of finite nonzero operands,
including exact cancellation, underflow, ties, carry, and overflow. -/
theorem arch_fma_finite_result_cases (a b c : BitVec 32)
    (ha : finiteNonzero a = true) (hb : finiteNonzero b = true)
    (hc : finiteNonzero c = true) :
    (fmaExact a b c = 0 ∧ arch_fma_f32 a b c = 0#32) ∨
    (fmaExact a b c ≠ 0 ∧
      RneF32Result (arch_fma_sign a b c == 1#1)
        (arch_fma_mag a b c).toNat (arch_fma_elo a b c).toInt
        (arch_fma_f32 a b c)) := by
  by_cases hz : fmaExact a b c = 0
  · exact Or.inl ⟨hz, arch_fma_exact_cancellation a b c ha hb hc hz⟩
  · exact Or.inr ⟨hz, (arch_fma_exact_rne_composition a b c ha hb hc hz).2.2.2.2⟩

end ArchFp
