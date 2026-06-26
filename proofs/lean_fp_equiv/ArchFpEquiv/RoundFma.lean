import ArchFpEquiv.RoundProof
import Std.Tactic.BVDecide

/-! # Tier 2, fma — width-470 rounder instantiation (in progress) -/

namespace ArchFp

def round470_struct (s : BitVec 1) (sig : BitVec 470) (e0 : BitVec 16) : BitVec 32 :=
  if sig == 0#470 then s ++ 0#31
  else
    let p := arch_msb_index470 sig
    let ev := p + e0
    let biased := ev + 127#16
    let isSub := BitVec.sle biased 0#16
    let k := if isSub then 65387#16 else ev - 23#16
    let sh := k - e0
    let shLe0 := BitVec.sle sh 0#16
    let zsig := BitVec.setWidth 472 sig
    let kept0 := if shLe0 then zsig <<< (BitVec.setWidth 472 (0#16 - sh)).toNat
                 else zsig >>> (BitVec.setWidth 472 sh).toNat
    let shm1 := BitVec.setWidth 472 (sh - 1#16)
    let guardRaw := BitVec.extractLsb 0 0 (zsig >>> shm1.toNat)
    let guard := if shLe0 then 0#1 else guardRaw
    let mask := ((1#472) <<< shm1.toNat) - 1#472
    let stickyRaw := BitVec.ofBool (zsig &&& mask != 0#472)
    let sticky := if shLe0 then 0#1 else stickyRaw
    let lsb := BitVec.extractLsb 0 0 kept0
    let roundup := guard &&& (sticky ||| lsb)
    let kept := kept0 + BitVec.setWidth 472 roundup
    let subRes := (s ++ 0#31) ||| (s ++ BitVec.extractLsb 30 0 kept)
    let carry := BitVec.ofBool (BitVec.extractLsb 24 24 kept == 1#1)
    let biasedN := if carry == 1#1 then biased + 1#16 else biased
    let overflow := BitVec.sle 255#16 biasedN
    let infRes := s ++ ((0xFF#8) ++ (0#23))
    let keptN := if carry == 1#1 then kept >>> (BitVec.setWidth 472 (1#16)).toNat else kept
    let normRes := s ++ (BitVec.extractLsb 7 0 biasedN ++ BitVec.extractLsb 22 0 keptN)
    let nonSub := if overflow then infRes else normRes
    if isSub then subRes else nonSub


theorem arch_eq_struct470 (s : BitVec 1) (sig : BitVec 470) (e0 : BitVec 16) :
    arch_round470 s sig e0 = round470_struct s sig e0 := by
  unfold arch_round470 round470_struct arch_msb_index470
  bv_decide (config := { timeout := 600 })

end ArchFp
