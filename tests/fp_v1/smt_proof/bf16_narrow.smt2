; SMT equivalence proof (doc/plan_fp_types.md §8.1).
;
; Property: the emitted f32->bf16 narrow RTL (src/codegen/fp.rs:
;   arch_f32_to_bf16) is bit-exactly equivalent to IEEE-754 round-to-nearest-
;   even rounding of the f32 value into binary-bf16 (FloatingPoint 8 8), with a
;   canonical quiet NaN (0x7FC0), for ALL 2^32 inputs. `rtl_narrow` is a literal
;   transcription of the SystemVerilog bit-logic (guard/round/sticky bias trick).
;
; Result: `unsat` (z3) ⇒ proven for all inputs.
(set-logic QF_FPBV)
(define-sort BV () (_ BitVec 32))
(define-fun isnan32 ((x BV)) Bool (and (= ((_ extract 30 23) x) #b11111111) (not (= ((_ extract 22 0) x) (_ bv0 23)))))
; ---- emitted-RTL arch_f32_to_bf16, transcribed bit-for-bit ----
(define-fun rtl_narrow ((x BV)) (_ BitVec 16)
  (ite (isnan32 x) #x7FC0
    (let ((lsb ((_ extract 16 16) x)) (rbit ((_ extract 15 15) x))
          (sticky (ite (= ((_ extract 14 0) x) (_ bv0 15)) #b0 #b1)))
      (let ((roundup (bvand rbit (bvor sticky lsb))))
        ((_ extract 31 16) (bvadd x (ite (= roundup #b1) #x00010000 #x00000000)))))))
(declare-fun x () BV)
(define-fun fx   () (_ FloatingPoint 8 24) ((_ to_fp 8 24) x))
(define-fun spec () (_ FloatingPoint 8  8) ((_ to_fp 8 8) RNE fx))   ; round f32 value -> bf16, RNE
(define-fun rbf  () (_ BitVec 16) (rtl_narrow x))
(define-fun frbf () (_ FloatingPoint 8 8) ((_ to_fp 8 8) rbf))
; bit-exact: NaN -> canonical 0x7FC0, else same bf16 value (incl signed zero)
(assert (not (ite (fp.isNaN spec) (= rbf #x7FC0) (= frbf spec))))
(check-sat)
