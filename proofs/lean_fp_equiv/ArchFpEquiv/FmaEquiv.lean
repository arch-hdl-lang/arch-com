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

/-- **The fma equivalence.** The bounded sticky-fold fma `arch_fma_f32` is
    bit-identical to the exact-wide 470-bit reference `arch_fma_f32_ref`, for all
    32-bit inputs. Dispatches the full case lattice: special values, zero addend,
    exact cancellation, then the finite non-cancelling regime split by sign,
    alignment gap (`diff ≤ 48` vs `> 48`), and higher-significand magnitude. -/
theorem arch_fma_f32_eq_ref (a b c : BitVec 32) :
    arch_fma_f32 a b c = arch_fma_f32_ref a b c := by
  by_cases hspec : isNaN a = true ∨ isNaN b = true ∨ isNaN c = true ∨ isInf a = true
    ∨ isInf b = true ∨ isInf c = true ∨ isZero a = true ∨ isZero b = true
  · exact fma_eq_ref_special a b c hspec
  · -- ¬special ⇒ a, b finite-nonzero and c not NaN/Inf
    have hna : isNaN a = false := (Bool.eq_false_or_eq_true _).resolve_left (fun h => hspec (Or.inl h))
    have hnb : isNaN b = false :=
      (Bool.eq_false_or_eq_true _).resolve_left (fun h => hspec (Or.inr (Or.inl h)))
    have hnc' : isNaN c = false :=
      (Bool.eq_false_or_eq_true _).resolve_left (fun h => hspec (Or.inr (Or.inr (Or.inl h))))
    have hia : isInf a = false :=
      (Bool.eq_false_or_eq_true _).resolve_left (fun h => hspec (Or.inr (Or.inr (Or.inr (Or.inl h)))))
    have hib : isInf b = false :=
      (Bool.eq_false_or_eq_true _).resolve_left
        (fun h => hspec (Or.inr (Or.inr (Or.inr (Or.inr (Or.inl h))))))
    have hic : isInf c = false :=
      (Bool.eq_false_or_eq_true _).resolve_left
        (fun h => hspec (Or.inr (Or.inr (Or.inr (Or.inr (Or.inr (Or.inl h)))))))
    have hza : isZero a = false :=
      (Bool.eq_false_or_eq_true _).resolve_left
        (fun h => hspec (Or.inr (Or.inr (Or.inr (Or.inr (Or.inr (Or.inr (Or.inl h))))))))
    have hzb : isZero b = false :=
      (Bool.eq_false_or_eq_true _).resolve_left
        (fun h => hspec (Or.inr (Or.inr (Or.inr (Or.inr (Or.inr (Or.inr (Or.inr h))))))))
    have ha : finiteNonzero a = true := by simp [finiteNonzero, hna, hia, hza]
    have hb : finiteNonzero b = true := by simp [finiteNonzero, hnb, hib, hzb]
    by_cases hzc : isZero c = true
    · exact fma_eq_ref_czero a b c ha hb hzc
    · have hzc' : isZero c = false := (Bool.eq_false_or_eq_true _).resolve_left hzc
      have hc : finiteNonzero c = true := by simp [finiteNonzero, hnc', hic, hzc']
      by_cases hcanc : arch_fma_mag98 a b c = 0#98
      · exact fma_eq_ref_cancel a b c ha hb hc hcanc
      · by_cases hsame : BitVec.extractLsb 31 31 c
            = BitVec.extractLsb 31 31 a ^^^ BitVec.extractLsb 31 31 b
        · by_cases hd48 : (fmaDiff98 a b c).toNat ≤ 48
          · exact fma_eq_ref_same_small a b c ha hb hc hsame hd48 hcanc
          · have hdhi : (fmaDiff98 a b c).toNat ≤ 421 := fma_diff98_bound a b c ha hb hc
            by_cases hsig : 2 ^ 23 ≤ (fmaSigHi98 a b c).toNat
            · exact fma_eq_ref_same_big a b c ha hb hc hsame (by omega) hdhi hsig hcanc
            · exact fma_eq_ref_same_big_sub a b c ha hb hc hsame (by omega) hdhi (by omega) hcanc
        · have hdiff : (BitVec.extractLsb 31 31 a ^^^ BitVec.extractLsb 31 31 b
              == BitVec.extractLsb 31 31 c) = false := by
            rw [beq_eq_false_iff_ne]; exact fun h => hsame h.symm
          by_cases hd48 : (fmaDiff98 a b c).toNat ≤ 48
          · exact fma_eq_ref_diff_small a b c ha hb hc hdiff hd48 hcanc
          · have hdhi : (fmaDiff98 a b c).toNat ≤ 421 := fma_diff98_bound a b c ha hb hc
            by_cases hsig_gt : 2 ^ 23 < (fmaSigHi98 a b c).toNat
            · exact fma_eq_ref_diff_big a b c ha hb hc hdiff (by omega) hdhi hsig_gt hcanc
            · by_cases hsig_lt : (fmaSigHi98 a b c).toNat < 2 ^ 23
              · exact fma_eq_ref_diff_big_sub a b c ha hb hc hdiff (by omega) hdhi hsig_lt hcanc
              · have hsigeq : (fmaSigHi98 a b c).toNat = 2 ^ 23 := by omega
                by_cases hd49 : (fmaDiff98 a b c).toNat = 49
                · exact fma_eq_ref_diff49 a b c ha hb hc hd49 hcanc
                · exact fma_eq_ref_diff_boundary a b c ha hb hc hdiff (by omega) hdhi hsigeq hcanc

end ArchFp
