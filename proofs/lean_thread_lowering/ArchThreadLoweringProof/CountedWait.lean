/-!
Counted-wait extension for the ARCH thread-to-FSM lowering certificate model.

This module adds the first timing-sensitive ingredient from the real lowering:
`wait N cycle` states with a counter in the runtime configuration. Both the
source semantics and lowered FSM semantics load the counter when entering a
counted-wait state, decrement it while waiting, and advance when it reaches
zero.

It also models `multi_transitions`: a deterministic dispatch list of guarded
branches with a fall-through target. That covers the control-table shape used
by ARCH lowering for if-with-wait dispatch states, for-loop exits, and
fork/join product states.
-/

namespace Arch.ThreadLoweringProof.CountedWait

abbrev Action := Nat
abbrev Guard := Nat
abbrev Env := Guard -> Bool
abbrev NatEnv := Nat -> Nat

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

structure Branch where
  guard : GuardExpr
  target : Nat
deriving Repr, BEq

inductive Control where
  | advance
  | jump (target : Nat)
  | waitUntil (guard : GuardExpr)
  | waitCycles (count : Nat)
  | guarded (guard : GuardExpr) (target : Nat)
  | dispatch (branches : List Branch)
deriving Repr, BEq

def dispatchBranches : Control -> List Branch
  | Control.dispatch branches => branches
  | _ => []

structure ThreadState where
  actions : List Action
  control : Control
deriving Repr, BEq

structure SourceThread where
  numStates : Nat
  once : Bool
  state : Nat -> ThreadState

structure FsmState where
  actions : List Action
  control : Control
  target : Nat
deriving Repr, BEq

structure LoweredFsm where
  state : Nat -> FsmState

structure Config where
  pc : Nat
  cnt : Nat
deriving Repr, BEq

def sourceNext (src : SourceThread) (pc : Nat) : Nat :=
  if pc + 1 < src.numStates then pc + 1 else if src.once then pc else 0

def sourceObs (src : SourceThread) (cfg : Config) : List Action :=
  (src.state cfg.pc).actions

def fsmObs (fsm : LoweredFsm) (cfg : Config) : List Action :=
  (fsm.state cfg.pc).actions

def sourceLoadCounterAt (src : SourceThread) (pc : Nat) (oldCnt : Nat) : Nat :=
  match (src.state pc).control with
  | Control.waitCycles count => count.pred
  | _ => oldCnt

def fsmLoadCounterAt (fsm : LoweredFsm) (pc : Nat) (oldCnt : Nat) : Nat :=
  match (fsm.state pc).control with
  | Control.waitCycles count => count.pred
  | _ => oldCnt

def sourceAdvanceTo (src : SourceThread) (cfg : Config) (target : Nat) : Config :=
  { pc := target, cnt := sourceLoadCounterAt src target cfg.cnt }

def fsmAdvanceTo (fsm : LoweredFsm) (cfg : Config) (target : Nat) : Config :=
  { pc := target, cnt := fsmLoadCounterAt fsm target cfg.cnt }

def firstEnabledTarget (env : Env) (natEnv : NatEnv) (branches : List Branch) (fallback : Nat) : Nat :=
  match branches with
  | [] => fallback
  | branch :: rest =>
      if GuardExpr.eval env branch.guard natEnv then
        branch.target
      else
        firstEnabledTarget env natEnv rest fallback

theorem firstEnabledTarget_fallback_eq
    (env : Env)
    (natEnv : NatEnv)
    (branches : List Branch)
    {a b : Nat}
    (h : a = b) :
    firstEnabledTarget env natEnv branches a = firstEnabledTarget env natEnv branches b := by
  induction branches with
  | nil => simp [firstEnabledTarget, h]
  | cons branch rest ih =>
      by_cases hguard : GuardExpr.eval env branch.guard natEnv
      · simp [firstEnabledTarget, hguard]
      · simp [firstEnabledTarget, hguard, ih]

def sourceStep (src : SourceThread) (env : Env) (natEnv : NatEnv) (cfg : Config) : Config :=
  match (src.state cfg.pc).control with
  | Control.advance =>
      sourceAdvanceTo src cfg (sourceNext src cfg.pc)
  | Control.jump target =>
      sourceAdvanceTo src cfg target
  | Control.waitUntil guard =>
      if GuardExpr.eval env guard natEnv then
        sourceAdvanceTo src cfg (sourceNext src cfg.pc)
      else
        cfg
  | Control.waitCycles _ =>
      if cfg.cnt = 0 then
        sourceAdvanceTo src cfg (sourceNext src cfg.pc)
      else
        { cfg with cnt := cfg.cnt.pred }
  | Control.guarded guard target =>
      if GuardExpr.eval env guard natEnv then
        sourceAdvanceTo src cfg target
      else
        cfg
  | Control.dispatch branches =>
      sourceAdvanceTo src cfg (firstEnabledTarget env natEnv branches (sourceNext src cfg.pc))

def fsmStep (fsm : LoweredFsm) (env : Env) (natEnv : NatEnv) (cfg : Config) : Config :=
  let state := fsm.state cfg.pc
  match state.control with
  | Control.advance =>
      fsmAdvanceTo fsm cfg state.target
  | Control.jump target =>
      fsmAdvanceTo fsm cfg target
  | Control.waitUntil guard =>
      if GuardExpr.eval env guard natEnv then
        fsmAdvanceTo fsm cfg state.target
      else
        cfg
  | Control.waitCycles _ =>
      if cfg.cnt = 0 then
        fsmAdvanceTo fsm cfg state.target
      else
        { cfg with cnt := cfg.cnt.pred }
  | Control.guarded guard target =>
      if GuardExpr.eval env guard natEnv then
        fsmAdvanceTo fsm cfg target
      else
        cfg
  | Control.dispatch branches =>
      fsmAdvanceTo fsm cfg (firstEnabledTarget env natEnv branches state.target)

structure LoweringCertifies (src : SourceThread) (fsm : LoweredFsm) : Prop where
  actions_ok : forall pc, (fsm.state pc).actions = (src.state pc).actions
  control_ok : forall pc, (fsm.state pc).control = (src.state pc).control
  dispatch_branches_ok :
    forall pc, dispatchBranches (fsm.state pc).control = dispatchBranches (src.state pc).control
  target_ok : forall pc, (fsm.state pc).target = sourceNext src pc

theorem load_counter_equiv
    {src : SourceThread}
    {fsm : LoweredFsm}
    (cert : LoweringCertifies src fsm)
    (pc oldCnt : Nat) :
    sourceLoadCounterAt src pc oldCnt = fsmLoadCounterAt fsm pc oldCnt := by
  simp [sourceLoadCounterAt, fsmLoadCounterAt, cert.control_ok pc]

theorem advance_equiv
    {src : SourceThread}
    {fsm : LoweredFsm}
    (cert : LoweringCertifies src fsm)
    (cfg : Config)
    (target : Nat) :
    sourceAdvanceTo src cfg target = fsmAdvanceTo fsm cfg target := by
  simp [sourceAdvanceTo, fsmAdvanceTo, load_counter_equiv cert target cfg.cnt]

theorem one_step_equiv
    {src : SourceThread}
    {fsm : LoweredFsm}
    (cert : LoweringCertifies src fsm)
    (env : Env)
    (natEnv : NatEnv)
    (cfg : Config) :
    sourceObs src cfg = fsmObs fsm cfg
      /\ sourceStep src env natEnv cfg = fsmStep fsm env natEnv cfg := by
  constructor
  · simp [sourceObs, fsmObs, cert.actions_ok cfg.pc]
  · cases h : (src.state cfg.pc).control with
    | advance =>
        simp [
          sourceStep,
          fsmStep,
          h,
          cert.control_ok cfg.pc,
          cert.target_ok cfg.pc,
          advance_equiv cert cfg (sourceNext src cfg.pc),
        ]
    | jump target =>
        simp [
          sourceStep,
          fsmStep,
          h,
          cert.control_ok cfg.pc,
          advance_equiv cert cfg target,
        ]
    | waitUntil guard =>
        by_cases henv : GuardExpr.eval env guard natEnv
        · simp [
            sourceStep,
            fsmStep,
            h,
            henv,
            cert.control_ok cfg.pc,
            cert.target_ok cfg.pc,
            advance_equiv cert cfg (sourceNext src cfg.pc),
          ]
        · simp [sourceStep, fsmStep, h, henv, cert.control_ok cfg.pc]
    | waitCycles count =>
        by_cases hcnt : cfg.cnt = 0
        · simp [
            sourceStep,
            fsmStep,
            h,
            hcnt,
            cert.control_ok cfg.pc,
            cert.target_ok cfg.pc,
            advance_equiv cert cfg (sourceNext src cfg.pc),
          ]
        · simp [sourceStep, fsmStep, h, hcnt, cert.control_ok cfg.pc]
    | guarded guard target =>
        by_cases henv : GuardExpr.eval env guard natEnv
        · simp [
            sourceStep,
            fsmStep,
            h,
            henv,
            cert.control_ok cfg.pc,
            advance_equiv cert cfg target,
          ]
        · simp [sourceStep, fsmStep, h, henv, cert.control_ok cfg.pc]
    | dispatch branches =>
        have htarget :
            firstEnabledTarget env natEnv branches (sourceNext src cfg.pc)
              = firstEnabledTarget env natEnv branches (fsm.state cfg.pc).target :=
          firstEnabledTarget_fallback_eq env natEnv branches (cert.target_ok cfg.pc).symm
        simp [
          sourceStep,
          fsmStep,
          h,
          cert.control_ok cfg.pc,
          htarget,
          advance_equiv cert cfg (firstEnabledTarget env natEnv branches (fsm.state cfg.pc).target),
        ]

structure StepEffectFaithful (src : SourceThread) (fsm : LoweredFsm) : Prop where
  step_ok :
    forall env natEnv cfg,
      sourceStep src env natEnv cfg = fsmStep fsm env natEnv cfg
  pc_ok :
    forall env natEnv cfg,
      (sourceStep src env natEnv cfg).pc = (fsmStep fsm env natEnv cfg).pc
  counter_ok :
    forall env natEnv cfg,
      (sourceStep src env natEnv cfg).cnt = (fsmStep fsm env natEnv cfg).cnt

theorem step_effect_faithful
    {src : SourceThread}
    {fsm : LoweredFsm}
    (cert : LoweringCertifies src fsm) :
    StepEffectFaithful src fsm := by
  refine
    { step_ok := ?_
      pc_ok := ?_
      counter_ok := ?_ }
  · intro env natEnv cfg
    exact (one_step_equiv cert env natEnv cfg).right
  · intro env natEnv cfg
    exact congrArg Config.pc ((one_step_equiv cert env natEnv cfg).right)
  · intro env natEnv cfg
    exact congrArg Config.cnt ((one_step_equiv cert env natEnv cfg).right)

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
      exact (one_step_equiv cert (inputs t) (natInputs t) (fsmCfgAt fsm inputs natInputs cfg0 t)).right

def sourceTraceObs
    (src : SourceThread)
    (inputs : Nat -> Env)
    (natInputs : Nat -> NatEnv)
    (cfg0 : Config)
    (t : Nat) : List Action :=
  sourceObs src (sourceCfgAt src inputs natInputs cfg0 t)

def fsmTraceObs
    (fsm : LoweredFsm)
    (inputs : Nat -> Env)
    (natInputs : Nat -> NatEnv)
    (cfg0 : Config)
    (t : Nat) : List Action :=
  fsmObs fsm (fsmCfgAt fsm inputs natInputs cfg0 t)

theorem trace_equiv
    {src : SourceThread}
    {fsm : LoweredFsm}
    (cert : LoweringCertifies src fsm)
    (inputs : Nat -> Env)
    (natInputs : Nat -> NatEnv)
    (cfg0 : Config) :
    forall t, sourceTraceObs src inputs natInputs cfg0 t = fsmTraceObs fsm inputs natInputs cfg0 t := by
  intro t
  have hcfg := cfg_trace_equiv cert inputs natInputs cfg0 t
  simp [sourceTraceObs, fsmTraceObs, hcfg]
  exact (one_step_equiv cert (fun _ => false) (fun _ => 0) (fsmCfgAt fsm inputs natInputs cfg0 t)).left

def reqGuard : Guard := 0
def reqGuardExpr : GuardExpr := GuardExpr.atom reqGuard

def exampleSource : SourceThread :=
  { numStates := 4
    once := false
    state := fun pc =>
      if pc = 0 then
        { actions := [10], control := Control.waitUntil reqGuardExpr }
      else if pc = 1 then
        { actions := [20], control := Control.waitCycles 3 }
      else if pc = 2 then
        { actions := [30], control := Control.dispatch [
            { guard := GuardExpr.atom 1, target := 0 },
            { guard := GuardExpr.atom 2, target := 3 },
          ] }
      else
        { actions := [40], control := Control.advance } }

def exampleFsm : LoweredFsm :=
  { state := fun pc =>
      if pc = 0 then
        { actions := [10], control := Control.waitUntil reqGuardExpr, target := 1 }
      else if pc = 1 then
        { actions := [20], control := Control.waitCycles 3, target := 2 }
      else if pc = 2 then
        { actions := [30], control := Control.dispatch [
            { guard := GuardExpr.atom 1, target := 0 },
            { guard := GuardExpr.atom 2, target := 3 },
          ], target := sourceNext exampleSource pc }
      else
        { actions := [40], control := Control.advance, target := sourceNext exampleSource pc } }

example : LoweringCertifies exampleSource exampleFsm := by
  refine
    { actions_ok := ?_
      control_ok := ?_
      dispatch_branches_ok := ?_
      target_ok := ?_ }
  · intro pc
    by_cases h0 : pc = 0 <;> by_cases h1 : pc = 1 <;> by_cases h2 : pc = 2 <;>
      simp [exampleSource, exampleFsm, h0, h1, h2]
  · intro pc
    by_cases h0 : pc = 0 <;> by_cases h1 : pc = 1 <;> by_cases h2 : pc = 2 <;>
      simp [exampleSource, exampleFsm, h0, h1, h2]
  · intro pc
    by_cases h0 : pc = 0 <;> by_cases h1 : pc = 1 <;> by_cases h2 : pc = 2 <;>
      simp [exampleSource, exampleFsm, dispatchBranches, h0, h1, h2]
  · intro pc
    by_cases h0 : pc = 0 <;> by_cases h1 : pc = 1 <;> by_cases h2 : pc = 2 <;>
      simp [exampleSource, exampleFsm, sourceNext, h0, h1, h2]

end Arch.ThreadLoweringProof.CountedWait
