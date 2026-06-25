; SMT equivalence proof (doc/plan_fp_types.md §8.1).
;
; Property: the emitted FP32 comparison RTL (src/codegen/fp.rs:
;   arch_f32_eq / ne / lt / le / gt / ge) is bit-exactly equivalent to the
;   SMT-LIB `FloatingPoint` theory (IEEE-754) comparisons, for ALL 2^64 input
;   pairs. The `rtl_*` definitions below are a literal transcription of the
;   SystemVerilog bit-logic, so this proves the emitted RTL directly.
;
; Result: `unsat` (z3) ⇒ no counterexample ⇒ proven for all inputs.
; Chain: emitted SV ≡ fp.* (proved here) ≡ IEEE-754 (by the theory).
(set-logic QF_FPBV)
(define-sort BV () (_ BitVec 32))
(define-sort F () (_ FloatingPoint 8 24))
(define-fun isnan ((x BV)) Bool (and (= ((_ extract 30 23) x) #b11111111) (not (= ((_ extract 22 0) x) (_ bv0 23)))))
(define-fun iszero ((x BV)) Bool (= ((_ extract 30 0) x) (_ bv0 31)))
; ---- emitted-RTL logic, transcribed bit-for-bit from src/codegen/fp.rs ----
(define-fun rtl_eq ((a BV)(b BV)) Bool
  (ite (or (isnan a)(isnan b)) false (or (= a b) (and (iszero a)(iszero b)))))
(define-fun rtl_lt ((a BV)(b BV)) Bool
  (ite (or (isnan a)(isnan b)) false
  (ite (and (iszero a)(iszero b)) false
  (ite (distinct ((_ extract 31 31) a) ((_ extract 31 31) b)) (= ((_ extract 31 31) a) #b1)
  (ite (= ((_ extract 31 31) a) #b0) (bvult ((_ extract 30 0) a) ((_ extract 30 0) b))
       (bvugt ((_ extract 30 0) a) ((_ extract 30 0) b)))))))
(define-fun rtl_le ((a BV)(b BV)) Bool (or (rtl_lt a b) (rtl_eq a b)))
(define-fun rtl_gt ((a BV)(b BV)) Bool (rtl_lt b a))
(define-fun rtl_ge ((a BV)(b BV)) Bool (or (rtl_lt b a) (rtl_eq a b)))
(define-fun rtl_ne ((a BV)(b BV)) Bool (not (rtl_eq a b)))
(declare-fun a () BV)
(declare-fun b () BV)
(define-fun fa () F ((_ to_fp 8 24) a))
(define-fun fb () F ((_ to_fp 8 24) b))
; miter: RTL compare must equal the IEEE compare for every (a,b)
(assert (not (and
  (= (rtl_eq a b) (fp.eq  fa fb))
  (= (rtl_ne a b) (not (fp.eq fa fb)))
  (= (rtl_lt a b) (fp.lt  fa fb))
  (= (rtl_le a b) (fp.leq fa fb))
  (= (rtl_gt a b) (fp.gt  fa fb))
  (= (rtl_ge a b) (fp.geq fa fb)))))
(check-sat)
