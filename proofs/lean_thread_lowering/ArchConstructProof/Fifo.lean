import Std

/-!
Reusable proof model for first-class ARCH FIFO/LIFO constructs.

The model captures the abstract queue/stack step relation that the construct is
specified to implement.  Generated certificates instantiate the generic
theorem for concrete DEPTH/kind/type-width parameters.
-/

namespace Arch.ConstructProof.Fifo

inductive Kind where
  | fifo
  | lifo
deriving Repr, BEq

structure Instance where
  name : String
  kind : Kind
  depth : Nat
  dataWidth : Nat
  overflow : Bool
deriving Repr, BEq

def bounded (inst : Instance) (contents : List α) : Prop :=
  contents.length <= inst.depth

def ptrMod (inst : Instance) : Nat :=
  2 * inst.depth

def ptrIndex (inst : Instance) (ptr : Nat) : Nat :=
  ptr % inst.depth

def ptrOccupancy (inst : Instance) (wrPtr rdPtr : Nat) : Nat :=
  (wrPtr + ptrMod inst - rdPtr) % ptrMod inst

def updateMem (mem : Nat -> α) (idx : Nat) (data : α) : Nat -> α :=
  fun query => if query = idx then data else mem query

structure SyncGenerated (inst : Instance) (α : Type) where
  full : Nat -> Nat -> Bool
  empty : Nat -> Nat -> Bool
  pushReady : Nat -> Nat -> Bool
  popValid : Nat -> Nat -> Bool
  writeIndex : Nat -> Nat
  readIndex : Nat -> Nat
  nextWrPtr : Nat -> Bool -> Nat
  nextRdPtr : Nat -> Bool -> Nat
  nextMem : (Nat -> α) -> Nat -> α -> Bool -> Nat -> α
  full_eq :
    forall wrPtr rdPtr,
      full wrPtr rdPtr = (ptrOccupancy inst wrPtr rdPtr == inst.depth)
  empty_eq :
    forall wrPtr rdPtr,
      empty wrPtr rdPtr = (ptrOccupancy inst wrPtr rdPtr == 0)
  push_ready_eq :
    forall wrPtr rdPtr,
      pushReady wrPtr rdPtr = !(full wrPtr rdPtr)
  pop_valid_eq :
    forall wrPtr rdPtr,
      popValid wrPtr rdPtr = !(empty wrPtr rdPtr)
  write_index_eq :
    forall wrPtr,
      writeIndex wrPtr = ptrIndex inst wrPtr
  read_index_eq :
    forall rdPtr,
      readIndex rdPtr = ptrIndex inst rdPtr
  next_wr_ptr_eq :
    forall wrPtr doPush,
      nextWrPtr wrPtr doPush =
        if doPush then (wrPtr + 1) % ptrMod inst else wrPtr
  next_rd_ptr_eq :
    forall rdPtr doPop,
      nextRdPtr rdPtr doPop =
        if doPop then (rdPtr + 1) % ptrMod inst else rdPtr
  next_mem_eq :
    forall mem wrPtr data doPush,
      nextMem mem wrPtr data doPush =
        if doPush then updateMem mem (ptrIndex inst wrPtr) data else mem

structure SyncEquationsHold (inst : Instance) (eqs : SyncGenerated inst α) : Prop where
  full_eq :
    forall wrPtr rdPtr,
      eqs.full wrPtr rdPtr = (ptrOccupancy inst wrPtr rdPtr == inst.depth)
  empty_eq :
    forall wrPtr rdPtr,
      eqs.empty wrPtr rdPtr = (ptrOccupancy inst wrPtr rdPtr == 0)
  push_ready_eq :
    forall wrPtr rdPtr,
      eqs.pushReady wrPtr rdPtr = !(eqs.full wrPtr rdPtr)
  pop_valid_eq :
    forall wrPtr rdPtr,
      eqs.popValid wrPtr rdPtr = !(eqs.empty wrPtr rdPtr)
  write_index_eq :
    forall wrPtr,
      eqs.writeIndex wrPtr = ptrIndex inst wrPtr
  read_index_eq :
    forall rdPtr,
      eqs.readIndex rdPtr = ptrIndex inst rdPtr
  next_wr_ptr_eq :
    forall wrPtr doPush,
      eqs.nextWrPtr wrPtr doPush =
        if doPush then (wrPtr + 1) % ptrMod inst else wrPtr
  next_rd_ptr_eq :
    forall rdPtr doPop,
      eqs.nextRdPtr rdPtr doPop =
        if doPop then (rdPtr + 1) % ptrMod inst else rdPtr
  next_mem_eq :
    forall mem wrPtr data doPush,
      eqs.nextMem mem wrPtr data doPush =
        if doPush then updateMem mem (ptrIndex inst wrPtr) data else mem

structure LifoGenerated (inst : Instance) (α : Type) where
  full : Nat -> Bool
  empty : Nat -> Bool
  pushReady : Nat -> Bool
  popValid : Nat -> Bool
  writeIndex : Nat -> Bool -> Nat
  readIndex : Nat -> Nat
  nextSp : Nat -> Bool -> Bool -> Nat
  nextMem : (Nat -> α) -> Nat -> α -> Bool -> Bool -> Nat -> α
  full_eq :
    forall sp,
      full sp = (sp == inst.depth)
  empty_eq :
    forall sp,
      empty sp = (sp == 0)
  push_ready_eq :
    forall sp,
      pushReady sp = !(full sp)
  pop_valid_eq :
    forall sp,
      popValid sp = !(empty sp)
  write_index_eq :
    forall sp doPop,
      writeIndex sp doPop = if doPop then sp - 1 else sp
  read_index_eq :
    forall sp,
      readIndex sp = sp - 1
  next_sp_eq :
    forall sp doPush doPop,
      nextSp sp doPush doPop =
        if doPush && doPop then sp
        else if doPush then sp + 1
        else if doPop then sp - 1
        else sp
  next_mem_eq :
    forall mem sp data doPush doPop,
      nextMem mem sp data doPush doPop =
        if doPush then updateMem mem (if doPop then sp - 1 else sp) data else mem

structure LifoEquationsHold (inst : Instance) (eqs : LifoGenerated inst α) : Prop where
  full_eq :
    forall sp,
      eqs.full sp = (sp == inst.depth)
  empty_eq :
    forall sp,
      eqs.empty sp = (sp == 0)
  push_ready_eq :
    forall sp,
      eqs.pushReady sp = !(eqs.full sp)
  pop_valid_eq :
    forall sp,
      eqs.popValid sp = !(eqs.empty sp)
  write_index_eq :
    forall sp doPop,
      eqs.writeIndex sp doPop = if doPop then sp - 1 else sp
  read_index_eq :
    forall sp,
      eqs.readIndex sp = sp - 1
  next_sp_eq :
    forall sp doPush doPop,
      eqs.nextSp sp doPush doPop =
        if doPush && doPop then sp
        else if doPush then sp + 1
        else if doPop then sp - 1
        else sp
  next_mem_eq :
    forall mem sp data doPush doPop,
      eqs.nextMem mem sp data doPush doPop =
        if doPush then updateMem mem (if doPop then sp - 1 else sp) data else mem

def fifoStep (inst : Instance) (contents : List α) (push : Option α) (popReady : Bool) : List α :=
  let canPop := popReady && (0 < contents.length)
  let afterPop := if canPop then contents.drop 1 else contents
  (match push with
   | none => afterPop
   | some value =>
      if contents.length < inst.depth then
        afterPop ++ [value]
      else
        afterPop).take inst.depth

def lifoStep (inst : Instance) (contents : List α) (push : Option α) (popReady : Bool) : List α :=
  let canPop := popReady && (0 < contents.length)
  (match push with
   | some value =>
      if contents.length < inst.depth then
        if canPop then
          contents.dropLast ++ [value]
        else
          contents ++ [value]
      else if canPop then
        contents.dropLast
      else
        contents
   | none =>
      if canPop then contents.dropLast else contents).take inst.depth

def step (inst : Instance) (contents : List α) (push : Option α) (popReady : Bool) : List α :=
  match inst.kind with
  | Kind.fifo => fifoStep inst contents push popReady
  | Kind.lifo => lifoStep inst contents push popReady

theorem fifo_step_preserves_bound
    (inst : Instance)
    (contents : List α)
    (push : Option α)
    (popReady : Bool)
    (_hbounded : bounded inst contents) :
    bounded inst (fifoStep inst contents push popReady) := by
  unfold bounded fifoStep
  exact List.length_take_le _ _

theorem lifo_step_preserves_bound
    (inst : Instance)
    (contents : List α)
    (push : Option α)
    (popReady : Bool)
    (_hbounded : bounded inst contents) :
    bounded inst (lifoStep inst contents push popReady) := by
  unfold bounded lifoStep
  exact List.length_take_le _ _

theorem step_preserves_bound
    (inst : Instance)
    (contents : List α)
    (push : Option α)
    (popReady : Bool)
    (hbounded : bounded inst contents) :
    bounded inst (step inst contents push popReady) := by
  unfold step
  cases inst.kind with
  | fifo =>
      exact fifo_step_preserves_bound inst contents push popReady hbounded
  | lifo =>
      exact lifo_step_preserves_bound inst contents push popReady hbounded

theorem certificate_checks
    (inst : Instance)
    (h_depth : 0 < inst.depth)
    (h_width : 0 < inst.dataWidth) :
    0 < inst.depth
      /\ 0 < inst.dataWidth
      /\ forall (contents : List (BitVec inst.dataWidth)) (push : Option (BitVec inst.dataWidth)) popReady,
        bounded inst contents -> bounded inst (step inst contents push popReady) := by
  exact ⟨h_depth, h_width, step_preserves_bound inst⟩

theorem sync_certificate_checks
    (inst : Instance)
    (eqs : SyncGenerated inst (BitVec inst.dataWidth))
    (h_depth : 0 < inst.depth)
    (h_width : 0 < inst.dataWidth) :
    0 < inst.depth
      /\ 0 < inst.dataWidth
      /\ SyncEquationsHold inst eqs
      /\ forall (contents : List (BitVec inst.dataWidth)) (push : Option (BitVec inst.dataWidth)) popReady,
        bounded inst contents -> bounded inst (step inst contents push popReady) := by
  exact
    ⟨h_depth, h_width,
      { full_eq := eqs.full_eq
        empty_eq := eqs.empty_eq
        push_ready_eq := eqs.push_ready_eq
        pop_valid_eq := eqs.pop_valid_eq
        write_index_eq := eqs.write_index_eq
        read_index_eq := eqs.read_index_eq
        next_wr_ptr_eq := eqs.next_wr_ptr_eq
        next_rd_ptr_eq := eqs.next_rd_ptr_eq
        next_mem_eq := eqs.next_mem_eq },
      step_preserves_bound inst⟩

theorem lifo_certificate_checks
    (inst : Instance)
    (eqs : LifoGenerated inst (BitVec inst.dataWidth))
    (h_depth : 0 < inst.depth)
    (h_width : 0 < inst.dataWidth) :
    0 < inst.depth
      /\ 0 < inst.dataWidth
      /\ LifoEquationsHold inst eqs
      /\ forall (contents : List (BitVec inst.dataWidth)) (push : Option (BitVec inst.dataWidth)) popReady,
        bounded inst contents -> bounded inst (step inst contents push popReady) := by
  exact
    ⟨h_depth, h_width,
      { full_eq := eqs.full_eq
        empty_eq := eqs.empty_eq
        push_ready_eq := eqs.push_ready_eq
        pop_valid_eq := eqs.pop_valid_eq
        write_index_eq := eqs.write_index_eq
        read_index_eq := eqs.read_index_eq
        next_sp_eq := eqs.next_sp_eq
        next_mem_eq := eqs.next_mem_eq },
      step_preserves_bound inst⟩

end Arch.ConstructProof.Fifo
