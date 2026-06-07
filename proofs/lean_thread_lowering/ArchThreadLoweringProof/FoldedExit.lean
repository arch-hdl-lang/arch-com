/-!
Folded wait-exit assignment proof for ARCH thread-to-FSM lowering.

The Rust lowering has an optimization (`fold_wait_until_exit_assignments`) that
moves a pure action state's sequential assignments into the preceding
`wait until` state's exit arm, then skips the absorbed action state. This file
models that timing shape with an abstract register store and proves that a
certified folded FSM step matches the source thread step for every environment
and configuration.
-/

namespace Arch.ThreadLoweringProof.FoldedExit

abbrev Guard := Nat
abbrev Var := Nat
abbrev Value := Nat
abbrev Env := Guard -> Bool
abbrev NatEnv := Nat -> Nat
abbrev Store := Var -> Value
abbrev Update := Store -> Store

inductive NatExpr where
  | var (name : Nat)
  | const (value : Nat)
deriving Repr, BEq

def NatExpr.eval (env : NatEnv) : NatExpr -> Nat
  | NatExpr.var name => env name
  | NatExpr.const value => value

inductive GuardExpr where
  | atom (guard : Guard)
  | trueLit
  | falseLit
  | neg (expr : GuardExpr)
  | and (lhs rhs : GuardExpr)
  | or (lhs rhs : GuardExpr)
  | lt (lhs rhs : NatExpr)
  | ge (lhs rhs : NatExpr)
  | eq (lhs rhs : NatExpr)
  | ne (lhs rhs : NatExpr)
deriving Repr, BEq

def GuardExpr.eval (env : Env) (expr : GuardExpr) (natEnv : NatEnv := fun _ => 0) : Bool :=
  match expr with
  | GuardExpr.atom guard => env guard
  | GuardExpr.trueLit => true
  | GuardExpr.falseLit => false
  | GuardExpr.neg expr => !(GuardExpr.eval env expr natEnv)
  | GuardExpr.and lhs rhs => GuardExpr.eval env lhs natEnv && GuardExpr.eval env rhs natEnv
  | GuardExpr.or lhs rhs => GuardExpr.eval env lhs natEnv || GuardExpr.eval env rhs natEnv
  | GuardExpr.lt lhs rhs => decide (NatExpr.eval natEnv lhs < NatExpr.eval natEnv rhs)
  | GuardExpr.ge lhs rhs => decide (NatExpr.eval natEnv rhs <= NatExpr.eval natEnv lhs)
  | GuardExpr.eq lhs rhs => decide (NatExpr.eval natEnv lhs = NatExpr.eval natEnv rhs)
  | GuardExpr.ne lhs rhs => decide (NatExpr.eval natEnv lhs ≠ NatExpr.eval natEnv rhs)

def applyUpdates (updates : List Update) (store : Store) : Store :=
  updates.foldl (fun acc update => update acc) store

inductive Control where
  | advance
  | waitUntil (guard : GuardExpr)
deriving Repr, BEq

structure SourceState where
  updates : List Update
  exitUpdates : List Update
  control : Control

structure SourceThread where
  numStates : Nat
  state : Nat -> SourceState

structure FsmState where
  updates : List Update
  foldedExitUpdates : List Update
  control : Control
  target : Nat
  foldedTarget : Option Nat

structure LoweredFsm where
  state : Nat -> FsmState

structure Config where
  pc : Nat
  store : Store

def sourceNext (src : SourceThread) (pc : Nat) : Nat :=
  if pc + 1 < src.numStates then pc + 1 else 0

def sourceAdvanceTo
    (src : SourceThread)
    (cfg : Config)
    (target : Nat)
    (exitUpdates : List Update) : Config :=
  let base := applyUpdates (src.state cfg.pc).updates cfg.store
  { pc := target, store := applyUpdates exitUpdates base }

def fsmAdvanceTo
    (fsm : LoweredFsm)
    (cfg : Config)
    (target : Nat)
    (exitUpdates : List Update) : Config :=
  let base := applyUpdates (fsm.state cfg.pc).updates cfg.store
  { pc := target, store := applyUpdates exitUpdates base }

def sourceStep (src : SourceThread) (env : Env) (natEnv : NatEnv) (cfg : Config) : Config :=
  match (src.state cfg.pc).control with
  | Control.advance =>
      sourceAdvanceTo src cfg (sourceNext src cfg.pc) []
  | Control.waitUntil guard =>
      if GuardExpr.eval env guard natEnv then
        sourceAdvanceTo src cfg (sourceNext src cfg.pc) (src.state cfg.pc).exitUpdates
      else
        cfg

def fsmStep (fsm : LoweredFsm) (env : Env) (natEnv : NatEnv) (cfg : Config) : Config :=
  let state := fsm.state cfg.pc
  match state.control with
  | Control.advance =>
      fsmAdvanceTo fsm cfg state.target []
  | Control.waitUntil guard =>
      if GuardExpr.eval env guard natEnv then
        let target := state.foldedTarget.getD state.target
        fsmAdvanceTo fsm cfg target state.foldedExitUpdates
      else
        cfg

structure LoweringCertifies (src : SourceThread) (fsm : LoweredFsm) : Prop where
  updates_ok : forall pc, (fsm.state pc).updates = (src.state pc).updates
  control_ok : forall pc, (fsm.state pc).control = (src.state pc).control
  target_ok : forall pc, (fsm.state pc).target = sourceNext src pc
  folded_updates_ok :
    forall pc, (fsm.state pc).foldedExitUpdates = (src.state pc).exitUpdates
  folded_target_ok :
    forall pc,
      (src.state pc).exitUpdates = [] ->
        (fsm.state pc).foldedTarget = none
      \/ (fsm.state pc).foldedTarget = some (sourceNext src pc)
  folded_target_some_ok :
    forall pc target,
      (fsm.state pc).foldedTarget = some target ->
        target = sourceNext src pc

theorem advance_equiv
    {src : SourceThread}
    {fsm : LoweredFsm}
    (cert : LoweringCertifies src fsm)
    (cfg : Config)
    (target : Nat)
    (exitUpdates : List Update)
    (htarget : target = sourceNext src cfg.pc) :
    sourceAdvanceTo src cfg (sourceNext src cfg.pc) exitUpdates
      = fsmAdvanceTo fsm cfg target exitUpdates := by
  simp [sourceAdvanceTo, fsmAdvanceTo, cert.updates_ok cfg.pc, htarget]

theorem folded_target_value
    {src : SourceThread}
    {fsm : LoweredFsm}
    (cert : LoweringCertifies src fsm)
    (pc : Nat) :
    (fsm.state pc).foldedTarget.getD (fsm.state pc).target = sourceNext src pc := by
  cases hfold : (fsm.state pc).foldedTarget with
  | none =>
      simp [cert.target_ok pc]
  | some target =>
      have htarget := cert.folded_target_some_ok pc target hfold
      simp [htarget]

theorem one_step_equiv
    {src : SourceThread}
    {fsm : LoweredFsm}
    (cert : LoweringCertifies src fsm)
    (env : Env)
    (natEnv : NatEnv)
    (cfg : Config) :
    sourceStep src env natEnv cfg = fsmStep fsm env natEnv cfg := by
  cases h : (src.state cfg.pc).control with
  | advance =>
      simp [
        sourceStep,
        fsmStep,
        h,
        cert.control_ok cfg.pc,
        cert.target_ok cfg.pc,
        advance_equiv cert cfg (sourceNext src cfg.pc) [] rfl,
      ]
  | waitUntil guard =>
      by_cases henv : GuardExpr.eval env guard natEnv
      · have htarget :
            (fsm.state cfg.pc).foldedTarget.getD (fsm.state cfg.pc).target
              = sourceNext src cfg.pc :=
          folded_target_value cert cfg.pc
        simp [
          sourceStep,
          fsmStep,
          h,
          henv,
          cert.control_ok cfg.pc,
          cert.folded_updates_ok cfg.pc,
          htarget,
          advance_equiv cert cfg
            ((fsm.state cfg.pc).foldedTarget.getD (fsm.state cfg.pc).target)
            (src.state cfg.pc).exitUpdates
            htarget,
        ]
      · simp [sourceStep, fsmStep, h, henv, cert.control_ok cfg.pc]

def sourceCfgAt
    (src : SourceThread)
    (inputs : Nat -> Env)
    (natInputs : Nat -> NatEnv)
    (cfg0 : Config) : Nat -> Config
  | 0 => cfg0
  | Nat.succ t => sourceStep src (inputs t) (natInputs t) (sourceCfgAt src inputs natInputs cfg0 t)

def fsmCfgAt
    (fsm : LoweredFsm)
    (inputs : Nat -> Env)
    (natInputs : Nat -> NatEnv)
    (cfg0 : Config) : Nat -> Config
  | 0 => cfg0
  | Nat.succ t => fsmStep fsm (inputs t) (natInputs t) (fsmCfgAt fsm inputs natInputs cfg0 t)

theorem cfg_trace_equiv
    {src : SourceThread}
    {fsm : LoweredFsm}
    (cert : LoweringCertifies src fsm)
    (inputs : Nat -> Env)
    (natInputs : Nat -> NatEnv)
    (cfg0 : Config) :
    forall t, sourceCfgAt src inputs natInputs cfg0 t = fsmCfgAt fsm inputs natInputs cfg0 t := by
  intro t
  induction t with
  | zero => rfl
  | succ t ih =>
      simp [sourceCfgAt, fsmCfgAt]
      rw [ih]
      exact one_step_equiv cert (inputs t) (natInputs t) (fsmCfgAt fsm inputs natInputs cfg0 t)

def setVar (var : Var) (value : Value) : Update :=
  fun store name => if name = var then value else store name

theorem setVar_same (var : Var) (value : Value) (store : Store) :
    setVar var value store var = value := by
  simp [setVar]

theorem setVar_other
    {var other : Var}
    (value : Value)
    (store : Store)
    (h : other ≠ var) :
    setVar var value store other = store other := by
  simp [setVar, h]

theorem applyUpdates_single_setVar_same
    (var : Var)
    (value : Value)
    (store : Store) :
    applyUpdates [setVar var value] store var = value := by
  simp [applyUpdates, setVar]

def reqGuard : Guard := 0
def reqGuardExpr : GuardExpr := GuardExpr.atom reqGuard
def outVar : Var := 0

def exampleSource : SourceThread :=
  { numStates := 2
    state := fun pc =>
      if pc = 0 then
        { updates := []
          exitUpdates := [setVar outVar 42]
          control := Control.waitUntil reqGuardExpr }
      else
        { updates := []
          exitUpdates := []
          control := Control.advance } }

def exampleFsm : LoweredFsm :=
  { state := fun pc =>
      if pc = 0 then
        { updates := []
          foldedExitUpdates := [setVar outVar 42]
          control := Control.waitUntil reqGuardExpr
          target := 1
          foldedTarget := some 1 }
      else
        { updates := []
          foldedExitUpdates := []
          control := Control.advance
          target := sourceNext exampleSource pc
          foldedTarget := none } }

example : LoweringCertifies exampleSource exampleFsm := by
  refine
    { updates_ok := ?_
      control_ok := ?_
      target_ok := ?_
      folded_updates_ok := ?_
      folded_target_ok := ?_
      folded_target_some_ok := ?_ }
  · intro pc
    by_cases h : pc = 0 <;> simp [exampleSource, exampleFsm, h]
  · intro pc
    by_cases h : pc = 0 <;> simp [exampleSource, exampleFsm, h]
  · intro pc
    by_cases h : pc = 0 <;> simp [exampleSource, exampleFsm, sourceNext, h]
  · intro pc
    by_cases h : pc = 0 <;> simp [exampleSource, exampleFsm, h]
  · intro pc hupdates
    by_cases h : pc = 0 <;> simp [exampleSource, exampleFsm, sourceNext, h] at *
  · intro pc target hfold
    by_cases h : pc = 0 <;> simp [exampleSource, exampleFsm, sourceNext, h] at *
    exact hfold.symm

end Arch.ThreadLoweringProof.FoldedExit
