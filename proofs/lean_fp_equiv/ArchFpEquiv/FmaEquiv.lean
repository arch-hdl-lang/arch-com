import ArchFpEquiv.Fma
import ArchFpEquiv.FmaRefSpecial
import ArchFpEquiv.FmaInvariance

/-!
# `arch_fma_f32 = arch_fma_f32_ref` — assembling the cases

The special-value region is dispatched by chaining the sticky-fold lattice
(`Fma.lean`) and the reference lattice (`FmaRefSpecial.lean`): on each
NaN / Inf / zero-product case both reduce to the *same* value, so the equality is
`rw` of the two lattice lemmas. The finite-nonzero region is covered by the
invariance cases in `FmaInvariance.lean`.
-/

namespace ArchFp

set_option maxHeartbeats 4000000
set_option maxRecDepth 100000

/-- **Exact-cancellation equivalence.** When the sticky-fold magnitude vanishes,
    both fmas are `+0`. -/
theorem fma_eq_ref_cancel (a b c : BitVec 32)
    (ha : finiteNonzero a = true) (hb : finiteNonzero b = true) (hc : finiteNonzero c = true)
    (hcanc : arch_fma_mag98 a b c = 0#98) :
    arch_fma_f32 a b c = arch_fma_f32_ref a b c := by
  rw [fma_cancel98 a b c ha hb hc hcanc, fma_ref_cancel98 a b c ha hb hc hcanc]

/-- **Zero-addend equivalence.** With `a,b` finite-nonzero and `c = 0`, both fmas
    round the product alone — `bv_decide` equates them directly (the `c = 0` branch
    prunes the addend, leaving the shared product rounding). -/
theorem fma_eq_ref_czero (a b c : BitVec 32)
    (ha : finiteNonzero a = true) (hb : finiteNonzero b = true) (hcz : isZero c = true) :
    arch_fma_f32 a b c = arch_fma_f32_ref a b c := by
  unfold finiteNonzero isNaN isInf isZero expField fracField
    arch_fma_f32 arch_fma_f32_ref at *
  bv_decide (config := { timeout := 600 })

/-- **Special-value equivalence.** When any operand is NaN/Inf, or the product has
    a zero factor, the sticky-fold and reference fma agree — both short-circuit to
    the same special value (NaN, signed infinity, or the reduced adder call). -/
theorem fma_eq_ref_special (a b c : BitVec 32)
    (h : isNaN a = true ∨ isNaN b = true ∨ isNaN c = true ∨ isInf a = true ∨ isInf b = true
      ∨ isInf c = true ∨ isZero a = true ∨ isZero b = true) :
    arch_fma_f32 a b c = arch_fma_f32_ref a b c := by
  by_cases hnan : isNaN a = true ∨ isNaN b = true ∨ isNaN c = true
  · rw [fma_nan a b c hnan, fma_ref_nan a b c hnan]
  · have hna : isNaN a = false := by
      cases hh : isNaN a
      · rfl
      · exact absurd (Or.inl hh) hnan
    have hnb : isNaN b = false := by
      cases hh : isNaN b
      · rfl
      · exact absurd (Or.inr (Or.inl hh)) hnan
    have hnc : isNaN c = false := by
      cases hh : isNaN c
      · rfl
      · exact absurd (Or.inr (Or.inr hh)) hnan
    by_cases hzti : (isZero a = true ∧ isInf b = true) ∨ (isInf a = true ∧ isZero b = true)
    · rw [fma_zero_times_inf a b c hzti, fma_ref_zero_times_inf a b c hzti]
    · by_cases hpi : isInf a = true ∨ isInf b = true
      · by_cases hci : isInf c = true
        · by_cases hsgn : sgn c = sgn a ^^^ sgn b
          · have hcc : isInf c = false ∨ sgn c = sgn a ^^^ sgn b := Or.inr hsgn
            rw [fma_inf_prod a b c hna hnb hnc hpi hzti hcc,
                fma_ref_inf_prod a b c hna hnb hnc hpi hzti hcc]
          · rw [fma_inf_minus_inf a b c hna hnb hpi hci hsgn,
                fma_ref_inf_minus_inf a b c hna hnb hpi hci hsgn]
        · have hcif : isInf c = false := by
            cases hh : isInf c
            · rfl
            · exact absurd hh hci
          have hcc : isInf c = false ∨ sgn c = sgn a ^^^ sgn b := Or.inl hcif
          rw [fma_inf_prod a b c hna hnb hnc hpi hzti hcc,
              fma_ref_inf_prod a b c hna hnb hnc hpi hzti hcc]
      · have hpa : isInf a = false := by
          cases hh : isInf a
          · rfl
          · exact absurd (Or.inl hh) hpi
        have hpb : isInf b = false := by
          cases hh : isInf b
          · rfl
          · exact absurd (Or.inr hh) hpi
        by_cases hci : isInf c = true
        · rw [fma_inf_c a b c hna hnb hnc hpa hpb hci,
              fma_ref_inf_c a b c hna hnb hnc hpa hpb hci]
        · have hcif : isInf c = false := by
            cases hh : isInf c
            · rfl
            · exact absurd hh hci
          have hpz : isZero a = true ∨ isZero b = true := by
            rcases h with h | h | h | h | h | h | h | h
            · simp [hna] at h
            · simp [hnb] at h
            · simp [hnc] at h
            · simp [hpa] at h
            · simp [hpb] at h
            · simp [hcif] at h
            · exact Or.inl h
            · exact Or.inr h
          rw [fma_prod_zero a b c hna hnb hpa hpb hnc hcif hpz,
              fma_ref_prod_zero a b c hna hnb hpa hpb hnc hcif hpz]

end ArchFp
