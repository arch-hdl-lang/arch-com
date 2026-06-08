import Std

/-!
Reusable proof model for first-class ARCH pipeline constructs.

The model captures the structural valid/stall/flush behavior common to
generated pipelines.  Stages are represented as `Option α`: `none` is invalid
and `some data` is a valid payload.  Generated certificates can instantiate the
generic theorems for a concrete stage count and payload width.
-/

namespace Arch.ConstructProof.Pipeline

structure Instance where
  name : String
  stageCount : Nat
  dataWidth : Nat
deriving Repr, BEq

def StageIdx (inst : Instance) : Type :=
  Fin inst.stageCount

def State (inst : Instance) (α : Type) : Type :=
  StageIdx inst -> Option α

def ValidState (inst : Instance) : Type :=
  StageIdx inst -> Bool

structure Controls (inst : Instance) where
  stall : StageIdx inst -> Bool
  flush : StageIdx inst -> Bool

def previousIdx (inst : Instance) (idx : StageIdx inst) (h_nonzero : idx.val ≠ 0) : StageIdx inst :=
  ⟨idx.val - 1, by
    have h_pos : 0 < idx.val := Nat.pos_of_ne_zero h_nonzero
    omega⟩

def sourceValue (inst : Instance) (state : State inst α) (input : Option α) (idx : StageIdx inst) :
    Option α :=
  if h_zero : idx.val = 0 then
    input
  else
    state (previousIdx inst idx h_zero)

def sourceValid (inst : Instance) (valid : ValidState inst) (inputValid : Bool) (idx : StageIdx inst) :
    Bool :=
  if h_zero : idx.val = 0 then
    inputValid
  else
    valid (previousIdx inst idx h_zero)

def pipelineStep
    (inst : Instance)
    (controls : Controls inst)
    (state : State inst α)
    (input : Option α) :
    State inst α :=
  fun idx =>
    if controls.flush idx then
      none
    else if controls.stall idx then
      state idx
    else
      sourceValue inst state input idx

def validVector (inst : Instance) (state : State inst α) : ValidState inst :=
  fun idx => (state idx).isSome

def abstractValidStep
    (inst : Instance)
    (controls : Controls inst)
    (valid : ValidState inst)
    (inputValid : Bool) :
    ValidState inst :=
  fun idx =>
    if controls.flush idx then
      false
    else if controls.stall idx then
      valid idx
    else
      sourceValid inst valid inputValid idx

structure Generated (inst : Instance) (α : Type) where
  nextStage : Controls inst -> State inst α -> Option α -> StageIdx inst -> Option α
  next_stage_eq :
    forall controls state input idx,
      nextStage controls state input idx = pipelineStep inst controls state input idx

structure EquationsHold (inst : Instance) (eqs : Generated inst α) : Prop where
  next_stage_eq :
    forall controls state input idx,
      eqs.nextStage controls state input idx = pipelineStep inst controls state input idx

structure PipelineParametricProof (inst : Instance) (eqs : Generated inst α) : Prop where
  stage_count_pos : 0 < inst.stageCount
  data_width_pos : 0 < inst.dataWidth
  equations : EquationsHold inst eqs
  flush_clears_stage :
    forall controls (state : State inst α) (input : Option α) idx,
      controls.flush idx = true ->
        pipelineStep inst controls state input idx = none
  stalled_stage_holds :
    forall controls (state : State inst α) (input : Option α) idx,
      controls.flush idx = false ->
        controls.stall idx = true ->
          pipelineStep inst controls state input idx = state idx
  unstalled_stage_advances :
    forall controls (state : State inst α) (input : Option α) idx,
      controls.flush idx = false ->
        controls.stall idx = false ->
          pipelineStep inst controls state input idx = sourceValue inst state input idx
  valid_step_matches_abstract :
    forall controls (state : State inst α) (input : Option α) idx,
      (pipelineStep inst controls state input idx).isSome =
        abstractValidStep inst controls (validVector inst state) input.isSome idx

theorem source_value_is_some_matches_source_valid
    (inst : Instance)
    (state : State inst α)
    (input : Option α)
    (idx : StageIdx inst) :
    (sourceValue inst state input idx).isSome =
      sourceValid inst (validVector inst state) input.isSome idx := by
  unfold sourceValue sourceValid validVector
  split <;> rfl

theorem flush_clears_stage
    (inst : Instance)
    (controls : Controls inst)
    (state : State inst α)
    (input : Option α)
    (idx : StageIdx inst)
    (h_flush : controls.flush idx = true) :
    pipelineStep inst controls state input idx = none := by
  unfold pipelineStep
  simp [h_flush]

theorem flush_clears_valid
    (inst : Instance)
    (controls : Controls inst)
    (state : State inst α)
    (input : Option α)
    (idx : StageIdx inst)
    (h_flush : controls.flush idx = true) :
    (pipelineStep inst controls state input idx).isSome = false := by
  rw [flush_clears_stage inst controls state input idx h_flush]
  rfl

theorem stalled_stage_holds
    (inst : Instance)
    (controls : Controls inst)
    (state : State inst α)
    (input : Option α)
    (idx : StageIdx inst)
    (h_no_flush : controls.flush idx = false)
    (h_stall : controls.stall idx = true) :
    pipelineStep inst controls state input idx = state idx := by
  unfold pipelineStep
  simp [h_no_flush, h_stall]

theorem unstalled_stage_advances
    (inst : Instance)
    (controls : Controls inst)
    (state : State inst α)
    (input : Option α)
    (idx : StageIdx inst)
    (h_no_flush : controls.flush idx = false)
    (h_no_stall : controls.stall idx = false) :
    pipelineStep inst controls state input idx = sourceValue inst state input idx := by
  unfold pipelineStep
  simp [h_no_flush, h_no_stall]

theorem valid_step_matches_abstract
    (inst : Instance)
    (controls : Controls inst)
    (state : State inst α)
    (input : Option α)
    (idx : StageIdx inst) :
    (pipelineStep inst controls state input idx).isSome =
      abstractValidStep inst controls (validVector inst state) input.isSome idx := by
  unfold pipelineStep abstractValidStep
  by_cases h_flush : controls.flush idx = true
  · rw [h_flush]
    rfl
  · have h_flush_false : controls.flush idx = false := by
      cases h : controls.flush idx <;> simp [h] at h_flush ⊢
    rw [h_flush_false]
    by_cases h_stall : controls.stall idx = true
    · rw [h_stall]
      rfl
    · have h_stall_false : controls.stall idx = false := by
        cases h : controls.stall idx <;> simp [h] at h_stall ⊢
      rw [h_stall_false]
      exact source_value_is_some_matches_source_valid inst state input idx

theorem generated_step_refines_model
    (inst : Instance)
    (eqs : Generated inst α)
    (controls : Controls inst)
    (state : State inst α)
    (input : Option α)
    (idx : StageIdx inst) :
    eqs.nextStage controls state input idx = pipelineStep inst controls state input idx := by
  exact eqs.next_stage_eq controls state input idx

theorem parametric_proof
    (inst : Instance)
    (eqs : Generated inst α)
    (h_stage_count : 0 < inst.stageCount)
    (h_width : 0 < inst.dataWidth) :
    PipelineParametricProof inst eqs := by
  refine
    { stage_count_pos := h_stage_count
      data_width_pos := h_width
      equations := { next_stage_eq := eqs.next_stage_eq }
      flush_clears_stage := ?_
      stalled_stage_holds := ?_
      unstalled_stage_advances := ?_
      valid_step_matches_abstract := ?_ }
  · intro controls state input idx h_flush
    exact flush_clears_stage inst controls state input idx h_flush
  · intro controls state input idx h_no_flush h_stall
    exact stalled_stage_holds inst controls state input idx h_no_flush h_stall
  · intro controls state input idx h_no_flush h_no_stall
    exact unstalled_stage_advances inst controls state input idx h_no_flush h_no_stall
  · intro controls state input idx
    exact valid_step_matches_abstract inst controls state input idx

theorem certificate_checks
    (inst : Instance)
    (eqs : Generated inst (BitVec inst.dataWidth))
    (h_stage_count : 0 < inst.stageCount)
    (h_width : 0 < inst.dataWidth) :
    0 < inst.stageCount
      /\ 0 < inst.dataWidth
      /\ EquationsHold inst eqs
      /\ PipelineParametricProof inst eqs := by
  exact
    ⟨h_stage_count, h_width,
      { next_stage_eq := eqs.next_stage_eq },
      parametric_proof inst eqs h_stage_count h_width⟩

end Arch.ConstructProof.Pipeline
