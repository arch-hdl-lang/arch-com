; SMT equivalence proof (doc/plan_fp_types.md §8.1).
;
; Property: the emitted bf16->f32 widen RTL (src/codegen/fp.rs:
;   arch_bf16_to_f32 = canon({h, 16'b0})) is bit-exactly equal to the EXACT
;   widening of the bf16 value into f32 (bf16 ⊂ f32), with canonical NaN, for
;   ALL 2^16 inputs. `rtl_widen` is a literal transcription of the SV.
;
; Result: `unsat` (z3) ⇒ proven for all inputs.
(set-logic QF_FPBV)
(define-fun isnan32 ((x (_ BitVec 32))) Bool (and (= ((_ extract 30 23) x) #b11111111) (not (= ((_ extract 22 0) x) (_ bv0 23)))))
; ---- emitted-RTL arch_bf16_to_f32, transcribed bit-for-bit ----
(define-fun rtl_widen ((h (_ BitVec 16))) (_ BitVec 32)
  (let ((z (concat h (_ bv0 16)))) (ite (isnan32 z) #x7FC00000 z)))
(declare-fun h () (_ BitVec 16))
(define-fun spec () (_ FloatingPoint 8 24) ((_ to_fp 8 24) RNE ((_ to_fp 8 8) h)))  ; widen bf16 value -> f32 (exact)
(define-fun rf   () (_ BitVec 32) (rtl_widen h))
(assert (not (ite (fp.isNaN spec) (= rf #x7FC00000) (= ((_ to_fp 8 24) rf) spec))))
(check-sat)
