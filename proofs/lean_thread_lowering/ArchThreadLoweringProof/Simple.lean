/-!
Prototype certificate model for ARCH thread-to-FSM lowering.

This file intentionally models a very small fragment first: straight-line
thread states with per-cycle observable actions and optional `wait until`
guards. The lowered FSM is accepted by a certificate predicate that ties each
runtime state back to the source state it claims to implement.

The theorem at the bottom is the shape we want for the real backend:
if the generated FSM table satisfies the certificate, then source thread
execution and lowered FSM execution have the same observable trace for every
input stream and every cycle.
-/

namespace Arch.ThreadLoweringProof

abbrev Action := Nat
abbrev Guard := Nat
abbrev Env := Guard -> Bool

structure ThreadState where
  actions : List Action
  wait : Option Guard
deriving Repr, BEq

structure SourceThread where
  numStates : Nat
  state : Nat -> ThreadState

structure FsmState where
  actions : List Action
  wait : Option Guard
  target : Nat
deriving Repr, BEq

structure LoweredFsm where
  state : Nat -> FsmState

def sourceNext (src : SourceThread) (pc : Nat) : Nat :=
  if pc + 1 < src.numStates then pc + 1 else 0

def sourceObs (src : SourceThread) (pc : Nat) : List Action :=
  (src.state pc).actions

def fsmObs (fsm : LoweredFsm) (pc : Nat) : List Action :=
  (fsm.state pc).actions

def sourceStep (src : SourceThread) (env : Env) (pc : Nat) : Nat :=
  match (src.state pc).wait with
  | none => sourceNext src pc
  | some guard => if env guard then sourceNext src pc else pc

def fsmStep (fsm : LoweredFsm) (env : Env) (pc : Nat) : Nat :=
  let state := fsm.state pc
  match state.wait with
  | none => state.target
  | some guard => if env guard then state.target else pc

structure LoweringCertifies (src : SourceThread) (fsm : LoweredFsm) : Prop where
  actions_ok : forall pc, (fsm.state pc).actions = (src.state pc).actions
  wait_ok : forall pc, (fsm.state pc).wait = (src.state pc).wait
  target_ok : forall pc, (fsm.state pc).target = sourceNext src pc

theorem one_step_equiv
    {src : SourceThread}
    {fsm : LoweredFsm}
    (cert : LoweringCertifies src fsm)
    (env : Env)
    (pc : Nat) :
    sourceObs src pc = fsmObs fsm pc
      /\ sourceStep src env pc = fsmStep fsm env pc := by
  constructor
  · simp [sourceObs, fsmObs, cert.actions_ok pc]
  · simp [sourceStep, fsmStep, cert.wait_ok pc, cert.target_ok pc]

def sourcePcAt
    (src : SourceThread)
    (inputs : Nat -> Env)
    (pc0 : Nat) : Nat -> Nat
  | 0 => pc0
  | Nat.succ t => sourceStep src (inputs t) (sourcePcAt src inputs pc0 t)

def fsmPcAt
    (fsm : LoweredFsm)
    (inputs : Nat -> Env)
    (pc0 : Nat) : Nat -> Nat
  | 0 => pc0
  | Nat.succ t => fsmStep fsm (inputs t) (fsmPcAt fsm inputs pc0 t)

theorem pc_trace_equiv
    {src : SourceThread}
    {fsm : LoweredFsm}
    (cert : LoweringCertifies src fsm)
    (inputs : Nat -> Env)
    (pc0 : Nat) :
    forall t, sourcePcAt src inputs pc0 t = fsmPcAt fsm inputs pc0 t := by
  intro t
  induction t with
  | zero => rfl
  | succ t ih =>
      simp [sourcePcAt, fsmPcAt]
      rw [ih]
      exact (one_step_equiv cert (inputs t) (fsmPcAt fsm inputs pc0 t)).right

def sourceTraceObs
    (src : SourceThread)
    (inputs : Nat -> Env)
    (pc0 : Nat)
    (t : Nat) : List Action :=
  sourceObs src (sourcePcAt src inputs pc0 t)

def fsmTraceObs
    (fsm : LoweredFsm)
    (inputs : Nat -> Env)
    (pc0 : Nat)
    (t : Nat) : List Action :=
  fsmObs fsm (fsmPcAt fsm inputs pc0 t)

theorem trace_equiv
    {src : SourceThread}
    {fsm : LoweredFsm}
    (cert : LoweringCertifies src fsm)
    (inputs : Nat -> Env)
    (pc0 : Nat) :
    forall t, sourceTraceObs src inputs pc0 t = fsmTraceObs fsm inputs pc0 t := by
  intro t
  have hpc := pc_trace_equiv cert inputs pc0 t
  simp [sourceTraceObs, fsmTraceObs, hpc]
  exact (one_step_equiv cert (fun _ => false) (fsmPcAt fsm inputs pc0 t)).left

def reqGuard : Guard := 0

def exampleSource : SourceThread :=
  { numStates := 2
    state := fun pc =>
      if pc = 0 then
        { actions := [10], wait := some reqGuard }
      else
        { actions := [20], wait := none } }

def exampleFsm : LoweredFsm :=
  { state := fun pc =>
      if pc = 0 then
        { actions := [10], wait := some reqGuard, target := 1 }
      else
        { actions := [20], wait := none, target := sourceNext exampleSource pc } }

example : LoweringCertifies exampleSource exampleFsm := by
  refine
    { actions_ok := ?_
      wait_ok := ?_
      target_ok := ?_ }
  · intro pc
    by_cases h : pc = 0 <;> simp [exampleSource, exampleFsm, h]
  · intro pc
    by_cases h : pc = 0 <;> simp [exampleSource, exampleFsm, h]
  · intro pc
    by_cases h : pc = 0 <;> simp [exampleSource, exampleFsm, sourceNext, h]

end Arch.ThreadLoweringProof
