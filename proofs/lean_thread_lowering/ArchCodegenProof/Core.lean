import ArchCodegenProof.Netlist

/-!
# ARCH-Core: normative semantics of the module subset

`Core.Module` is the elaborated, thread-free form of an ARCH `module` that
the codegen validator currently covers: `UInt<N>`/`Bool` ports, `let`,
`wire` + `comb`, and registers with a synchronous or asynchronous reset in one
`seq on clk rising` block.

This file is the **specification**: it states what an ARCH module means,
independently of any SystemVerilog. Width rules follow
`doc/ARCH_HDL_Specification.md` §"Arithmetic widths" (IEEE 1800 §11.6
style):

| ARCH                    | result width              | value                     |
|-------------------------|---------------------------|---------------------------|
| `a + b`                 | `max(W a, W b) + 1`       | exact sum                 |
| `a - b`                 | `max(W a, W b) + 1`       | difference mod `2^width`  |
| `a * b`                 | `W a + W b`               | exact product             |
| `a +% b`, `-%`, `*%`    | `max(W a, W b)`           | wraps mod `2^width`       |
| `a & b`, `\|`, `^`      | `max(W a, W b)`           | bitwise                   |
| `~a`                    | `W a`                     | complement                |
| comparisons, `and/or/not` | 1                       | `0`/`1`                   |
| `c ? a : b`             | `max(W a, W b)`           |                           |
| `.trunc/.zext/.resize<N>` | `N`                     | low `N` bits (unsigned)   |
| `a[hi:lo]`              | `hi - lo + 1`             | bit field                 |

Assignments store the right-hand side truncated or zero-extended to the
target's declared width; the ARCH type checker guarantees the "truncated"
case never arises in accepted programs.

The widening-`-` row is the one place the written spec is silent on the
value; the type checker gives the width and every backend wraps.
-/

namespace Arch.CodegenProof
namespace Core

inductive Expr where
  | sig (x : Sig)
  | lit (w n : Nat)
  | bool (b : Bool)
  | add (a b : Expr)
  | sub (a b : Expr)
  | mul (a b : Expr)
  | binW (op : BinOp) (a b : Expr)
  | bnot (a : Expr)
  | cmp (op : CmpOp) (a b : Expr)
  | lnot (a : Expr)
  | land (a b : Expr)
  | lor (a b : Expr)
  | mux (c a b : Expr)
  | resize (n : Nat) (a : Expr)
  | slice (hi lo : Nat) (a : Expr)
deriving DecidableEq, Repr

section
variable (W : Sig → Nat)

def width : Expr → Nat
  | .sig x => W x
  | .lit w _ => w
  | .bool _ => 1
  | .add a b | .sub a b => max (width a) (width b) + 1
  | .mul a b => width a + width b
  | .binW _ a b => max (width a) (width b)
  | .bnot a => width a
  | .cmp .. | .lnot _ | .land .. | .lor .. => 1
  | .mux _ a b => max (width a) (width b)
  | .resize n _ => n
  | .slice hi lo _ => hi + 1 - lo

def eval (σ : Env) : Expr → Nat
  | .sig x => mask (W x) (σ x)
  | .lit w n => mask w n
  | .bool b => b2n b
  | .add a b => mask (max (width W a) (width W b) + 1) (eval σ a + eval σ b)
  | .sub a b => subW (max (width W a) (width W b) + 1) (eval σ a) (eval σ b)
  | .mul a b => mask (width W a + width W b) (eval σ a * eval σ b)
  | .binW op a b => op.apply (max (width W a) (width W b)) (eval σ a) (eval σ b)
  | .bnot a => notW (width W a) (eval σ a)
  | .cmp op a b => b2n (op.apply (eval σ a) (eval σ b))
  | .lnot a => b2n (eval σ a == 0)
  | .land a b => b2n (eval σ a != 0 && eval σ b != 0)
  | .lor a b => b2n (eval σ a != 0 || eval σ b != 0)
  | .mux c a b =>
    let w := max (width W a) (width W b)
    if eval σ c ≠ 0 then mask w (eval σ a) else mask w (eval σ b)
  | .resize n a => mask n (eval σ a)
  | .slice hi lo a => mask (hi + 1 - lo) (eval σ a / 2 ^ lo)

end

inductive Stmts where
  | nil
  | assign (x : Sig) (e : Expr) (rest : Stmts)
  | ite (c : Expr) (t f : Stmts) (rest : Stmts)
deriving DecidableEq, Repr

section
variable (W : Sig → Nat)

/-- `comb` blocks: blocking, in statement order. -/
def execB : Stmts → Env → Env
  | .nil, σ => σ
  | .assign x e r, σ => execB r (upd σ x (mask (W x) (eval W σ e)))
  | .ite c t f r, σ => execB r (if eval W σ c ≠ 0 then execB t σ else execB f σ)

/-- `seq` blocks: nonblocking — reads see the pre-edge state `pre`. -/
def execN (pre : Env) : Stmts → Env → Env
  | .nil, acc => acc
  | .assign x e r, acc => execN pre r (upd acc x (mask (W x) (eval W pre e)))
  | .ite c t f r, acc =>
    execN pre r (if eval W pre c ≠ 0 then execN pre t acc else execN pre f acc)

def applyResets (pre : Env) : List (Sig × Expr) → Env → Env
  | [], acc => acc
  | (r, v) :: l, acc => applyResets pre l (upd acc r (mask (W r) (eval W pre v)))

end

structure Module where
  ports : List Port
  /-- `reg`, `wire` and `let` declarations with their widths. -/
  locals : List (Sig × Nat)
  /-- Clock of the `seq block`; `none` for a purely combinational module. -/
  clk : Option Sig
  /-- Reset `(signal, active-high?)` shared by every register. -/
  rst : Option (Sig × Bool)
  /-- `Reset<Async, _>`: the reset also acts without a clock edge. -/
  rstAsync : Bool := false
  /-- Registers of the `seq` block with their reset values. -/
  regs : List (Sig × Expr)
  /-- `reg r: T init V`: the register's value at power-up (time 0). -/
  inits : List (Sig × Expr) := []
  /-- One process per `let` and per `comb` block. -/
  combs : List Stmts
  /-- Body of `seq on clk rising`. -/
  seq : Stmts

namespace Module

def W (m : Module) : Sig → Nat := widthOf (declsOf m.ports m.locals)

def rstActive (W : Sig → Nat) (r : Sig) (high : Bool) (σ : Env) : Bool :=
  if high then mask (W r) (σ r) != 0 else mask (W r) (σ r) == 0

/-- Register state after a rising edge of `clk` from the settled state `σ`:
synchronous reset has priority over the `seq` body. -/
def edge (m : Module) (σ : Env) : Env :=
  match m.rst with
  | none => execN m.W σ m.seq σ
  | some (r, high) =>
    if rstActive m.W r high σ then applyResets m.W σ m.regs σ else execN m.W σ m.seq σ

/-- Register state in the cycle whose settled state is `post`, following the
cycle `pre`. A clock edge separates the two cycles. With an asynchronous
reset, any cycle in which reset is asserted shows every register at its reset
value without waiting for a clock edge (the reset values are evaluated in
that cycle); the clock edge itself sees the pre-edge reset level, exactly as
in the synchronous case. -/
def next (m : Module) (pre post : Env) : Env :=
  match m.rst with
  | some (r, high) =>
    if m.rstAsync ∧ rstActive m.W r high post then applyResets m.W post m.regs post
    else m.edge pre
  | none => m.edge pre

/-- Trace semantics: every `let`/`comb` process is settled in every cycle,
and every register takes its `next` value in the next cycle. Inputs are
unconstrained. -/
def Run (m : Module) (tr : Nat → Env) : Prop :=
  (∀ p ∈ m.inits, tr 0 p.1 = mask (m.W p.1) (eval m.W (tr 0) p.2)) ∧
  ∀ t, (∀ p ∈ m.combs, ∀ y, execB m.W p (tr t) y = tr t y) ∧
    ∀ r ∈ m.regs.map Prod.fst, tr (t + 1) r = m.next (tr t) (tr (t + 1)) r

end Module

/-! ## Lowering to the sized IR -/

section
variable (W : Sig → Nat)

def lower : Expr → Sir
  | .sig x => .var x (W x)
  | .lit w n => .resize w (.lit n)
  | .bool b => .lit (b2n b)
  | .add a b => .bin .add (max (width W a) (width W b) + 1) (lower a) (lower b)
  | .sub a b => .bin .sub (max (width W a) (width W b) + 1) (lower a) (lower b)
  | .mul a b => .bin .mul (width W a + width W b) (lower a) (lower b)
  | .binW op a b => .bin op (max (width W a) (width W b)) (lower a) (lower b)
  | .bnot a => .bnot (width W a) (lower a)
  | .cmp op a b => .cmp op (lower a) (lower b)
  | .lnot a => .lnot (lower a)
  | .land a b => .land (lower a) (lower b)
  | .lor a b => .lor (lower a) (lower b)
  | .mux c a b => .mux (max (width W a) (width W b)) (lower c) (lower a) (lower b)
  | .resize n a => .resize n (lower a)
  | .slice hi lo a => .slice hi lo (lower a)

theorem eval_lower (σ : Env) : ∀ e : Expr, (lower W e).eval σ = eval W σ e
  | .sig _ | .lit .. | .bool _ => rfl
  | .add a b | .sub a b | .mul a b | .binW _ a b | .cmp _ a b | .land a b | .lor a b => by
    simp only [lower, Sir.eval, eval, BinOp.apply, eval_lower σ a, eval_lower σ b]
  | .bnot a | .lnot a | .resize _ a | .slice _ _ a => by
    simp only [lower, Sir.eval, eval, eval_lower σ a]
  | .mux c a b => by
    simp only [lower, Sir.eval, eval, eval_lower σ c, eval_lower σ a, eval_lower σ b]

def lowerStmts : Stmts → Arch.CodegenProof.Stmts
  | .nil => .nil
  | .assign x e r => .assign x (.resize (W x) (lower W e)) (lowerStmts r)
  | .ite c t f r => .ite (lower W c) (lowerStmts t) (lowerStmts f) (lowerStmts r)

theorem execB_lower : ∀ (s : Stmts) (σ : Env), (lowerStmts W s).execB σ = execB W s σ
  | .nil, _ => rfl
  | .assign x e r, σ => by
    simp only [lowerStmts, Arch.CodegenProof.Stmts.execB, execB, Sir.eval, eval_lower,
      execB_lower r]
  | .ite c t f r, σ => by
    simp only [lowerStmts, Arch.CodegenProof.Stmts.execB, execB, eval_lower, execB_lower t,
      execB_lower f, execB_lower r]

theorem execN_lower (pre : Env) :
    ∀ (s : Stmts) (acc : Env), (lowerStmts W s).execN pre acc = execN W pre s acc
  | .nil, _ => rfl
  | .assign x e r, acc => by
    simp only [lowerStmts, Arch.CodegenProof.Stmts.execN, execN, Sir.eval, eval_lower,
      execN_lower pre r]
  | .ite c t f r, acc => by
    simp only [lowerStmts, Arch.CodegenProof.Stmts.execN, execN, eval_lower,
      execN_lower pre t, execN_lower pre f, execN_lower pre r]

def lowerResets : List (Sig × Expr) → Arch.CodegenProof.Stmts
  | [] => .nil
  | (r, v) :: l => .assign r (.resize (W r) (lower W v)) (lowerResets l)

theorem execN_lowerResets (pre : Env) :
    ∀ (l : List (Sig × Expr)) (acc : Env), (lowerResets W l).execN pre acc = applyResets W pre l acc
  | [], _ => rfl
  | (r, v) :: l, acc => by
    simp only [lowerResets, Arch.CodegenProof.Stmts.execN, applyResets, Sir.eval, eval_lower,
      execN_lowerResets pre l]

end

namespace Module

def rstCond (W : Sig → Nat) (r : Sig) (high : Bool) : Sir :=
  if high then .var r (W r) else .lnot (.var r (W r))

def lowerSeq (m : Module) : Arch.CodegenProof.Stmts :=
  match m.rst with
  | none => lowerStmts m.W m.seq
  | some (r, high) => .ite (rstCond m.W r high) (lowerResets m.W m.regs) (lowerStmts m.W m.seq) .nil

def toNetlist (m : Module) : Netlist where
  combs := m.combs.map (lowerStmts m.W)
  seq := m.lowerSeq
  regs := m.regs.map Prod.fst
  async := if m.rstAsync then m.rst.map (fun p => rstCond m.W p.1 p.2) else none
  inits := m.inits.map (fun p => (p.1, .resize (m.W p.1) (lower m.W p.2)))

theorem edge_toNetlist (m : Module) (σ : Env) : m.toNetlist.edge σ = m.edge σ := by
  simp only [Netlist.edge, toNetlist, lowerSeq, edge]
  split
  · exact execN_lower _ _ _ _
  · next r high _ =>
    simp only [Arch.CodegenProof.Stmts.execN, rstActive, rstCond, execN_lowerResets,
      execN_lower]
    cases high <;> simp [Sir.eval, b2n]

theorem next_toNetlist (m : Module) (pre post : Env) :
    m.toNetlist.next pre post = m.next pre post := by
  have he : ∀ σ, m.toNetlist.edge σ = m.edge σ := edge_toNetlist m
  have hasync : m.toNetlist.async =
      if m.rstAsync then m.rst.map (fun p => rstCond m.W p.1 p.2) else none := rfl
  have hseq : m.toNetlist.seq.execN post post = m.edge post := he post
  unfold Netlist.next next
  rw [hasync]
  cases hr : m.rst with
  | none => cases m.rstAsync <;> simp [he]
  | some p =>
    obtain ⟨r, high⟩ := p
    cases ha : m.rstAsync
    · simp [he]
    · have hc : ((rstCond m.W r high).eval post ≠ 0) = (rstActive m.W r high post = true) := by
        cases high <;> simp [rstCond, rstActive, Sir.eval, b2n]
      simp only [if_true, Option.map_some, hc, true_and]
      split
      · next hact => rw [hseq]; simp [edge, hr, hact]
      · exact he pre

theorem run_iff (m : Module) (tr : Nat → Env) : m.Run tr ↔ m.toNetlist.Run tr := by
  have hinit : (∀ p ∈ m.inits, tr 0 p.1 = mask (m.W p.1) (eval m.W (tr 0) p.2)) ↔
      m.toNetlist.InitOk (tr 0) := by
    simp only [Netlist.InitOk, toNetlist, List.mem_map, forall_exists_index, and_imp]
    constructor
    · rintro hs _ p hpm rfl
      simp only [Sir.eval, eval_lower]
      exact hs p hpm
    · intro hs p hpm
      simpa only [Sir.eval, eval_lower] using hs _ p hpm rfl
  simp only [Run, Netlist.Run, hinit, AllStable, Arch.CodegenProof.Stmts.Stable,
    next_toNetlist]
  simp [toNetlist, execB_lower]

end Module

end Core
end Arch.CodegenProof
