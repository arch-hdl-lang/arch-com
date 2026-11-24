import ArchFpEquiv.BlockRoundingBound

namespace ArchFp
section
variable {α : Type} (op : α → α → α) (v w : α → Rat)

/-- A weight pays for both relative rounding and an additive underflow term. -/
def WeightedPairs (u : Rat) : List α → Prop
  | a :: b :: rest =>
      (|v (op a b) - (v a + v b)| ≤ u * (w a + w b) ∧
        w (op a b) ≤ (1 + u) * (w a + w b)) ∧ WeightedPairs u rest
  | _ => True

def WeightedTree (u : Rat) : Nat → List α → Prop
  | 0, _ => True
  | d + 1, xs => WeightedPairs op v w u xs ∧ WeightedTree u d (gpairUp op xs)

theorem weighted_level_error {u : Rat} (hu : 0 ≤ u) (hw : ∀ x, 0 ≤ w x) :
    ∀ xs : List α, WeightedPairs op v w u xs →
      |((gpairUp op xs).map v).sum - (xs.map v).sum| ≤ u * (xs.map w).sum
  | [], _ => by simp [gpairUp]
  | [a], _ => by simp [gpairUp]; exact mul_nonneg hu (hw a)
  | a :: b :: rest, h => by
    have ih := weighted_level_error hu hw rest h.2
    simp only [gpairUp, List.map_cons, List.sum_cons]
    calc |v (op a b) + ((gpairUp op rest).map v).sum - (v a + (v b + (rest.map v).sum))|
        = |(v (op a b) - (v a + v b)) + (((gpairUp op rest).map v).sum - (rest.map v).sum)| := by ring_nf
      _ ≤ |v (op a b) - (v a + v b)| + |((gpairUp op rest).map v).sum - (rest.map v).sum| := abs_add_le _ _
      _ ≤ u * (w a + w b) + u * (rest.map w).sum := add_le_add h.1.1 ih
      _ = u * (w a + (w b + (rest.map w).sum)) := by ring

theorem weighted_level_growth {u : Rat} (hu : 0 ≤ u) (hw : ∀ x, 0 ≤ w x) :
    ∀ xs : List α, WeightedPairs op v w u xs →
      ((gpairUp op xs).map w).sum ≤ (1 + u) * (xs.map w).sum
  | [], _ => by simp [gpairUp]
  | [a], _ => by simp [gpairUp]; nlinarith [hw a]
  | a :: b :: rest, h => by
    have ih := weighted_level_growth hu hw rest h.2
    simp only [gpairUp, List.map_cons, List.sum_cons]
    calc w (op a b) + ((gpairUp op rest).map w).sum
        ≤ (1 + u) * (w a + w b) + (1 + u) * (rest.map w).sum := add_le_add h.1.2 ih
      _ = (1 + u) * (w a + (w b + (rest.map w).sum)) := by ring

variable [Inhabited α]

theorem gpair_bound_weighted {u : Rat} (hu : 0 < u) (hw : ∀ x, 0 ≤ w x)
    :
    ∀ (d : Nat) (xs : List α), xs ≠ [] → xs.length ≤ 2 ^ d → ((d:Rat) + 1) * u < 1 → WeightedTree op v w u d xs →
      |v (gpairSum op xs) - (xs.map v).sum| ≤ agamma u d * (xs.map w).sum := by
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
        List.sum_nil, add_zero, sub_self, abs_zero]; exact mul_nonneg hgnn1 (hw x)
    | a :: b :: rest =>
      have hqlen : (gpairUp op (a::b::rest)).length ≤ 2 ^ d := by
        rw [gpairUp_length]; simp only [List.length_cons] at hlen ⊢; omega
      have hqne : gpairUp op (a::b::rest) ≠ [] :=
        List.ne_nil_of_length_pos (by rw [gpairUp_length]; simp only [List.length_cons]; omega)
      have hih := IH (gpairUp op (a::b::rest)) hqne hqlen hduD htree.2
      have herr := weighted_level_error op v w (le_of_lt hu) hw (a::b::rest) htree.1
      have hgrow := weighted_level_growth op v w (le_of_lt hu) hw (a::b::rest) htree.1
      rw [show gpairSum op (a::b::rest) = gpairSum op (gpairUp op (a::b::rest)) by rw [gpairSum]]
      calc |v (gpairSum op (gpairUp op (a::b::rest))) - ((a::b::rest).map v).sum|
          ≤ |v (gpairSum op (gpairUp op (a::b::rest))) - ((gpairUp op (a::b::rest)).map v).sum|
              + |((gpairUp op (a::b::rest)).map v).sum - ((a::b::rest).map v).sum| := abs_sub_le _ _ _
        _ ≤ agamma u d * ((gpairUp op (a::b::rest)).map w).sum
              + u * ((a::b::rest).map w).sum := by gcongr
        _ ≤ agamma u d * ((1 + u) * ((a::b::rest).map w).sum)
              + u * ((a::b::rest).map w).sum := by gcongr
        _ = ((1 + u) * agamma u d + u) * ((a::b::rest).map w).sum := by ring
        _ ≤ agamma u (d + 1) * ((a::b::rest).map w).sum := by
            gcongr
            · exact List.sum_nonneg (by intro z hz; obtain ⟨x, _, rfl⟩ := List.mem_map.mp hz; exact hw x)
            · exact agamma_step u hu d hduD

end

/-- Adding k = eta/u to each magnitude turns a mixed rounding model into
weighted relative bounds. This uses only inequalities, not input enumeration. -/
theorem mixed_to_weighted (a b r u k : Rat) (hu : 0 ≤ u) (hk : 0 ≤ k)
    (h : |r - (a + b)| ≤ u * |a + b| + u*k) :
    |r - (a + b)| ≤ u * ((|a| + k) + (|b| + k)) ∧
    |r| + k ≤ (1 + u) * ((|a| + k) + (|b| + k)) := by
  have hab := abs_add_le a b
  have hweight : u * |a + b| ≤ u * (|a| + |b|) := mul_le_mul_of_nonneg_left hab hu
  have hr := abs_sub_abs_le_abs_sub r (a+b)
  constructor <;> nlinarith [mul_nonneg hu hk]


/-- A uniform local mixed-error law implies every reduction tree's weighted
contract. No block enumeration or run-time checker is used. -/
theorem weighted_tree_of_mixed {α : Type} (op : α → α → α) (v : α → Rat)
    (u k : Rat) (hu : 0 ≤ u) (hk : 0 ≤ k)
    (localLaw : ∀ a b, |v (op a b) - (v a + v b)| ≤ u * |v a + v b| + u*k)
    (d : Nat) (xs : List α) :
    WeightedTree op v (fun x => |v x| + k) u d xs := by
  have pairs : ∀ ys, WeightedPairs op v (fun x => |v x| + k) u ys := by
    intro ys
    induction ys using List.twoStepInduction with
    | nil => trivial
    | singleton a => trivial
    | cons_cons a b rest ih =>
      exact ⟨mixed_to_weighted (v a) (v b) (v (op a b)) u k hu hk (localLaw a b), ih⟩
  induction d generalizing xs with
  | zero => trivial
  | succ d ih => exact ⟨pairs xs, ih _⟩

theorem magnitude_weight_sum {α : Type} (v : α → Rat) (k : Rat) (xs : List α) :
    (xs.map (fun x => |v x|+k)).sum = (xs.map (fun x => |v x|)).sum + xs.length*k := by
  induction xs with
  | nil => simp
  | cons x xs ih => simp only [List.map_cons, List.sum_cons, List.length_cons, Nat.cast_add,
      Nat.cast_one, ih]; ring

/-- General pairwise reduction followed by two rounded nonnegative scales.
This is a numerical-model theorem: `localLaw`, `ht`, and `hq` must be supplied
by an implementation refinement for successful finite stages. -/
theorem general_scaled_tree_bound {α : Type} [Inhabited α]
    (op : α → α → α) (v : α → Rat) (u k : Rat)
    (hu : 0 < u) (hk : 0 ≤ k)
    (localLaw : ∀ a b, |v (op a b) - (v a + v b)| ≤ u * |v a + v b| + u*k)
    (d : Nat) (xs : List α) (hne : xs ≠ []) (hlen : xs.length ≤ 2^d)
    (hdu : ((d : Rat) + 1)*u < 1)
    (X Y t q : Rat) (hX : 0 ≤ X) (hY : 0 ≤ Y)
    (ht : |t - v (gpairSum op xs)*X| ≤ u*|v (gpairSum op xs)*X| + u*k)
    (hq : |q - t*Y| ≤ u*|t*Y| + u*k) :
    |q - (xs.map v).sum * X * Y| ≤
      (1+u)^2*X*Y*agamma u d * (xs.map (fun x => |v x|+k)).sum
      + (2*u+u^2)*X*Y*|(xs.map v).sum|
      + u*k*((1+u)*Y+1) := by
  have tree := weighted_tree_of_mixed op v u k (le_of_lt hu) hk localLaw d xs
  have hsum := gpair_bound_weighted op v (fun x => |v x|+k) hu
    (fun x => add_nonneg (abs_nonneg _) hk) d xs hne hlen hdu tree
  have h := two_rounded_scales_error (v (gpairSum op xs)) (xs.map v).sum
    t q X Y u (u*k) (agamma u d * (xs.map (fun x => |v x|+k)).sum)
    (le_of_lt hu) hX hY hsum ht hq
  convert h using 1 <;> ring

end ArchFp
