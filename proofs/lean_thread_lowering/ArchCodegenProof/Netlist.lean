import ArchCodegenProof.Sir

/-!
# Statements and clocked netlists over the sized IR

A *netlist* is the common target of both lowerings: a set of combinational
processes plus one clocked process.

* A combinational process is a statement list executed with **blocking**
  semantics (each assignment is visible to later statements). A state `σ`
  is *stable* for the process when executing the process from `σ` changes
  nothing. This is the fixpoint reading of both SV `assign`/`always_comb`
  and ARCH `let`/`comb`: for acyclic logic it has exactly one solution per
  input/register valuation, and for cyclic logic both sides admit exactly
  the same solutions.
* The clocked process is executed with **nonblocking** semantics: every
  right-hand side and condition reads the pre-edge state and the last write
  to a register wins.

`Netlist.Run` is the trace semantics: a run assigns a full valuation to every
cycle such that every combinational process is stable and every register
takes its next-state value at the following cycle. Inputs are left
unconstrained, so a run quantifies over all input streams.
-/

namespace Arch.CodegenProof

def upd (σ : Env) (x : Sig) (v : Nat) : Env := fun y => if y = x then v else σ y

inductive Stmts where
  | nil
  | assign (x : Sig) (e : Sir) (rest : Stmts)
  | ite (c : Sir) (t f : Stmts) (rest : Stmts)
deriving DecidableEq, Repr

namespace Stmts

/-- Blocking execution (`always_comb`, ARCH `comb`). -/
def execB : Stmts → Env → Env
  | .nil, σ => σ
  | .assign x e r, σ => r.execB (upd σ x (e.eval σ))
  | .ite c t f r, σ => r.execB (if c.eval σ ≠ 0 then t.execB σ else f.execB σ)

/-- Nonblocking execution (`always_ff`, ARCH `seq`): reads see `pre`. -/
def execN (pre : Env) : Stmts → Env → Env
  | .nil, acc => acc
  | .assign x e r, acc => r.execN pre (upd acc x (e.eval pre))
  | .ite c t f r, acc =>
    r.execN pre (if c.eval pre ≠ 0 then t.execN pre acc else f.execN pre acc)

def norm : Stmts → Stmts
  | .nil => .nil
  | .assign x e r => .assign x e.norm r.norm
  | .ite c t f r => .ite c.norm t.norm f.norm r.norm

theorem execB_norm : ∀ (s : Stmts) (σ : Env), s.norm.execB σ = s.execB σ
  | .nil, _ => rfl
  | .assign x e r, σ => by simp only [norm, execB, Sir.eval_norm, execB_norm r]
  | .ite c t f r, σ => by simp only [norm, execB, Sir.eval_norm, execB_norm t, execB_norm f,
      execB_norm r]

theorem execN_norm (pre : Env) : ∀ (s : Stmts) (acc : Env), s.norm.execN pre acc = s.execN pre acc
  | .nil, _ => rfl
  | .assign x e r, acc => by simp only [norm, execN, Sir.eval_norm, execN_norm pre r]
  | .ite c t f r, acc => by simp only [norm, execN, Sir.eval_norm, execN_norm pre t,
      execN_norm pre f, execN_norm pre r]

/-- `σ` is a fixpoint of the combinational process `s`. -/
def Stable (s : Stmts) (σ : Env) : Prop := ∀ y, s.execB σ y = σ y

/-! ## Splitting straight-line processes

`codegen` emits an ARCH `comb` block made only of plain assignments as one
SV `assign` per statement. Splitting a straight-line process with distinct
targets into singleton processes preserves stability, so the validator
splits both sides before comparing. -/

def straight : Stmts → Option (List (Sig × Sir))
  | .nil => some []
  | .assign x e r => (straight r).map ((x, e) :: ·)
  | .ite .. => none

def ofStraight : List (Sig × Sir) → Stmts
  | [] => .nil
  | (x, e) :: l => .assign x e (ofStraight l)

theorem straight_eq : ∀ {s : Stmts} {l : List (Sig × Sir)}, s.straight = some l → s = ofStraight l
  | .nil, l, h => by simp [straight] at h; subst h; rfl
  | .assign x e r, l, h => by
    simp only [straight, Option.map_eq_some_iff] at h
    obtain ⟨l', hr, rfl⟩ := h
    simp [ofStraight, straight_eq hr]
  | .ite .., _, h => by simp [straight] at h

def memB (x : Sig) : List Sig → Bool
  | [] => false
  | y :: l => x == y || memB x l

def nodupB : List Sig → Bool
  | [] => true
  | x :: l => !memB x l && nodupB l

theorem memB_false {x : Sig} : ∀ {l : List Sig}, memB x l = false → x ∉ l
  | [], _ => by simp
  | y :: l, h => by
    simp only [memB, Bool.or_eq_false_iff, beq_eq_false_iff_ne] at h
    simp only [List.mem_cons, not_or]
    exact ⟨h.1, memB_false h.2⟩

def split (s : Stmts) : List Stmts :=
  match s.straight with
  | some l => if nodupB (l.map Prod.fst) then l.map (fun p => .assign p.1 p.2 .nil) else [s]
  | none => [s]

theorem execB_ofStraight_notin : ∀ (l : List (Sig × Sir)) (σ : Env) (y : Sig),
    y ∉ l.map Prod.fst → (ofStraight l).execB σ y = σ y
  | [], _, _, _ => rfl
  | (x, e) :: l, σ, y, h => by
    simp only [List.map_cons, List.mem_cons, not_or] at h
    simp only [ofStraight, execB]
    rw [execB_ofStraight_notin l _ y h.2]
    simp [upd, h.1]

theorem stable_ofStraight_iff : ∀ (l : List (Sig × Sir)) (σ : Env),
    nodupB (l.map Prod.fst) = true →
    ((ofStraight l).Stable σ ↔ ∀ p ∈ l, (Stmts.assign p.1 p.2 .nil).Stable σ)
  | [], σ, _ => by simp [Stable, ofStraight, execB]
  | (x, e) :: l, σ, hnd => by
    simp only [List.map_cons, nodupB, Bool.and_eq_true, Bool.not_eq_eq_eq_not,
      Bool.not_true] at hnd
    have hx : x ∉ l.map Prod.fst := memB_false hnd.1
    have single : (Stmts.assign x e .nil).Stable σ ↔ e.eval σ = σ x := by
      simp only [Stable, execB, upd]
      constructor
      · intro h; simpa using h x
      · intro h y; split <;> simp_all
    simp only [List.mem_cons, forall_eq_or_imp]
    rw [single]
    constructor
    · intro h
      have hxv : e.eval σ = σ x := by
        have := h x
        simp only [ofStraight, execB] at this
        rw [execB_ofStraight_notin l _ x hx] at this
        simpa [upd] using this
      have hupd : upd σ x (e.eval σ) = σ := by
        funext y; simp only [upd]; split <;> simp_all
      refine ⟨hxv, (stable_ofStraight_iff l σ hnd.2).1 ?_⟩
      intro y; have := h y; simp only [ofStraight, execB, hupd] at this; exact this
    · rintro ⟨hxv, hrest⟩ y
      have hupd : upd σ x (e.eval σ) = σ := by
        funext y; simp only [upd]; split <;> simp_all
      simp only [ofStraight, execB, hupd]
      exact (stable_ofStraight_iff l σ hnd.2).2 hrest y

theorem stable_split_iff (s : Stmts) (σ : Env) :
    (∀ p ∈ s.split, p.Stable σ) ↔ s.Stable σ := by
  unfold split
  split
  · next l hl =>
    split
    · next hnd =>
      rw [straight_eq hl, stable_ofStraight_iff l σ hnd]
      simp only [List.mem_map]
      constructor
      · intro h p hp; exact h _ ⟨p, hp, rfl⟩
      · rintro h _ ⟨p, hp, rfl⟩; exact h p hp
    · simp
  · simp

/-! ## Order-insensitive comparison of nonblocking blocks

Nonblocking assignments to *distinct* registers commute: every right-hand
side reads the pre-edge state. `codegen` is free to order the reset branch
(or any straight-line run of `<=`) differently from the source, so the
validator compares such runs as sets. -/

def lookup (y : Sig) : List (Sig × Sir) → Option Sir
  | [] => none
  | (x, e) :: l => if y = x then some e else lookup y l

theorem execN_ofStraight (pre : Env) : ∀ (l : List (Sig × Sir)) (acc : Env) (y : Sig),
    nodupB (l.map Prod.fst) = true →
    (ofStraight l).execN pre acc y = match lookup y l with
      | some e => e.eval pre
      | none => acc y
  | [], _, _, _ => rfl
  | (x, e) :: l, acc, y, hnd => by
    simp only [List.map_cons, nodupB, Bool.and_eq_true, Bool.not_eq_eq_eq_not,
      Bool.not_true] at hnd
    have hx : x ∉ l.map Prod.fst := memB_false hnd.1
    simp only [ofStraight, execN, lookup]
    rw [execN_ofStraight pre l _ y hnd.2]
    by_cases hy : y = x
    · subst hy
      have : lookup y l = none := by
        clear hnd
        induction l with
        | nil => rfl
        | cons p l ih =>
          obtain ⟨px, pe⟩ := p
          simp only [List.map_cons, List.mem_cons, not_or] at hx
          simp only [lookup, if_neg hx.1]
          exact ih hx.2
      simp [this, upd]
    · simp only [if_neg hy]
      split <;> simp_all [upd]

theorem lookup_mem : ∀ {l : List (Sig × Sir)} {y : Sig} {e : Sir},
    lookup y l = some e → (y, e) ∈ l
  | [], _, _, h => by simp [lookup] at h
  | (x, e') :: l, y, e, h => by
    simp only [lookup] at h
    split at h
    · next hy => cases h; subst hy; exact List.mem_cons_self
    · exact List.mem_cons_of_mem _ (lookup_mem h)

theorem lookup_of_mem : ∀ {l : List (Sig × Sir)} {y : Sig} {e : Sir},
    nodupB (l.map Prod.fst) = true → (y, e) ∈ l → lookup y l = some e
  | [], _, _, _, h => by simp at h
  | (x, e') :: l, y, e, hnd, h => by
    simp only [List.map_cons, nodupB, Bool.and_eq_true, Bool.not_eq_eq_eq_not,
      Bool.not_true] at hnd
    have hx : x ∉ l.map Prod.fst := memB_false hnd.1
    simp only [lookup]
    rcases List.mem_cons.1 h with h0 | h1
    · simp only [Prod.mk.injEq] at h0; obtain ⟨rfl, rfl⟩ := h0; simp
    · have hyx : y ≠ x := fun hyx => hx (List.mem_map.2 ⟨(y, e), h1, hyx⟩)
      rw [if_neg hyx]; exact lookup_of_mem hnd.2 h1

def pairSubB (a b : List (Sig × Sir)) : Bool := a.all (fun p => b.any (fun q => decide (p = q)))

theorem pairSubB_sound {a b : List (Sig × Sir)} (h : pairSubB a b = true) : ∀ p ∈ a, p ∈ b := by
  intro p hp
  simp only [pairSubB, List.all_eq_true, List.any_eq_true, decide_eq_true_eq] at h
  obtain ⟨q, hq, rfl⟩ := h p hp
  exact hq

def straightEqv (a b : Stmts) : Bool :=
  match a.straight, b.straight with
  | some la, some lb =>
    nodupB (la.map Prod.fst) && nodupB (lb.map Prod.fst) && pairSubB la lb && pairSubB lb la
  | _, _ => false

theorem straightEqv_sound {a b : Stmts} (h : straightEqv a b = true) (pre acc : Env) :
    a.execN pre acc = b.execN pre acc := by
  unfold straightEqv at h
  split at h
  · next la lb ha hb =>
    simp only [Bool.and_eq_true] at h
    obtain ⟨⟨⟨hna, hnb⟩, hab⟩, hba⟩ := h
    rw [straight_eq ha, straight_eq hb]
    funext y
    rw [execN_ofStraight pre la acc y hna, execN_ofStraight pre lb acc y hnb]
    have : lookup y la = lookup y lb := by
      cases hl : lookup y la with
      | some e => exact (lookup_of_mem hnb (pairSubB_sound hab _ (lookup_mem hl))).symm
      | none =>
        cases hr : lookup y lb with
        | none => rfl
        | some e =>
          have := lookup_of_mem hna (pairSubB_sound hba _ (lookup_mem hr))
          rw [hl] at this; exact absurd this (by simp)
    rw [this]
  · simp at h

/-- Sufficient condition for two clocked processes to have equal effect. -/
def eqvN : Stmts → Stmts → Bool
  | .nil, .nil => true
  | .assign x e r, .assign x' e' r' =>
    straightEqv (.assign x e r) (.assign x' e' r') || (decide (x = x' ∧ e = e') && eqvN r r')
  | .ite c t f r, .ite c' t' f' r' => decide (c = c') && eqvN t t' && eqvN f f' && eqvN r r'
  | a, b => straightEqv a b

theorem eqvN_sound : ∀ {a b : Stmts}, eqvN a b = true → ∀ pre acc, a.execN pre acc = b.execN pre acc
  | .nil, .nil, _, _, _ => rfl
  | .assign x e r, .assign x' e' r', h, pre, acc => by
    simp only [eqvN, Bool.or_eq_true, Bool.and_eq_true, decide_eq_true_eq] at h
    rcases h with h | ⟨⟨rfl, rfl⟩, hr⟩
    · exact straightEqv_sound h pre acc
    · simp only [execN]; exact eqvN_sound hr pre _
  | .ite c t f r, .ite c' t' f' r', h, pre, acc => by
    simp only [eqvN, Bool.and_eq_true, decide_eq_true_eq] at h
    obtain ⟨⟨⟨rfl, ht⟩, hf⟩, hr⟩ := h
    simp only [execN]
    rw [eqvN_sound ht pre acc, eqvN_sound hf pre acc]
    exact eqvN_sound hr pre _
  | .nil, .assign .., h, pre, acc | .nil, .ite .., h, pre, acc
  | .assign .., .nil, h, pre, acc | .assign .., .ite .., h, pre, acc
  | .ite .., .nil, h, pre, acc | .ite .., .assign .., h, pre, acc => by
    simp only [eqvN] at h
    exact straightEqv_sound h pre acc

end Stmts

/-- All processes in the list are stable at `σ`. -/
def AllStable (ps : List Stmts) (σ : Env) : Prop := ∀ p ∈ ps, p.Stable σ

theorem allStable_flatMap_split (ps : List Stmts) (σ : Env) :
    AllStable (ps.flatMap Stmts.split) σ ↔ AllStable ps σ := by
  simp only [AllStable, List.mem_flatMap]
  constructor
  · intro h p hp
    exact (Stmts.stable_split_iff p σ).1 (fun q hq => h q ⟨p, hp, hq⟩)
  · rintro h q ⟨p, hp, hq⟩
    exact (Stmts.stable_split_iff p σ).2 (h p hp) q hq

theorem allStable_map_norm (ps : List Stmts) (σ : Env) :
    AllStable (ps.map Stmts.norm) σ ↔ AllStable ps σ := by
  simp [AllStable, Stmts.Stable, Stmts.execB_norm]

structure Netlist where
  combs : List Stmts
  seq : Stmts
  regs : List Sig

namespace Netlist

/-- Register values after one rising clock edge from the settled state `σ`. -/
def edge (n : Netlist) (σ : Env) : Env := n.seq.execN σ σ

def Run (n : Netlist) (tr : Nat → Env) : Prop :=
  ∀ t, AllStable n.combs (tr t) ∧ ∀ r ∈ n.regs, tr (t + 1) r = n.edge (tr t) r

/-- Canonical form compared by the validator: normalized expressions,
straight-line processes split into single assignments. -/
def canonCombs (n : Netlist) : List Stmts := (n.combs.map Stmts.norm).flatMap Stmts.split

def listSubB (a b : List Stmts) : Bool := a.all (fun p => b.any (fun q => decide (p = q)))

theorem listSubB_sound {a b : List Stmts} (h : listSubB a b = true) : ∀ p ∈ a, p ∈ b := by
  intro p hp
  simp only [listSubB, List.all_eq_true, List.any_eq_true, decide_eq_true_eq] at h
  obtain ⟨q, hq, rfl⟩ := h p hp
  exact hq

def regSubB (a b : List Sig) : Bool := a.all (fun x => Stmts.memB x b)

theorem memB_true {x : Sig} : ∀ {l : List Sig}, Stmts.memB x l = true → x ∈ l
  | [], h => by simp [Stmts.memB] at h
  | y :: l, h => by
    simp only [Stmts.memB, Bool.or_eq_true, beq_iff_eq] at h
    rcases h with h | h
    · simp [h]
    · exact List.mem_cons_of_mem _ (memB_true h)

theorem regSubB_sound {a b : List Sig} (h : regSubB a b = true) : ∀ x ∈ a, x ∈ b := by
  intro x hx
  simp only [regSubB, List.all_eq_true] at h
  exact memB_true (h x hx)

/-- Decidable sufficient condition for two netlists to have the same runs. -/
def sameB (a b : Netlist) : Bool :=
  listSubB a.canonCombs b.canonCombs && listSubB b.canonCombs a.canonCombs &&
  Stmts.eqvN a.seq.norm b.seq.norm && regSubB a.regs b.regs && regSubB b.regs a.regs

theorem allStable_canon (n : Netlist) (σ : Env) :
    AllStable n.canonCombs σ ↔ AllStable n.combs σ := by
  unfold canonCombs; rw [allStable_flatMap_split, allStable_map_norm]

theorem edge_eq_of_eqvN {a b : Netlist} (h : Stmts.eqvN a.seq.norm b.seq.norm = true)
    (σ : Env) : a.edge σ = b.edge σ := by
  unfold edge
  rw [← Stmts.execN_norm, Stmts.eqvN_sound h, Stmts.execN_norm]

theorem run_iff_of_sameB {a b : Netlist} (h : sameB a b = true) (tr : Nat → Env) :
    a.Run tr ↔ b.Run tr := by
  simp only [sameB, Bool.and_eq_true] at h
  obtain ⟨⟨⟨⟨hab, hba⟩, hseq⟩, hrab⟩, hrba⟩ := h
  have hcomb : ∀ σ, AllStable a.combs σ ↔ AllStable b.combs σ := by
    intro σ
    rw [← allStable_canon, ← allStable_canon]
    constructor
    · intro hs p hp; exact hs p (listSubB_sound hba p hp)
    · intro hs p hp; exact hs p (listSubB_sound hab p hp)
  have hregs : ∀ r, r ∈ a.regs ↔ r ∈ b.regs :=
    fun r => ⟨regSubB_sound hrab r, regSubB_sound hrba r⟩
  simp only [Run, hcomb, hregs, edge_eq_of_eqvN hseq]

end Netlist

end Arch.CodegenProof

namespace Arch.CodegenProof

/-! ## Interface declarations shared by both languages -/

inductive Dir where
  | input | output
deriving DecidableEq, Repr

structure Port where
  sig : Sig
  dir : Dir
  width : Nat
deriving DecidableEq, Repr

/-- Declared width of `x`; `0` for an undeclared signal (which then always
reads as `0` on both sides). -/
def widthOf : List (Sig × Nat) → Sig → Nat
  | [], _ => 0
  | (y, w) :: l, x => if x = y then w else widthOf l x

def declsOf (ports : List Port) (locals : List (Sig × Nat)) : List (Sig × Nat) :=
  ports.map (fun p => (p.sig, p.width)) ++ locals

end Arch.CodegenProof
