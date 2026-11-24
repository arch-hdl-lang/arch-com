import ArchFpEquiv.ArchRoundingBridge
import ArchFpEquiv.TreeRoundingBound
namespace ArchFp
set_option maxHeartbeats 4000000

/-- Nonfinite inputs cannot disappear into a finite add result. -/
theorem add_finite_inputs (a b : BitVec 32) (h : IsFiniteF32 (arch_f32_add a b)) :
    IsFiniteF32 a ∧ IsFiniteF32 b := by
  unfold IsFiniteF32 expField arch_f32_add at *
  bv_decide

/-- This includes infinity times zero: that result is NaN, never finite. -/
theorem mul_finite_inputs (a b : BitVec 32) (h : IsFiniteF32 (arch_f32_mul a b)) :
    IsFiniteF32 a ∧ IsFiniteF32 b := by
  unfold IsFiniteF32 expField arch_f32_mul at *
  bv_decide

def AllFinite (xs : List (BitVec 32)) : Prop := ∀ x ∈ xs, IsFiniteF32 x

theorem pair_finite_back : ∀ xs : List (BitVec 32),
    AllFinite (gpairUp arch_f32_add xs) → AllFinite xs
  | [], _ => by simp [AllFinite]
  | [a], h => h
  | a::b::rest, h => by
    have hab : IsFiniteF32 (arch_f32_add a b) := h _ (by simp [gpairUp])
    obtain ⟨ha,hb⟩ := add_finite_inputs a b hab
    have ht := pair_finite_back rest (by intro x hx; exact h x (by simp [gpairUp, hx]))
    intro x hx
    simp only [List.mem_cons] at hx
    rcases hx with rfl | rfl | hx
    · exact ha
    · exact hb
    · exact ht x hx

theorem pair_weighted_of_finite : ∀ xs : List (BitVec 32),
    AllFinite (gpairUp arch_f32_add xs) →
    WeightedPairs arch_f32_add f32R (fun x => |f32R x| + (1/2^126 : Rat)) f32u xs
  | [], _ => trivial
  | [_], _ => trivial
  | a::b::rest, h => by
    have hab : IsFiniteF32 (arch_f32_add a b) := h _ (by simp [gpairUp])
    obtain ⟨ha,hb⟩ := add_finite_inputs a b hab
    refine ⟨?_, pair_weighted_of_finite rest (by intro x hx; exact h x (by simp [gpairUp,hx]))⟩
    apply mixed_to_weighted _ _ _ f32u (1/2^126) (by norm_num [f32u]) (by positivity)
    convert arch_add_mixed a b ha hb hab using 1 <;> norm_num [f32u,halfEta]

/-- A finite final tree value forces every evaluated intermediate to be finite;
the local numerical bounds therefore follow from the ARCH rounding theorem. -/
theorem tree_weighted_of_finite : ∀ (d : Nat) (xs : List (BitVec 32)),
    xs ≠ [] → xs.length ≤ 2^d → IsFiniteF32 (gpairSum arch_f32_add xs) →
    AllFinite xs ∧ WeightedTree arch_f32_add f32R
      (fun x => |f32R x| + (1/2^126 : Rat)) f32u d xs := by
  intro d
  induction d with
  | zero =>
    intro xs hn hl hf
    match xs, hn, hl with
    | [a], _, _ => exact ⟨by simpa [AllFinite,gpairSum] using hf, trivial⟩
  | succ d ih =>
    intro xs hn hl hf
    match xs with
    | [a] =>
      have ha : IsFiniteF32 a := by simpa [gpairSum] using hf
      have hd := ih [a] (by simp) (by change 1 ≤ 2^d; have := Nat.two_pow_pos d; omega) (by simpa [gpairSum] using ha)
      exact ⟨by simpa [AllFinite] using ha, ⟨trivial, hd.2⟩⟩
    | a::b::rest =>
      have hlen : (gpairUp arch_f32_add (a::b::rest)).length ≤ 2^d := by
        rw [gpairUp_length]; simp only [List.length_cons] at hl ⊢; omega
      have hne : gpairUp arch_f32_add (a::b::rest) ≠ [] := by simp [gpairUp]
      rw [gpairSum] at hf
      have h := ih _ hne hlen hf
      exact ⟨pair_finite_back _ h.1, pair_weighted_of_finite _ h.1, h.2⟩


/-- The conservative absolute error budget, including both scale roundings. -/
noncomputable def mixedBlockAllowance (d : Nat) (ps : List (BitVec 32)) (X Y : Rat) : Rat :=
  (1+f32u)^2 * X*Y*agamma f32u d * (ps.map (fun p => |f32R p| + (1/2^126 : Rat))).sum
    + (2*f32u+f32u^2)*X*Y*|(ps.map f32R).sum|
    + halfEta*((1+f32u)*Y+1)

 theorem scaled_tree_mixed_of_finite (d : Nat) (ps : List (BitVec 32))
    (hne : ps ≠ []) (hlen : ps.length ≤ 2^d) (hdu : ((d : Rat)+1)*f32u < 1)
    (x y : BitVec 32) (hx : 0 ≤ f32R x) (hy : 0 ≤ f32R y)
    (hf : IsFiniteF32 (arch_f32_mul (arch_f32_mul (archPairwiseSum ps) x) y)) :
    |f32R (arch_f32_mul (arch_f32_mul (archPairwiseSum ps) x) y) -
      (ps.map f32R).sum * f32R x * f32R y| ≤ mixedBlockAllowance d ps (f32R x) (f32R y) := by
  obtain ⟨ht,hyf⟩ := mul_finite_inputs _ _ hf
  obtain ⟨hs,hxf⟩ := mul_finite_inputs _ _ ht
  have hs' := hs
  rw [archPairwiseSum_eq] at hs'
  have htree := (tree_weighted_of_finite d ps hne hlen hs').2
  have hsum := gpair_bound_weighted arch_f32_add f32R
    (fun p => |f32R p| + (1/2^126 : Rat)) (by norm_num [f32u])
    (by intro p; positivity) d ps hne hlen hdu htree
  rw [← archPairwiseSum_eq] at hsum
  have h1 := arch_mul_mixed (archPairwiseSum ps) x hs hxf ht
  have h2 := arch_mul_mixed (arch_f32_mul (archPairwiseSum ps) x) y ht hyf hf
  have h := two_rounded_scales_error _ _ _ _ _ _ _ _ _ (by norm_num [f32u]) hx hy hsum h1 h2
  unfold mixedBlockAllowance
  convert h using 1 <;> ring

/-- Universal concrete-block bound: no per-block check, exact-scale witness,
normal-result condition, or summation-safety premise. The final finite bit
classification alone selects the numerical-error branch. -/
theorem arch_block_mixed_bound (fmt : BlockElem) (sa sb : BitVec 8)
    (ea eb : List (BitVec 8)) (heq : ea.length = eb.length) (hne : ea ≠ [])
    (hlen : ea.length ≤ 32)
    (hfinite : ∀ p ∈ ea.zip eb, FiniteElem fmt p.1 ∧ FiniteElem fmt p.2)
    (hsa : sa ≠ 255#8) (hsb : sb ≠ 255#8)
    (hf : IsFiniteF32 (archScaledDot fmt sa sb ea eb)) :
    |f32R (archScaledDot fmt sa sb ea eb) - exactBlockDot fmt sa sb ea eb| ≤
      mixedBlockAllowance 5 (archProducts fmt ea eb) (f32R (widenScale sa)) (f32R (widenScale sb)) := by
  have hl : (archProducts fmt ea eb).length = ea.length := by simp [archProducts,List.length_zip,heq]
  have hn : archProducts fmt ea eb ≠ [] := by
    intro h; have := congrArg List.length h; rw [hl] at this; exact hne (by simpa using this)
  have hx : 0 ≤ f32R (widenScale sa) := by rw [scale_value sa hsa]; positivity
  have hy : 0 ≤ f32R (widenScale sb) := by rw [scale_value sb hsb]; positivity
  have h := scaled_tree_mixed_of_finite 5 (archProducts fmt ea eb) hn
    (by rw [hl]; exact hlen) (by norm_num [f32u]) (widenScale sa) (widenScale sb) hx hy hf
  have he : ((archProducts fmt ea eb).map f32R).sum * f32R (widenScale sa) * f32R (widenScale sb)
      = exactBlockDot fmt sa sb ea eb := by
    unfold exactBlockDot
    rw [← products_sum_exact fmt ea eb hfinite]
    rfl
  rw [he] at h
  exact h


theorem finite_add_not_nan (a b : BitVec 32) (ha : IsFiniteF32 a) (hb : IsFiniteF32 b) :
    isNaN (arch_f32_add a b) = false := by
  unfold IsFiniteF32 isNaN expField fracField arch_f32_add at *
  bv_decide

theorem finite_mul_not_nan (a b : BitVec 32) (ha : IsFiniteF32 a) (hb : IsFiniteF32 b) :
    isNaN (arch_f32_mul a b) = false := by
  unfold IsFiniteF32 isNaN expField fracField arch_f32_mul at *
  bv_decide

theorem nonfinite_cases (a : BitVec 32) (h : ¬ IsFiniteF32 a) :
    isInf a = true ∨ isNaN a = true := by
  unfold IsFiniteF32 isInf isNaN expField fracField at *
  bv_decide

theorem finite_add_overflow (a b : BitVec 32) (ha : IsFiniteF32 a) (hb : IsFiniteF32 b)
    (hr : ¬ IsFiniteF32 (arch_f32_add a b)) : isInf (arch_f32_add a b) = true := by
  rcases nonfinite_cases _ hr with h | h
  · exact h
  · rw [finite_add_not_nan a b ha hb] at h; contradiction

theorem finite_mul_overflow (a b : BitVec 32) (ha : IsFiniteF32 a) (hb : IsFiniteF32 b)
    (hr : ¬ IsFiniteF32 (arch_f32_mul a b)) : isInf (arch_f32_mul a b) = true := by
  rcases nonfinite_cases _ hr with h | h
  · exact h
  · rw [finite_mul_not_nan a b ha hb] at h; contradiction

/-- An actual pair of finite operands in this tree level overflowed to infinity. -/
def LevelOverflow : List (BitVec 32) → Prop
  | a::b::rest =>
    (IsFiniteF32 a ∧ IsFiniteF32 b ∧ isInf (arch_f32_add a b) = true) ∨ LevelOverflow rest
  | _ => False

/-- An overflow in the evaluated reduction tree. Later levels are visited only
when the previous level was entirely finite, giving a first-overflow witness. -/
def TreeOverflow : Nat → List (BitVec 32) → Prop
  | 0, _ => False
  | d+1, xs => LevelOverflow xs ∨
      (AllFinite (gpairUp arch_f32_add xs) ∧ TreeOverflow d (gpairUp arch_f32_add xs))

theorem level_progress : ∀ xs : List (BitVec 32), AllFinite xs →
    AllFinite (gpairUp arch_f32_add xs) ∨ LevelOverflow xs
  | [], _ => Or.inl (by simp [AllFinite,gpairUp])
  | [a], h => Or.inl h
  | a::b::rest, h => by
    have ha := h a (by simp)
    have hb := h b (by simp)
    by_cases hr : IsFiniteF32 (arch_f32_add a b)
    · have ht := level_progress rest (by intro x hx; exact h x (by simp [hx]))
      rcases ht with ht | ht
      · left; intro x hx
        simp only [gpairUp,List.mem_cons] at hx
        rcases hx with rfl | hx
        · exact hr
        · exact ht x hx
      · exact Or.inr (Or.inr ht)
    · exact Or.inr (Or.inl ⟨ha,hb,finite_add_overflow a b ha hb hr⟩)

theorem tree_progress : ∀ (d : Nat) (xs : List (BitVec 32)),
    xs ≠ [] → xs.length ≤ 2^d → AllFinite xs →
    IsFiniteF32 (gpairSum arch_f32_add xs) ∨ TreeOverflow d xs := by
  intro d
  induction d with
  | zero =>
    intro xs hn hl hf
    match xs, hn, hl with
    | [a], _, _ => exact Or.inl (by simpa [gpairSum] using hf a (by simp))
  | succ d ih =>
    intro xs hn hl hf
    match xs with
    | [a] => exact Or.inl (by simpa [gpairSum] using hf a (by simp))
    | a::b::rest =>
      rcases level_progress _ hf with hlevel | ho
      · have hlen : (gpairUp arch_f32_add (a::b::rest)).length ≤ 2^d := by
          rw [gpairUp_length]; simp only [List.length_cons] at hl ⊢; omega
        rcases ih _ (by simp [gpairUp]) hlen hlevel with ht | ho
        · exact Or.inl (by simpa only [gpairSum] using ht)
        · exact Or.inr (Or.inr ⟨hlevel,ho⟩)
      · exact Or.inr (Or.inl ho)

/-- Explicit stages where the first nonfinite result can arise. Invalid input
encodings are excluded by the public block theorem, not silently coerced to Rat. -/
def BlockOverflow (d : Nat) (ps : List (BitVec 32)) (x y : BitVec 32) : Prop :=
  TreeOverflow d ps ∨
  (IsFiniteF32 (archPairwiseSum ps) ∧ IsFiniteF32 x ∧
    isInf (arch_f32_mul (archPairwiseSum ps) x) = true) ∨
  (IsFiniteF32 (arch_f32_mul (archPairwiseSum ps) x) ∧ IsFiniteF32 y ∧
    isInf (arch_f32_mul (arch_f32_mul (archPairwiseSum ps) x) y) = true)

theorem scaled_tree_overflow (d : Nat) (ps : List (BitVec 32))
    (hne : ps ≠ []) (hlen : ps.length ≤ 2^d) (hp : AllFinite ps)
    (x y : BitVec 32) (hx : IsFiniteF32 x) (hy : IsFiniteF32 y)
    (hbad : ¬ IsFiniteF32 (arch_f32_mul (arch_f32_mul (archPairwiseSum ps) x) y)) :
    BlockOverflow d ps x y := by
  rcases tree_progress d ps hne hlen hp with hs | ho
  · rw [← archPairwiseSum_eq] at hs
    by_cases ht : IsFiniteF32 (arch_f32_mul (archPairwiseSum ps) x)
    · exact Or.inr (Or.inr ⟨ht,hy,finite_mul_overflow _ _ ht hy hbad⟩)
    · exact Or.inr (Or.inl ⟨hs,hx,finite_mul_overflow _ _ hs hx ht⟩)
  · exact Or.inl ho


theorem scale_finite (s : BitVec 8) (hs : s ≠ 255#8) : IsFiniteF32 (widenScale s) := by
  unfold IsFiniteF32 expField widenScale arch_e8m0_to_f32
  bv_decide

/-- Every valid finite-input block has either a finite result satisfying the
general mixed bound, or a nonfinite result with an actual finite-input overflow
witness in its reduction/scaling sequence. Infinity and NaN are classified
explicitly and never interpreted as rational values in the error branch. -/
theorem arch_block_result_cases (fmt : BlockElem) (sa sb : BitVec 8)
    (ea eb : List (BitVec 8)) (heq : ea.length = eb.length) (hne : ea ≠ [])
    (hlen : ea.length ≤ 32)
    (hfinite : ∀ p ∈ ea.zip eb, FiniteElem fmt p.1 ∧ FiniteElem fmt p.2)
    (hsa : sa ≠ 255#8) (hsb : sb ≠ 255#8) :
    (IsFiniteF32 (archScaledDot fmt sa sb ea eb) ∧
      |f32R (archScaledDot fmt sa sb ea eb) - exactBlockDot fmt sa sb ea eb| ≤
        mixedBlockAllowance 5 (archProducts fmt ea eb) (f32R (widenScale sa)) (f32R (widenScale sb))) ∨
    (¬ IsFiniteF32 (archScaledDot fmt sa sb ea eb) ∧
      BlockOverflow 5 (archProducts fmt ea eb) (widenScale sa) (widenScale sb) ∧
      (isInf (archScaledDot fmt sa sb ea eb) = true ∨ isNaN (archScaledDot fmt sa sb ea eb) = true)) := by
  by_cases hf : IsFiniteF32 (archScaledDot fmt sa sb ea eb)
  · exact Or.inl ⟨hf, arch_block_mixed_bound fmt sa sb ea eb heq hne hlen hfinite hsa hsb hf⟩
  · have hl : (archProducts fmt ea eb).length = ea.length := by
      simp [archProducts,List.length_zip,heq]
    have hn : archProducts fmt ea eb ≠ [] := by
      intro h; have := congrArg List.length h; rw [hl] at this; exact hne (by simpa using this)
    have hp : AllFinite (archProducts fmt ea eb) := by
      intro x hx
      obtain ⟨p,hp,rfl⟩ := List.mem_map.mp hx
      exact products_finite fmt p.1 p.2 (hfinite p hp).1 (hfinite p hp).2
    exact Or.inr ⟨hf, scaled_tree_overflow 5 _ hn (by rw [hl]; exact hlen) hp
      _ _ (scale_finite sa hsa) (scale_finite sb hsb) hf, nonfinite_cases _ hf⟩

end ArchFp
