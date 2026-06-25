; SMT equivalence proof (doc/plan_fp_types.md §8.1).
;
; Property: the emitted float->uint RTL (src/codegen/fp.rs: arch_f32_to_uint,
;   here at N=32) computes the toward-zero conversion bit-exactly, equal to the
;   IEEE-754 `fp.to_ubv` (RTZ) partial function for in-range inputs. Decode +
;   variable-shift + negative->0 + saturate logic transcribed literally.
;
; Scope (per §8.1): `fp.to_ubv` is partial (undefined for NaN / out-of-range /
;   negative), so this proves the in-range non-negative cases; saturation and
;   NaN->uint-max corners are signed off by the §8.2 differential campaign.
;
; Result: `unsat` (z3) ⇒ proven for all in-range inputs.
(set-logic QF_FPBV)
(define-sort BV32 () (_ BitVec 32))
(define-fun isnan32 ((x BV32)) Bool (and (= ((_ extract 30 23) x) #b11111111) (not (= ((_ extract 22 0) x) (_ bv0 23)))))
(define-fun isinf32 ((x BV32)) Bool (and (= ((_ extract 30 23) x) #b11111111) (= ((_ extract 22 0) x) (_ bv0 23))))
(define-fun iszero32 ((x BV32)) Bool (= ((_ extract 30 0) x) (_ bv0 31)))
(define-fun mant24 ((x BV32)) (_ BitVec 24)
  (ite (= ((_ extract 30 23) x) #b00000000) (concat #b0 ((_ extract 22 0) x)) (concat #b1 ((_ extract 22 0) x))))
(define-fun eunb128 ((x BV32)) (_ BitVec 128)
  (ite (= ((_ extract 30 23) x) #b00000000) (bvneg (_ bv149 128)) (bvsub ((_ zero_extend 120) ((_ extract 30 23) x)) (_ bv150 128))))
(define-fun magf ((x BV32)) (_ BitVec 128)
  (let ((m ((_ zero_extend 104) (mant24 x))) (e (eunb128 x)))
    (ite (bvsge e (_ bv64 128)) (bvnot (_ bv0 128))
    (ite (bvsge e (_ bv0 128))  (bvshl m e)
         (let ((sh (bvneg e))) (ite (bvuge sh (_ bv128 128)) (_ bv0 128) (bvlshr m sh)))))))
(define-fun limu () (_ BitVec 128) (bvsub (bvshl (_ bv1 128) (_ bv32 128)) (_ bv1 128)))
(define-fun rtl_to_uint32 ((x BV32)) (_ BitVec 64)
  (ite (isnan32 x) ((_ zero_extend 32) #xFFFFFFFF)
  (ite (iszero32 x) (_ bv0 64)
  (ite (= ((_ extract 31 31) x) #b1) (_ bv0 64)
  (ite (isinf32 x) ((_ extract 63 0) limu)
  (let ((mag (magf x))) (ite (bvugt mag limu) ((_ extract 63 0) limu) ((_ extract 63 0) mag))))))))
(declare-fun x () BV32)
(define-fun fx   () (_ FloatingPoint 8 24) ((_ to_fp 8 24) x))
(define-fun spec () (_ BitVec 64) ((_ fp.to_ubv 64) RTZ fx))
(define-fun inrange () Bool (and (not (fp.isNaN fx)) (not (fp.isInfinite fx))
   (fp.geq fx ((_ to_fp 8 24) RNE 0.0)) (fp.lt fx ((_ to_fp 8 24) RNE 4294967296.0))))
(assert inrange)
(assert (not (= ((_ zero_extend 32) ((_ extract 31 0) (rtl_to_uint32 x))) spec)))
(check-sat)
