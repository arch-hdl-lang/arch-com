import ArchCodegenProof.Netlist

/-!
# SystemVerilog subset: semantics of what `arch build` emits for a module

This is the **target-language specification** the codegen validator trusts.
It models the IEEE 1800-2017 rules that matter for the emitted subset:

* **Expression sizing (§11.6.1, Table 11-21).** Every operand of `+ - * &
  | ^ ~` and both arms of `?:` are *context-determined*: they are evaluated
  at the width of the enclosing context, which for an assignment is
  `max(width(lhs), self-width(rhs))`. Operands of comparisons are sized to
  the larger of the two operands; operands of `! && ||`, the condition of
  `?:` and the argument of `$unsigned` are self-determined.
* **Signedness (§11.8.1).** An expression is signed only if all of its
  context-determined operands are signed; a signed operand in an unsigned
  expression is reinterpreted as unsigned before extension. Identifiers
  declared `logic` are unsigned, unsized decimal literals are signed 32-bit,
  based literals without `s` are unsigned.
* **Extension (§11.8.2).** Operands narrower than the context are
  sign-extended if the expression is signed and zero-extended otherwise.
* **Size casts (§6.24.1).** `N'(e)` evaluates `e` as if assigned to an
  `N`-bit variable (context `max(N, self-width(e))`), keeps the low `N`
  bits and passes the signedness of `e` through.
* **Processes.** `assign` and `always_comb` are combinational processes;
  `always_ff @(posedge clk)` uses nonblocking assignments.

Signed comparisons are modelled (two's complement) but not yet supported by
the lowering; `Expr.supported` rules them out.
-/

namespace Arch.CodegenProof
namespace Sv

/-- Constant width expressions as they appear in size casts, e.g.
`(8 > $bits(t) ? 8 : $bits(t))'(acc + t)`. -/
inductive CW where
  | num (n : Nat)
  | bits (x : Sig)
  | sel (op : CmpOp) (a b t f : CW)
deriving DecidableEq, Repr

def CW.eval (W : Sig → Nat) : CW → Nat
  | .num n => n
  | .bits x => W x
  | .sel op a b t f => if op.apply (a.eval W) (b.eval W) then t.eval W else f.eval W

inductive Expr where
  | ident (x : Sig)
  /-- `size = none` is an unsized literal (32 bits). -/
  | lit (size : Option Nat) (signed : Bool) (n : Nat)
  | bin (op : BinOp) (a b : Expr)
  | bnot (a : Expr)
  | cmp (op : CmpOp) (a b : Expr)
  | lnot (a : Expr)
  | land (a b : Expr)
  | lor (a b : Expr)
  | cond (c a b : Expr)
  | cast (w : CW) (e : Expr)
  | unsigned (e : Expr)
  | slice (x : Sig) (hi lo : Nat)
deriving DecidableEq, Repr

def litWidth : Option Nat → Nat
  | none => 32
  | some w => w

section
variable (W : Sig → Nat)

/-- Self-determined width (Table 11-21). -/
def selfW : Expr → Nat
  | .ident x => W x
  | .lit sz _ _ => litWidth sz
  | .bin _ a b => max (selfW a) (selfW b)
  | .bnot a => selfW a
  | .cmp .. | .lnot _ | .land .. | .lor .. => 1
  | .cond _ a b => max (selfW a) (selfW b)
  | .cast w _ => w.eval W
  | .unsigned e => selfW e
  | .slice _ hi lo => hi + 1 - lo

end

/-- Self-determined signedness (§11.8.1). -/
def selfS : Expr → Bool
  | .ident _ => false
  | .lit _ s _ => s
  | .bin _ a b => selfS a && selfS b
  | .bnot a => selfS a
  | .cmp .. | .lnot _ | .land .. | .lor .. => false
  | .cond _ a b => selfS a && selfS b
  | .cast _ e => selfS e
  | .unsigned _ => false
  | .slice .. => false

/-- Extend a `src`-bit operand to the context width `w` (§11.8.2). -/
def ext (s : Bool) (src w v : Nat) : Nat := if s then sextVal w src v else mask w v

def toInt (w v : Nat) : Int := if 0 < w ∧ 2 ^ (w - 1) ≤ v then (v : Int) - 2 ^ w else v

def cmpInt : CmpOp → Int → Int → Bool
  | .eq, x, y => x == y
  | .ne, x, y => x != y
  | .lt, x, y => decide (x < y)
  | .le, x, y => decide (x ≤ y)
  | .gt, x, y => decide (x > y)
  | .ge, x, y => decide (x ≥ y)

section
variable (W : Sig → Nat) (σ : Env)

/-- Value of `e` evaluated in a context of width `w` and signedness `s`. -/
def evalCtx : Expr → Nat → Bool → Nat
  | .ident x, w, s => ext s (W x) w (mask (W x) (σ x))
  | .lit sz _ n, w, s => ext s (litWidth sz) w (mask (litWidth sz) n)
  | .bin op a b, w, s => op.apply w (evalCtx a w s) (evalCtx b w s)
  | .bnot a, w, s => notW w (evalCtx a w s)
  | .cmp op a b, w, s =>
    let cw := max (selfW W a) (selfW W b)
    let cs := selfS a && selfS b
    let x := evalCtx a cw cs
    let y := evalCtx b cw cs
    ext s 1 w (b2n (if cs then cmpInt op (toInt cw x) (toInt cw y) else op.apply x y))
  | .lnot a, w, s => ext s 1 w (b2n (evalCtx a (selfW W a) (selfS a) == 0))
  | .land a b, w, s =>
    ext s 1 w (b2n (evalCtx a (selfW W a) (selfS a) != 0 && evalCtx b (selfW W b) (selfS b) != 0))
  | .lor a b, w, s =>
    ext s 1 w (b2n (evalCtx a (selfW W a) (selfS a) != 0 || evalCtx b (selfW W b) (selfS b) != 0))
  | .cond c a b, w, s =>
    if evalCtx c (selfW W c) (selfS c) ≠ 0 then evalCtx a w s else evalCtx b w s
  | .cast cw e, w, s =>
    let n := cw.eval W
    ext s n w (mask n (evalCtx e (max n (selfW W e)) (selfS e)))
  | .unsigned e, w, s => ext s (selfW W e) w (evalCtx e (selfW W e) (selfS e))
  | .slice x hi lo, w, s => ext s (hi + 1 - lo) w (mask (hi + 1 - lo) (mask (W x) (σ x) / 2 ^ lo))

/-- Value stored by `x = e` / `x <= e`. -/
def assignVal (x : Sig) (e : Expr) : Nat :=
  mask (W x) (evalCtx W σ e (max (W x) (selfW W e)) (selfS e))

/-- Truth value of an `if (c)` condition (self-determined). -/
def condVal (c : Expr) : Nat := evalCtx W σ c (selfW W c) (selfS c)

end

inductive Stmts where
  | nil
  | assign (x : Sig) (e : Expr) (rest : Stmts)
  | ite (c : Expr) (t f : Stmts) (rest : Stmts)
deriving DecidableEq, Repr

section
variable (W : Sig → Nat)

/-- `always_comb` body: blocking assignments. -/
def execB : Stmts → Env → Env
  | .nil, σ => σ
  | .assign x e r, σ => execB r (upd σ x (assignVal W σ x e))
  | .ite c t f r, σ => execB r (if condVal W σ c ≠ 0 then execB t σ else execB f σ)

/-- `always_ff` body: nonblocking assignments read the pre-edge state. -/
def execN (pre : Env) : Stmts → Env → Env
  | .nil, acc => acc
  | .assign x e r, acc => execN pre r (upd acc x (assignVal W pre x e))
  | .ite c t f r, acc =>
    execN pre r (if condVal W pre c ≠ 0 then execN pre t acc else execN pre f acc)

end

/-- Variables written by an `always_ff` body. -/
def targets : Stmts → List Sig
  | .nil => []
  | .assign x _ r => x :: targets r
  | .ite _ t f r => targets t ++ targets f ++ targets r

inductive Proc where
  | assign (x : Sig) (e : Expr)
  | comb (body : Stmts)
deriving DecidableEq, Repr

def Proc.exec (W : Sig → Nat) (σ : Env) : Proc → Env
  | .assign x e => upd σ x (assignVal W σ x e)
  | .comb b => execB W b σ

structure Module where
  ports : List Port
  locals : List (Sig × Nat)
  procs : List Proc
  /-- Clock of the single `always_ff @(posedge clk)`, if any. -/
  ffClk : Option Sig
  ffBody : Stmts

namespace Module

def W (m : Module) : Sig → Nat := widthOf (declsOf m.ports m.locals)

/-- Trace semantics: every continuous process is settled in every cycle;
every variable written by `always_ff` takes its nonblocking next value at the
next cycle. Inputs are unconstrained. -/
def Run (m : Module) (tr : Nat → Env) : Prop :=
  ∀ t, (∀ p ∈ m.procs, ∀ y, p.exec m.W (tr t) y = tr t y) ∧
    ∀ r ∈ targets m.ffBody, tr (t + 1) r = execN m.W (tr t) m.ffBody (tr t) r

end Module

/-! ## Lowering to the sized IR -/

def Expr.supported : Expr → Bool
  | .ident _ | .lit .. | .slice .. => true
  | .bin _ a b | .land a b | .lor a b => a.supported && b.supported
  | .bnot a | .lnot a | .cast _ a | .unsigned a => a.supported
  | .cmp _ a b => !(selfS a && selfS b) && a.supported && b.supported
  | .cond c a b => c.supported && a.supported && b.supported

def Stmts.supported : Stmts → Bool
  | .nil => true
  | .assign _ e r => e.supported && r.supported
  | .ite c t f r => c.supported && t.supported && f.supported && r.supported

def Proc.supported : Proc → Bool
  | .assign _ e => e.supported
  | .comb b => b.supported

def Module.supported (m : Module) : Bool := m.procs.all Proc.supported && m.ffBody.supported

def extS (s : Bool) (src w : Nat) (t : Sir) : Sir := if s then .sext w src t else .resize w t

theorem eval_extS (σ : Env) (s : Bool) (src w : Nat) (t : Sir) :
    (extS s src w t).eval σ = ext s src w (t.eval σ) := by
  cases s <;> rfl

section
variable (W : Sig → Nat)

def lowerCtx : Expr → Nat → Bool → Sir
  | .ident x, w, s => extS s (W x) w (.var x (W x))
  | .lit sz _ n, w, s => extS s (litWidth sz) w (.resize (litWidth sz) (.lit n))
  | .bin op a b, w, s => .bin op w (lowerCtx a w s) (lowerCtx b w s)
  | .bnot a, w, s => .bnot w (lowerCtx a w s)
  | .cmp op a b, w, s =>
    let cw := max (selfW W a) (selfW W b)
    extS s 1 w (.cmp op (lowerCtx a cw false) (lowerCtx b cw false))
  | .lnot a, w, s => extS s 1 w (.lnot (lowerCtx a (selfW W a) (selfS a)))
  | .land a b, w, s =>
    extS s 1 w (.land (lowerCtx a (selfW W a) (selfS a)) (lowerCtx b (selfW W b) (selfS b)))
  | .lor a b, w, s =>
    extS s 1 w (.lor (lowerCtx a (selfW W a) (selfS a)) (lowerCtx b (selfW W b) (selfS b)))
  | .cond c a b, w, s => .mux w (lowerCtx c (selfW W c) (selfS c)) (lowerCtx a w s) (lowerCtx b w s)
  | .cast cw e, w, s =>
    let n := cw.eval W
    extS s n w (.resize n (lowerCtx e (max n (selfW W e)) (selfS e)))
  | .unsigned e, w, s => extS s (selfW W e) w (lowerCtx e (selfW W e) (selfS e))
  | .slice x hi lo, w, s => extS s (hi + 1 - lo) w (.slice hi lo (.var x (W x)))

theorem ext_lt (s : Bool) (src w v : Nat) : ext s src w v < 2 ^ w := by
  cases s
  · exact mask_lt _ _
  · exact Sir.sextVal_lt _ _ _

theorem evalCtx_lt (σ : Env) : ∀ (e : Expr) (w : Nat) (s : Bool), evalCtx W σ e w s < 2 ^ w
  | .ident .., _, _ | .lit .., _, _ | .cmp .., _, _ | .lnot _, _, _ | .land .., _, _
  | .lor .., _, _ | .cast .., _, _ | .unsigned _, _, _ | .slice .., _, _ => ext_lt _ _ _ _
  | .bin .., _, _ => Sir.binOp_lt _ _ _ _
  | .bnot _, _, _ => Sir.notW_lt _ _
  | .cond c a b, w, s => by
    simp only [evalCtx]; split
    · exact evalCtx_lt σ a w s
    · exact evalCtx_lt σ b w s

theorem eval_lowerCtx (σ : Env) : ∀ (e : Expr), e.supported = true →
    ∀ (w : Nat) (s : Bool), (lowerCtx W e w s).eval σ = evalCtx W σ e w s
  | .ident _, _, _, _ | .lit .., _, _, _ => by
    simp only [lowerCtx, evalCtx, eval_extS, Sir.eval]
  | .slice .., _, _, _ => by
    simp only [lowerCtx, evalCtx, eval_extS, Sir.eval]
  | .bin op a b, h, w, s => by
    simp only [Expr.supported, Bool.and_eq_true] at h
    simp only [lowerCtx, evalCtx, Sir.eval, eval_lowerCtx σ a h.1, eval_lowerCtx σ b h.2]
  | .bnot a, h, w, s => by
    simp only [Expr.supported] at h
    simp only [lowerCtx, evalCtx, Sir.eval, eval_lowerCtx σ a h]
  | .cmp op a b, h, w, s => by
    simp only [Expr.supported, Bool.and_eq_true, Bool.not_eq_eq_eq_not, Bool.not_true] at h
    obtain ⟨⟨hs, ha⟩, hb⟩ := h
    simp only [lowerCtx, evalCtx, eval_extS, Sir.eval, eval_lowerCtx σ a ha,
      eval_lowerCtx σ b hb, hs]
    rfl
  | .lnot a, h, w, s => by
    simp only [Expr.supported] at h
    simp only [lowerCtx, evalCtx, eval_extS, Sir.eval, eval_lowerCtx σ a h]
  | .land a b, h, w, s => by
    simp only [Expr.supported, Bool.and_eq_true] at h
    simp only [lowerCtx, evalCtx, eval_extS, Sir.eval, eval_lowerCtx σ a h.1,
      eval_lowerCtx σ b h.2]
  | .lor a b, h, w, s => by
    simp only [Expr.supported, Bool.and_eq_true] at h
    simp only [lowerCtx, evalCtx, eval_extS, Sir.eval, eval_lowerCtx σ a h.1,
      eval_lowerCtx σ b h.2]
  | .cond c a b, h, w, s => by
    simp only [Expr.supported, Bool.and_eq_true] at h
    obtain ⟨⟨hc, ha⟩, hb⟩ := h
    simp only [lowerCtx, evalCtx, Sir.eval, eval_lowerCtx σ c hc, eval_lowerCtx σ a ha,
      eval_lowerCtx σ b hb]
    split
    · exact mask_of_lt (evalCtx_lt W σ a w s)
    · exact mask_of_lt (evalCtx_lt W σ b w s)
  | .cast cw e, h, w, s => by
    simp only [Expr.supported] at h
    simp only [lowerCtx, evalCtx, eval_extS, Sir.eval, eval_lowerCtx σ e h]
  | .unsigned e, h, w, s => by
    simp only [Expr.supported] at h
    simp only [lowerCtx, evalCtx, eval_extS, eval_lowerCtx σ e h]

def lowerAssign (x : Sig) (e : Expr) : Sir :=
  .resize (W x) (lowerCtx W e (max (W x) (selfW W e)) (selfS e))

def lowerCond (c : Expr) : Sir := lowerCtx W c (selfW W c) (selfS c)

def lowerStmts : Stmts → Arch.CodegenProof.Stmts
  | .nil => .nil
  | .assign x e r => .assign x (lowerAssign W x e) (lowerStmts r)
  | .ite c t f r => .ite (lowerCond W c) (lowerStmts t) (lowerStmts f) (lowerStmts r)

theorem execB_lower : ∀ (s : Stmts), s.supported = true →
    ∀ σ, (lowerStmts W s).execB σ = execB W s σ
  | .nil, _, _ => rfl
  | .assign x e r, h, σ => by
    simp only [Stmts.supported, Bool.and_eq_true] at h
    simp only [lowerStmts, Arch.CodegenProof.Stmts.execB, execB, lowerAssign, assignVal,
      Sir.eval, eval_lowerCtx W σ e h.1, execB_lower r h.2]
  | .ite c t f r, h, σ => by
    simp only [Stmts.supported, Bool.and_eq_true] at h
    obtain ⟨⟨⟨hc, ht⟩, hf⟩, hr⟩ := h
    simp only [lowerStmts, Arch.CodegenProof.Stmts.execB, execB, lowerCond, condVal,
      eval_lowerCtx W σ c hc, execB_lower t ht, execB_lower f hf, execB_lower r hr]
    rfl

theorem execN_lower (pre : Env) : ∀ (s : Stmts), s.supported = true →
    ∀ acc, (lowerStmts W s).execN pre acc = execN W pre s acc
  | .nil, _, _ => rfl
  | .assign x e r, h, acc => by
    simp only [Stmts.supported, Bool.and_eq_true] at h
    simp only [lowerStmts, Arch.CodegenProof.Stmts.execN, execN, lowerAssign, assignVal,
      Sir.eval, eval_lowerCtx W pre e h.1, execN_lower pre r h.2]
  | .ite c t f r, h, acc => by
    simp only [Stmts.supported, Bool.and_eq_true] at h
    obtain ⟨⟨⟨hc, ht⟩, hf⟩, hr⟩ := h
    simp only [lowerStmts, Arch.CodegenProof.Stmts.execN, execN, lowerCond, condVal,
      eval_lowerCtx W pre c hc, execN_lower pre t ht, execN_lower pre f hf,
      execN_lower pre r hr]
    rfl

def Proc.lower : Proc → Arch.CodegenProof.Stmts
  | .assign x e => .assign x (lowerAssign W x e) .nil
  | .comb b => lowerStmts W b

theorem Proc.execB_lower (p : Proc) (h : p.supported = true) (σ : Env) :
    (Proc.lower W p).execB σ = p.exec W σ := by
  cases p with
  | assign x e =>
    simp only [Proc.supported] at h
    simp only [Proc.lower, Arch.CodegenProof.Stmts.execB, Proc.exec, lowerAssign, assignVal,
      Sir.eval, eval_lowerCtx W σ e h]
  | comb b => exact Sv.execB_lower W b h σ

end

namespace Module

def toNetlist (m : Module) : Netlist where
  combs := m.procs.map (Proc.lower m.W)
  seq := lowerStmts m.W m.ffBody
  regs := targets m.ffBody

theorem run_iff (m : Module) (h : m.supported = true) (tr : Nat → Env) :
    m.Run tr ↔ m.toNetlist.Run tr := by
  simp only [Module.supported, Bool.and_eq_true, List.all_eq_true] at h
  obtain ⟨hp, hff⟩ := h
  simp only [Run, Netlist.Run, AllStable, Arch.CodegenProof.Stmts.Stable, Netlist.edge,
    toNetlist, List.mem_map, forall_exists_index, and_imp, forall_apply_eq_imp_iff₂,
    execN_lower m.W _ m.ffBody hff]
  constructor
  · intro hr t
    refine ⟨fun p hpm y => ?_, (hr t).2⟩
    rw [Proc.execB_lower m.W p (hp p hpm)]; exact (hr t).1 p hpm y
  · intro hr t
    refine ⟨fun p hpm y => ?_, (hr t).2⟩
    rw [← Proc.execB_lower m.W p (hp p hpm)]; exact (hr t).1 p hpm y

end Module

end Sv
end Arch.CodegenProof
