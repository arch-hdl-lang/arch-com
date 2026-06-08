import Std

/-!
Reusable proof model for first-class ARCH credit_channel accounting.

The model captures the sender credit counter and receiver FIFO occupancy
refinement expected from a credit_channel instance.  Generated certificates can
instantiate the generic theorems with the DEPTH and equations emitted by the
compiler.
-/

namespace Arch.ConstructProof.CreditChannel

structure Instance where
  name : String
  depth : Nat
  payloadWidth : Nat
deriving Repr, BEq

structure State where
  credit : Nat
  occupancy : Nat
deriving Repr, BEq

def initialState (inst : Instance) : State :=
  { credit := inst.depth, occupancy := 0 }

def balanced (inst : Instance) (st : State) : Prop :=
  st.credit <= inst.depth
    /\ st.occupancy <= inst.depth
    /\ st.credit + st.occupancy = inst.depth

def canSend (st : State) : Bool :=
  decide (0 < st.credit)

def receiverValid (st : State) : Bool :=
  decide (0 < st.occupancy)

structure LegalStep (st : State) (send creditReturn : Bool) : Prop where
  send_requires_credit :
    send = true -> 0 < st.credit
  return_requires_occupancy :
    creditReturn = true -> 0 < st.occupancy

def nextCredit (credit : Nat) (send creditReturn : Bool) : Nat :=
  match send, creditReturn with
  | true, false => credit - 1
  | false, true => credit + 1
  | _, _ => credit

def nextOccupancy (occupancy : Nat) (send creditReturn : Bool) : Nat :=
  match send, creditReturn with
  | true, false => occupancy + 1
  | false, true => occupancy - 1
  | _, _ => occupancy

def step (st : State) (send creditReturn : Bool) : State :=
  { credit := nextCredit st.credit send creditReturn
    occupancy := nextOccupancy st.occupancy send creditReturn }

structure Generated (inst : Instance) where
  canSend : State -> Bool
  receiverValid : State -> Bool
  nextCredit : State -> Bool -> Bool -> Nat
  nextOccupancy : State -> Bool -> Bool -> Nat
  can_send_eq :
    forall st,
      canSend st = Arch.ConstructProof.CreditChannel.canSend st
  receiver_valid_eq :
    forall st,
      receiverValid st = Arch.ConstructProof.CreditChannel.receiverValid st
  next_credit_eq :
    forall st send creditReturn,
      nextCredit st send creditReturn =
        Arch.ConstructProof.CreditChannel.nextCredit st.credit send creditReturn
  next_occupancy_eq :
    forall st send creditReturn,
      nextOccupancy st send creditReturn =
        Arch.ConstructProof.CreditChannel.nextOccupancy st.occupancy send creditReturn

structure EquationsHold (inst : Instance) (eqs : Generated inst) : Prop where
  can_send_eq :
    forall st,
      eqs.canSend st = canSend st
  receiver_valid_eq :
    forall st,
      eqs.receiverValid st = receiverValid st
  next_credit_eq :
    forall st send creditReturn,
      eqs.nextCredit st send creditReturn =
        nextCredit st.credit send creditReturn
  next_occupancy_eq :
    forall st send creditReturn,
      eqs.nextOccupancy st send creditReturn =
        nextOccupancy st.occupancy send creditReturn

def generatedStep (eqs : Generated inst) (st : State) (send creditReturn : Bool) : State :=
  { credit := eqs.nextCredit st send creditReturn
    occupancy := eqs.nextOccupancy st send creditReturn }

structure ParametricProof (inst : Instance) (eqs : Generated inst) : Prop where
  depth_pos : 0 < inst.depth
  payload_width_pos : 0 < inst.payloadWidth
  equations : EquationsHold inst eqs
  can_send_iff_credit_nonzero :
    forall st,
      eqs.canSend st = true <-> st.credit ≠ 0
  receiver_valid_iff_occupancy_nonzero :
    forall st,
      eqs.receiverValid st = true <-> st.occupancy ≠ 0
  accounting_preserved :
    forall st send creditReturn,
      balanced inst st ->
      LegalStep st send creditReturn ->
      balanced inst (generatedStep eqs st send creditReturn)

theorem initial_balanced
    (inst : Instance) :
    balanced inst (initialState inst) := by
  simp [balanced, initialState]

theorem can_send_iff_credit_pos
    (st : State) :
    canSend st = true <-> 0 < st.credit := by
  simp [canSend]

theorem can_send_iff_credit_nonzero
    (st : State) :
    canSend st = true <-> st.credit ≠ 0 := by
  rw [can_send_iff_credit_pos]
  exact Nat.pos_iff_ne_zero

theorem receiver_valid_iff_occupancy_pos
    (st : State) :
    receiverValid st = true <-> 0 < st.occupancy := by
  simp [receiverValid]

theorem receiver_valid_iff_occupancy_nonzero
    (st : State) :
    receiverValid st = true <-> st.occupancy ≠ 0 := by
  rw [receiver_valid_iff_occupancy_pos]
  exact Nat.pos_iff_ne_zero

theorem accounting_sum_preserved
    (inst : Instance)
    (st : State)
    (send creditReturn : Bool)
    (h_sum : st.credit + st.occupancy = inst.depth)
    (h_legal : LegalStep st send creditReturn) :
    (step st send creditReturn).credit
      + (step st send creditReturn).occupancy
      = inst.depth := by
  cases send <;> cases creditReturn
  · simp [step, nextCredit, nextOccupancy]
    exact h_sum
  · simp [step, nextCredit, nextOccupancy]
    have h_return := h_legal.return_requires_occupancy rfl
    omega
  · simp [step, nextCredit, nextOccupancy]
    have h_send := h_legal.send_requires_credit rfl
    omega
  · simp [step, nextCredit, nextOccupancy]
    exact h_sum

theorem step_preserves_balance
    (inst : Instance)
    (st : State)
    (send creditReturn : Bool)
    (h_balanced : balanced inst st)
    (h_legal : LegalStep st send creditReturn) :
    balanced inst (step st send creditReturn) := by
  rcases h_balanced with ⟨h_credit_bound, h_occ_bound, h_sum⟩
  refine ⟨?_, ?_, accounting_sum_preserved inst st send creditReturn h_sum h_legal⟩
  · cases send <;> cases creditReturn
    · simp [step, nextCredit, nextOccupancy]
      exact h_credit_bound
    · simp [step, nextCredit, nextOccupancy]
      have h_return := h_legal.return_requires_occupancy rfl
      omega
    · simp [step, nextCredit, nextOccupancy]
      omega
    · simp [step, nextCredit, nextOccupancy]
      exact h_credit_bound
  · cases send <;> cases creditReturn
    · simp [step, nextCredit, nextOccupancy]
      exact h_occ_bound
    · simp [step, nextCredit, nextOccupancy]
      omega
    · simp [step, nextCredit, nextOccupancy]
      have h_send := h_legal.send_requires_credit rfl
      omega
    · simp [step, nextCredit, nextOccupancy]
      exact h_occ_bound

theorem generated_step_eq_step
    (eqs : Generated inst)
    (st : State)
    (send creditReturn : Bool) :
    generatedStep eqs st send creditReturn = step st send creditReturn := by
  unfold generatedStep step
  simp [eqs.next_credit_eq, eqs.next_occupancy_eq]

theorem generated_step_preserves_balance
    (inst : Instance)
    (eqs : Generated inst)
    (st : State)
    (send creditReturn : Bool)
    (h_balanced : balanced inst st)
    (h_legal : LegalStep st send creditReturn) :
    balanced inst (generatedStep eqs st send creditReturn) := by
  rw [generated_step_eq_step eqs st send creditReturn]
  exact step_preserves_balance inst st send creditReturn h_balanced h_legal

theorem parametric_proof
    (inst : Instance)
    (eqs : Generated inst)
    (h_depth : 0 < inst.depth)
    (h_width : 0 < inst.payloadWidth) :
    ParametricProof inst eqs := by
  refine
    { depth_pos := h_depth
      payload_width_pos := h_width
      equations :=
        { can_send_eq := eqs.can_send_eq
          receiver_valid_eq := eqs.receiver_valid_eq
          next_credit_eq := eqs.next_credit_eq
          next_occupancy_eq := eqs.next_occupancy_eq }
      can_send_iff_credit_nonzero := ?_
      receiver_valid_iff_occupancy_nonzero := ?_
      accounting_preserved := ?_ }
  · intro st
    rw [eqs.can_send_eq]
    exact can_send_iff_credit_nonzero st
  · intro st
    rw [eqs.receiver_valid_eq]
    exact receiver_valid_iff_occupancy_nonzero st
  · intro st send creditReturn h_balanced h_legal
    exact generated_step_preserves_balance inst eqs st send creditReturn h_balanced h_legal

theorem certificate_checks
    (inst : Instance)
    (eqs : Generated inst)
    (h_depth : 0 < inst.depth)
    (h_width : 0 < inst.payloadWidth) :
    0 < inst.depth
      /\ 0 < inst.payloadWidth
      /\ EquationsHold inst eqs
      /\ ParametricProof inst eqs := by
  exact
    ⟨h_depth, h_width,
      { can_send_eq := eqs.can_send_eq
        receiver_valid_eq := eqs.receiver_valid_eq
        next_credit_eq := eqs.next_credit_eq
        next_occupancy_eq := eqs.next_occupancy_eq },
      parametric_proof inst eqs h_depth h_width⟩

end Arch.ConstructProof.CreditChannel
