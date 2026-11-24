import ArchFpEquiv.F32PairwiseSum

namespace ArchFp

section
variable {α : Type} (op : α → α → α) (v : α → Rat)

/-- Per-add bounds only for the pairs actually evaluated in one round. -/
def PairErrors (u : Rat) : List α → Prop
  | a :: b :: rest => |v (op a b) - (v a + v b)| ≤ u * |v a + v b| ∧ PairErrors u rest
  | _ => True

/-- Finite-tree safety; no closure under unrelated or repeated additions. -/
def TreeErrors (u : Rat) : Nat → List α → Prop
  | 0, _ => True
  | d + 1, xs => PairErrors op v u xs ∧ TreeErrors u d (gpairUp op xs)

theorem gpairUp_err_local {u : Rat} (hu : 0 ≤ u)
    : ∀ xs : List α, PairErrors op v u xs → |((gpairUp op xs).map v).sum - (xs.map v).sum|
      ≤ u * (xs.map (fun x => |v x|)).sum
  | [], _ => by simp [gpairUp]
  | [a], _ => by simp [gpairUp]; positivity
  | a :: b :: rest, h => by
      have ih := gpairUp_err_local hu rest h.2
      simp only [gpairUp, List.map_cons, List.sum_cons]
      calc |v (op a b) + ((gpairUp op rest).map v).sum - (v a + (v b + (rest.map v).sum))|
          = |(v (op a b) - (v a + v b)) + (((gpairUp op rest).map v).sum - (rest.map v).sum)| := by
            ring_nf
        _ ≤ |v (op a b) - (v a + v b)| + |((gpairUp op rest).map v).sum - (rest.map v).sum| :=
            abs_add_le _ _
        _ ≤ u * |v a + v b| + u * (rest.map (fun x => |v x|)).sum := by gcongr; exact h.1
        _ ≤ u * (|v a| + |v b|) + u * (rest.map (fun x => |v x|)).sum := by
            gcongr; exact abs_add_le _ _
        _ = u * (|v a| + (|v b| + (rest.map (fun x => |v x|)).sum)) := by ring

theorem gpairUp_absSum_local {u : Rat} (hu : 0 ≤ u)
    : ∀ xs : List α, PairErrors op v u xs → ((gpairUp op xs).map (fun x => |v x|)).sum
      ≤ (1 + u) * (xs.map (fun x => |v x|)).sum
  | [], _ => by simp [gpairUp]
  | [a], _ => by simp [gpairUp]; nlinarith [abs_nonneg (v a)]
  | a :: b :: rest, h => by
      have ih := gpairUp_absSum_local hu rest h.2
      have hb : |v (op a b)| ≤ (1 + u) * (|v a| + |v b|) :=
        calc |v (op a b)| ≤ (1 + u) * |v a + v b| := by
              nlinarith [h.1, abs_nonneg (v a + v b),
                abs_sub_abs_le_abs_sub (v (op a b)) (v a + v b)]
          _ ≤ (1 + u) * (|v a| + |v b|) := by
              have h0 : (0:Rat) ≤ 1 + u := by linarith
              gcongr; exact abs_add_le _ _
      simp only [gpairUp, List.map_cons, List.sum_cons]
      calc |v (op a b)| + ((gpairUp op rest).map (fun x => |v x|)).sum
          ≤ (1 + u) * (|v a| + |v b|) + (1 + u) * (rest.map (fun x => |v x|)).sum := by gcongr
        _ = (1 + u) * (|v a| + (|v b| + (rest.map (fun x => |v x|)).sum)) := by ring


variable [Inhabited α]

theorem gpair_bound_tree {u : Rat} (hu : 0 < u)
    :
    ∀ (d : Nat) (xs : List α), xs ≠ [] → xs.length ≤ 2 ^ d → ((d:Rat) + 1) * u < 1 → TreeErrors op v u d xs →
      |v (gpairSum op xs) - (xs.map v).sum| ≤ agamma u d * (xs.map (fun x => |v x|)).sum := by
  intro d
  induction d with
  | zero =>
    intro xs hne hlen _ _
    match xs, hne, hlen with
    | [x], _, _ => simp [gpairSum, agamma]
  | succ d IH =>
    intro xs hne hlen hdu htree
    have hduD : ((d:Rat) + 1) * u < 1 := by push_cast at hdu; nlinarith [hu]
    have hgnn : 0 ≤ agamma u d := agamma_nonneg u (le_of_lt hu) d (by nlinarith [hu, hduD])
    have hgnn1 : 0 ≤ agamma u (d + 1) := by
      apply agamma_nonneg u (le_of_lt hu) (d + 1); push_cast; nlinarith [hu, hdu]
    match xs with
    | [x] => simp only [gpairSum, List.map_cons, List.map_nil, List.sum_cons,
        List.sum_nil, add_zero, sub_self, abs_zero]; exact mul_nonneg hgnn1 (by positivity)
    | a :: b :: rest =>
      have hqlen : (gpairUp op (a::b::rest)).length ≤ 2 ^ d := by
        rw [gpairUp_length]; simp only [List.length_cons] at hlen ⊢; omega
      have hqne : gpairUp op (a::b::rest) ≠ [] :=
        List.ne_nil_of_length_pos (by rw [gpairUp_length]; simp only [List.length_cons]; omega)
      have hih := IH (gpairUp op (a::b::rest)) hqne hqlen hduD htree.2
      have herr := gpairUp_err_local op v (le_of_lt hu) (a::b::rest) htree.1
      have hgrow := gpairUp_absSum_local op v (le_of_lt hu) (a::b::rest) htree.1
      rw [show gpairSum op (a::b::rest) = gpairSum op (gpairUp op (a::b::rest)) by rw [gpairSum]]
      calc |v (gpairSum op (gpairUp op (a::b::rest))) - ((a::b::rest).map v).sum|
          ≤ |v (gpairSum op (gpairUp op (a::b::rest))) - ((gpairUp op (a::b::rest)).map v).sum|
              + |((gpairUp op (a::b::rest)).map v).sum - ((a::b::rest).map v).sum| := abs_sub_le _ _ _
        _ ≤ agamma u d * ((gpairUp op (a::b::rest)).map (fun x => |v x|)).sum
              + u * ((a::b::rest).map (fun x => |v x|)).sum := by gcongr
        _ ≤ agamma u d * ((1 + u) * ((a::b::rest).map (fun x => |v x|)).sum)
              + u * ((a::b::rest).map (fun x => |v x|)).sum := by gcongr
        _ = ((1 + u) * agamma u d + u) * ((a::b::rest).map (fun x => |v x|)).sum := by ring
        _ ≤ agamma u (d + 1) * ((a::b::rest).map (fun x => |v x|)).sum := by
            gcongr
            · exact mapAbs_nonneg v _
            · exact agamma_step u hu d hduD

end


/-- Integer check of the local relative-error bound in units of 2^-149. -/
def checkAddError (a b : BitVec 32) : Bool :=
  decide (|f32SignedScaled (arch_f32_add a b) -
    (f32SignedScaled a + f32SignedScaled b)| * (2 : Int)^24 ≤
      |f32SignedScaled a + f32SignedScaled b|)

theorem checkAddError_sound (a b : BitVec 32) (h : checkAddError a b = true) :
    |f32R (arch_f32_add a b) - (f32R a + f32R b)| ≤ f32u * |f32R a + f32R b| := by
  have hi := of_decide_eq_true h
  have hr : |(f32SignedScaled (arch_f32_add a b) : Rat) -
      ((f32SignedScaled a : Rat) + (f32SignedScaled b : Rat))| * 2^24 ≤
      |(f32SignedScaled a : Rat) + (f32SignedScaled b : Rat)| := by exact_mod_cast hi
  unfold f32R f32u
  rw [← add_div, ← sub_div, abs_div, abs_div]
  norm_num
  norm_num at hr
  linarith

def checkLevel : List (BitVec 32) → Bool
  | a :: b :: rest => decide (IsFiniteF32 a) && decide (IsFiniteF32 b) &&
      decide (IsFiniteF32 (arch_f32_add a b)) && checkAddError a b && checkLevel rest
  | [a] => decide (IsFiniteF32 a)
  | [] => true

def checkTree : Nat → List (BitVec 32) → Bool
  | 0, _ => true
  | d + 1, xs => checkLevel xs && checkTree d (pairUp xs)

theorem checkLevel_sound : ∀ xs : List (BitVec 32), checkLevel xs = true →
    PairErrors arch_f32_add f32R f32u xs
  | [], _ => trivial
  | [_], _ => trivial
  | a :: b :: rest, h => by
    simp only [checkLevel, Bool.and_eq_true] at h
    exact ⟨checkAddError_sound a b h.1.2, checkLevel_sound rest h.2⟩

theorem checkTree_sound : ∀ (d : Nat) (xs : List (BitVec 32)), checkTree d xs = true →
    TreeErrors arch_f32_add f32R f32u d xs
  | 0, _, _ => trivial
  | d + 1, xs, h => by
    simp only [checkTree, Bool.and_eq_true] at h
    refine ⟨checkLevel_sound xs h.1, ?_⟩
    rw [← pairUp_eq]
    exact checkTree_sound d (pairUp xs) h.2

/-- A specification-produced magnitude witness, not the implementation output. -/
def scaleWitness (v : BitVec 32) (s : BitVec 8) : BitVec 32 :=
  roundNE_f32 false (mulSigNat v (widenScale s)) (mulExpInt v (widenScale s))

/-- Decidable exact scaling, including exact subnormal values. -/
def checkScale (v : BitVec 32) (s : BitVec 8) : Bool :=
  decide (IsFiniteF32 v) && decide (s ≠ 255#8) &&
  (isZero v ||
    (decide (biasedFinal (mulSigNat v (widenScale s)) (mulExpInt v (widenScale s)) ≤ 254) &&
     decide (IsFiniteF32 (scaleWitness v s)) &&
     decide (f32MagScaled (scaleWitness v s) * 2 ^ tScale (mulExpInt v (widenScale s)) =
       mulSigNat v (widenScale s) * 2 ^ uScale (mulExpInt v (widenScale s)))))

theorem checkScale_sound (v : BitVec 32) (s : BitVec 8) (h : checkScale v s = true) :
    NoScaleOverflow v s := by
  simp only [checkScale, Bool.and_eq_true, Bool.or_eq_true, decide_eq_true_eq] at h
  refine ⟨h.1.1, h.1.2, ?_⟩
  rcases h.2 with hz | hr
  · exact Or.inl hz
  · exact Or.inr ⟨hr.1.1, scaleWitness v s, hr.1.2, hr.2⟩

/-- Executable admissibility check for equal-length, nonempty blocks of at most
32 elements. It checks the actual five-level tree and both scale steps. -/
def checkBlock (fmt : BlockElem) (sa sb : BitVec 8) (ea eb : List (BitVec 8)) : Bool :=
  let ps := archProducts fmt ea eb
  let total := archPairwiseSum ps
  decide (ea.length = eb.length) && decide (ea ≠ []) && decide (ea.length ≤ 32) &&
    (ea.zip eb).all (fun p => decide (FiniteElem fmt p.1 ∧ FiniteElem fmt p.2)) &&
    checkTree 5 ps && checkScale total sa && checkScale (arch_f32_mul total (widenScale sa)) sb


/-- Acceptance discharges the summation and scaling conditions for this concrete
block. This is a sound checker theorem, not universal acceptance of all blocks. -/
theorem checked_block_error_bound (fmt : BlockElem) (sa sb : BitVec 8)
    (ea eb : List (BitVec 8)) (h : checkBlock fmt sa sb ea eb = true) :
    ratAbs (f32ToRat (archScaledDot fmt sa sb ea eb) - exactBlockDot fmt sa sb ea eb)
      ≤ gammaPairwise 5 *
        ratSum ((archProducts fmt ea eb).map (fun p => ratAbs (f32ToRat p))) *
          f32ToRat (widenScale sa) * f32ToRat (widenScale sb) := by
  simp only [checkBlock, Bool.and_eq_true, decide_eq_true_eq] at h
  rcases h with ⟨⟨⟨⟨⟨⟨heq, hnon⟩, hlen0⟩, hfinite0⟩, htree⟩, hscalea⟩, hscaleb⟩
  have hfinite : ∀ p ∈ ea.zip eb, FiniteElem fmt p.1 ∧ FiniteElem fmt p.2 := by
    simpa only [List.all_eq_true, decide_eq_true_eq] using hfinite0
  have hlen : (archProducts fmt ea eb).length ≤ 2^5 := by
    simpa [archProducts, List.length_zip, heq] using hlen0
  have hne : archProducts fmt ea eb ≠ [] := by
    intro hz
    have hl := congrArg List.length hz
    have hlp : (archProducts fmt ea eb).length = ea.length := by
      simp [archProducts, List.length_zip, heq]
    change (archProducts fmt ea eb).length = 0 at hl
    have : ea.length > 0 := List.length_pos_iff.mpr hnon
    omega
  have hdu : ((5 : Rat) + 1) * f32u < 1 := by norm_num [f32u]
  have hb := gpair_bound_tree arch_f32_add f32R (by unfold f32u; positivity)
    5 (archProducts fmt ea eb) hne hlen hdu (checkTree_sound _ _ htree)
  rw [← archPairwiseSum_eq] at hb
  have hB : ratAbs (f32ToRat (archPairwiseSum (archProducts fmt ea eb)) -
        ratSum ((archProducts fmt ea eb).map f32ToRat)) ≤
      gammaPairwise 5 * ratSum ((archProducts fmt ea eb).map (fun p => ratAbs (f32ToRat p))) := by
    rw [ratAbs_abs]
    simpa only [gammaPairwise, agamma, ratSum, f32ToRat, ratAbs_abs, List.sum] using hb
  have hov1 := checkScale_sound _ _ hscalea
  have hov2 := checkScale_sound _ _ hscaleb
  have hsa : 0 ≤ f32ToRat (widenScale sa) := by
    change 0 ≤ f32R (widenScale sa)
    rw [scale_value sa hov1.2.1]; positivity
  have hsb : 0 ≤ f32ToRat (widenScale sb) := by
    change 0 ≤ f32R (widenScale sb)
    rw [scale_value sb hov2.2.1]; positivity
  have h1 := ratAbs_sub_mul_le
    (f32ToRat (archPairwiseSum (archProducts fmt ea eb)))
    (ratSum ((archProducts fmt ea eb).map f32ToRat))
    (gammaPairwise 5 * ratSum ((archProducts fmt ea eb).map (fun p => ratAbs (f32ToRat p))))
    (f32ToRat (widenScale sa)) hsa hB
  have h2 := ratAbs_sub_mul_le
    (f32ToRat (archPairwiseSum (archProducts fmt ea eb)) * f32ToRat (widenScale sa))
    (ratSum ((archProducts fmt ea eb).map f32ToRat) * f32ToRat (widenScale sa))
    (gammaPairwise 5 * ratSum ((archProducts fmt ea eb).map (fun p => ratAbs (f32ToRat p))) *
      f32ToRat (widenScale sa))
    (f32ToRat (widenScale sb)) hsb h1
  rw [archScaledDot_scale_pull fmt sa sb ea eb hov1 hov2, exactBlockDot,
      ← products_sum_exact fmt ea eb hfinite]
  exact h2

end ArchFp
