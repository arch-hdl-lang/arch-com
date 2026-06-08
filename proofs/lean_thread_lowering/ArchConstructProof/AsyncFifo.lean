import Std

/-!
Reusable proof model for async FIFO / CDC FIFO pointer Gray-code behavior.

Async FIFOs commonly pass Gray-coded read/write pointers across clock domains so
that each source-domain increment exposes a single-bit transition to the
synchronizer.  This file starts with the backend-independent fact needed by
certificates: the generated Gray pointer retains the original binary pointer
value when decoded bit-by-bit.
-/

namespace Arch.ConstructProof.AsyncFifo

/-- Binary-to-Gray encoding for a fixed-width FIFO pointer. -/
def grayEncode {w : Nat} (ptr : BitVec w) : BitVec w :=
  ptr ^^^ (ptr >>> 1)

/--
Xor a finite little-endian suffix of a bit vector.

For a Gray-coded pointer, the decoded binary bit at position `start` is the xor
of Gray bits `[start, w)`.
-/
def suffixXor {w : Nat} (bits : BitVec w) : Nat -> Nat -> Bool
  | 0, _start => false
  | n + 1, start => bits.getLsbD start ^^ suffixXor bits n (start + 1)

/-- Decode one little-endian binary bit from a Gray-coded pointer. -/
def grayDecodeBit {w : Nat} (gray : BitVec w) (idx : Nat) : Bool :=
  suffixXor gray (w - idx) idx

/-- The Gray bit at `idx` is `ptr[idx] xor ptr[idx + 1]`. -/
theorem grayEncode_getLsbD
    {w : Nat}
    (ptr : BitVec w)
    (idx : Nat) :
    (grayEncode ptr).getLsbD idx =
      (ptr.getLsbD idx ^^ ptr.getLsbD (idx + 1)) := by
  unfold grayEncode
  rw [BitVec.getLsbD_xor, BitVec.getLsbD_ushiftRight]
  rw [Nat.add_comm]

/--
Telescoping lemma for Gray decoding.

If a decoded suffix spans exactly through the most-significant pointer bit, the
pairwise Gray xors cancel and leave the original binary bit at `idx`.
-/
theorem suffixXor_grayEncode_of_add_eq
    {w : Nat}
    (ptr : BitVec w) :
    forall (n idx : Nat),
      idx + n = w ->
      suffixXor (grayEncode ptr) n idx = ptr.getLsbD idx
  | 0, idx, hspan => by
      rw [suffixXor]
      rw [show idx = w by omega]
      exact (BitVec.getLsbD_of_ge ptr w (Nat.le_refl w)).symm
  | n + 1, idx, hspan => by
      rw [suffixXor, grayEncode_getLsbD]
      have hsuffix : idx + 1 + n = w := by omega
      rw [suffixXor_grayEncode_of_add_eq ptr n (idx + 1) hsuffix]
      rw [Bool.xor_assoc, Bool.xor_self, Bool.xor_false]

/--
Bit-level Gray decode roundtrip for any pointer width.

This is the main reusable async-FIFO certificate fact: every in-range bit of
`grayEncode ptr` decodes back to the corresponding bit of `ptr`.
-/
theorem gray_decode_bit_encode
    {w : Nat}
    (ptr : BitVec w)
    {idx : Nat}
    (hidx : idx < w) :
    grayDecodeBit (grayEncode ptr) idx = ptr.getLsbD idx := by
  unfold grayDecodeBit
  apply suffixXor_grayEncode_of_add_eq
  omega

/-- Gray encoding is injective for fixed-width pointers. -/
theorem grayEncode_injective
    {w : Nat}
    {lhs rhs : BitVec w}
    (hgray : grayEncode lhs = grayEncode rhs) :
    lhs = rhs := by
  apply BitVec.eq_of_getLsbD_eq
  intro idx hidx
  rw [← gray_decode_bit_encode lhs hidx]
  rw [← gray_decode_bit_encode rhs hidx]
  rw [hgray]

end Arch.ConstructProof.AsyncFifo
