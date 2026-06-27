import ArchFpEquiv.RoundProof

/-!
# Independent ground-truth check of the RNE spec

`roundNE_f32` is the *reference* `arch_round48` is proved against
(`Equiv.arch_round48_correct`), so nothing in the equivalence proof can catch a
bug in the spec itself — a wrong spec would make the theorem vacuously "correct
against a wrong reference". These `#guard`s pin `roundNE_f32` to externally-known
IEEE-754 binary32 bit patterns (elaboration fails if any is wrong), covering
normals, signs, ±0, the subnormal range and its normal boundary, ties-to-even in
both directions, rounding carry-out (mantissa overflow bumping the exponent),
overflow to ±∞, and the max finite value. They run as part of `lake build`.
-/

namespace ArchFp
-- value = (-1)^neg * sig * 2^e0.   roundNE_f32 neg sig e0  should give the f32 bits.

-- 1.0  = 0x3F800000   (sig=1, e0=0)
#guard roundNE_f32 false 1 0    = 0x3F800000#32
-- -1.0 = 0xBF800000
#guard roundNE_f32 true  1 0    = 0xBF800000#32
-- 2.0  = 0x40000000   (sig=1, e0=1)
#guard roundNE_f32 false 1 1    = 0x40000000#32
-- 0.5  = 0x3F000000   (sig=1, e0=-1)
#guard roundNE_f32 false 1 (-1) = 0x3F000000#32
-- 3.0  = 0x40400000   (sig=3, e0=0)
#guard roundNE_f32 false 3 0    = 0x40400000#32
-- 0.0  = 0x00000000   (sig=0)
#guard roundNE_f32 false 0 5    = 0x00000000#32
-- -0.0 = 0x80000000
#guard roundNE_f32 true  0 5    = 0x80000000#32
-- smallest positive subnormal 2^-149 = 0x00000001  (sig=1, e0=-149)
#guard roundNE_f32 false 1 (-149) = 0x00000001#32
-- largest subnormal (2^23-1)*2^-149 = 0x007FFFFF  (sig=2^23-1, e0=-149)
#guard roundNE_f32 false (2^23-1) (-149) = 0x007FFFFF#32
-- smallest normal 2^-126 = 0x00800000  (sig=2^23, e0=-149)
#guard roundNE_f32 false (2^23) (-149) = 0x00800000#32
-- ties-to-even: (2^24+1)*2^0 rounds to 2^24 (even) -> exp 151, mant 0 = 0x4B800000
#guard roundNE_f32 false (2^24+1) 0 = 0x4B800000#32
-- ties-to-even up: (2^24+3) -> nearest even is 2^24+4 -> mantissa 2 = 0x4B800002
#guard roundNE_f32 false (2^24+3) 0 = 0x4B800002#32
-- round up away (not tie): (2^24+3) checked; (2^25+3)*... ; use 2^24+ (0b11)
-- overflow to +inf: 2^200 -> 0x7F800000
#guard roundNE_f32 false 1 200 = 0x7F800000#32
-- overflow to -inf
#guard roundNE_f32 true  1 200 = 0xFF800000#32
-- max finite normal: (2^24-1)*2^104 = 0x7F7FFFFF  (largest finite f32)
#guard roundNE_f32 false (2^24-1) 104 = 0x7F7FFFFF#32
-- carry on rounding: (2^25-1)*2^0 = 33554431 rounds up to 2^25 -> exp bumps, mant 0
--   log2=24, sh=1, rneQuot(2^25-1,1)= (2^25-1)/2=2^24-1 + roundup(guard=1,lsb of q...) 
--   (2^25-1)=0b1..1 (25 ones); drop 1 bit: q=2^24-1, r=1=half, q odd -> round up -> 2^24 (carry!)
--   biased 24+127=151, carry -> 152, mant (2^24/2=2^23)%2^23=0 => exp 152 mant 0 = 0x4C000000
#guard roundNE_f32 false (2^25-1) 0 = 0x4C000000#32
#eval "all spec sanity checks passed"
end ArchFp
