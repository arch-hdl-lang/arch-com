import Std

/-!
Reusable proof model for first-class ARCH arbiters.

The generated certificate instantiates these definitions with the NUM_REQ,
policy, and latency seen by the compiler.  The theorems are parameterized over
the request vector, so they are not bounded to a particular test trace.
-/

namespace Arch.ConstructProof.Arbiter

inductive Policy where
  | priority
  | roundRobin
deriving Repr, BEq

structure Instance where
  name : String
  numReq : Nat
  policy : Policy
  latency : Nat
deriving Repr, BEq

def RequestVec (inst : Instance) : Type := Fin inst.numReq -> Bool

def oneHot (inst : Instance) (idx : Fin inst.numReq) : Fin inst.numReq -> Bool :=
  fun j => j = idx

def noGrant (_inst : Instance) : Fin _inst.numReq -> Bool :=
  fun _ => false

def priorityReady (req : Fin n -> Bool) (idx : Fin n) : Prop :=
  req idx = true /\ forall j : Fin n, j.val < idx.val -> req j = false

def roundRobinReady (start : Fin n) (req : Fin n -> Bool) (idx : Fin n) : Prop :=
  req idx = true
    /\ exists off : Nat,
      off < n
        /\ idx.val = (start.val + off) % n
        /\ forall j : Fin n,
          (exists earlier : Nat, earlier < off /\ j.val = (start.val + earlier) % n) ->
            req j = false

structure CorrectGrant (inst : Instance) : Prop where
  priority_subset :
    forall req : RequestVec inst, forall idx : Fin inst.numReq,
      priorityReady req idx -> req idx = true
  round_robin_subset :
    forall req : RequestVec inst, forall start idx : Fin inst.numReq,
      roundRobinReady start req idx -> req idx = true
  ready_onehot :
    forall idx : Fin inst.numReq, forall grant : Fin inst.numReq -> Bool,
      grant = oneHot inst idx -> (exists one : Fin inst.numReq, grant = oneHot inst one)

structure PriorityGenerated (inst : Instance) where
  readySelected : RequestVec inst -> Fin inst.numReq -> Prop
  readyVector : RequestVec inst -> Fin inst.numReq -> Fin inst.numReq -> Bool
  ready_selected_eq :
    forall req idx,
      readySelected req idx <-> priorityReady req idx
  ready_vector_eq :
    forall req idx,
      readySelected req idx -> readyVector req idx = oneHot inst idx

structure PriorityEquationsHold (inst : Instance) (eqs : PriorityGenerated inst) : Prop where
  ready_selected_eq :
    forall req idx,
      eqs.readySelected req idx <-> priorityReady req idx
  ready_vector_eq :
    forall req idx,
      eqs.readySelected req idx -> eqs.readyVector req idx = oneHot inst idx

structure RoundRobinGenerated (inst : Instance) where
  readySelected : Fin inst.numReq -> RequestVec inst -> Fin inst.numReq -> Prop
  readyVector : Fin inst.numReq -> RequestVec inst -> Fin inst.numReq -> Fin inst.numReq -> Bool
  nextPtr : Fin inst.numReq -> Fin inst.numReq -> Nat
  ready_selected_eq :
    forall start req idx,
      readySelected start req idx <-> roundRobinReady start req idx
  ready_vector_eq :
    forall start req idx,
      readySelected start req idx -> readyVector start req idx = oneHot inst idx
  next_ptr_eq :
    forall start idx,
      nextPtr start idx = if idx.val + 1 = inst.numReq then 0 else (idx.val + 1) % inst.numReq

structure RoundRobinEquationsHold (inst : Instance) (eqs : RoundRobinGenerated inst) : Prop where
  ready_selected_eq :
    forall start req idx,
      eqs.readySelected start req idx <-> roundRobinReady start req idx
  ready_vector_eq :
    forall start req idx,
      eqs.readySelected start req idx -> eqs.readyVector start req idx = oneHot inst idx
  next_ptr_eq :
    forall start idx,
      eqs.nextPtr start idx = if idx.val + 1 = inst.numReq then 0 else (idx.val + 1) % inst.numReq

theorem priority_subset
    (req : Fin n -> Bool)
    (idx : Fin n)
    (h : priorityReady req idx) :
    req idx = true := by
  exact h.1

theorem round_robin_subset
    (start : Fin n)
    (req : Fin n -> Bool)
    (idx : Fin n)
    (h : roundRobinReady start req idx) :
    req idx = true := by
  exact h.1

theorem onehot_witness
    (inst : Instance)
    (idx : Fin inst.numReq)
    (grant : Fin inst.numReq -> Bool)
    (h : grant = oneHot inst idx) :
    exists one : Fin inst.numReq, grant = oneHot inst one := by
  exact ⟨idx, h⟩

theorem generic_correct
    (inst : Instance) :
    CorrectGrant inst := by
  refine
    { priority_subset := ?_
      round_robin_subset := ?_
      ready_onehot := ?_ }
  · intro req idx h
    exact priority_subset req idx h
  · intro req start idx h
    exact round_robin_subset start req idx h
  · intro idx grant h
    exact onehot_witness inst idx grant h

theorem certificate_checks
    (inst : Instance)
    (h_req : 0 < inst.numReq)
    (h_latency : 0 < inst.latency) :
    0 < inst.numReq /\ 0 < inst.latency /\ CorrectGrant inst := by
  exact ⟨h_req, h_latency, generic_correct inst⟩

theorem priority_certificate_checks
    (inst : Instance)
    (eqs : PriorityGenerated inst)
    (h_req : 0 < inst.numReq)
    (h_latency : 0 < inst.latency) :
    0 < inst.numReq /\ 0 < inst.latency /\ PriorityEquationsHold inst eqs /\ CorrectGrant inst := by
  exact
    ⟨h_req, h_latency,
      { ready_selected_eq := eqs.ready_selected_eq
        ready_vector_eq := eqs.ready_vector_eq },
      generic_correct inst⟩

theorem round_robin_certificate_checks
    (inst : Instance)
    (eqs : RoundRobinGenerated inst)
    (h_req : 0 < inst.numReq)
    (h_latency : 0 < inst.latency) :
    0 < inst.numReq /\ 0 < inst.latency /\ RoundRobinEquationsHold inst eqs /\ CorrectGrant inst := by
  exact
    ⟨h_req, h_latency,
      { ready_selected_eq := eqs.ready_selected_eq
        ready_vector_eq := eqs.ready_vector_eq
        next_ptr_eq := eqs.next_ptr_eq },
      generic_correct inst⟩

end Arch.ConstructProof.Arbiter
