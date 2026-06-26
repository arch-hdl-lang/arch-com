// Utility: render the FP helpers (shared IR) to SV / SMT, or emit a generated
// SMT equivalence proof for an operator.
//
//   dump_fp            -> SystemVerilog (riscv profile)
//   dump_fp smt        -> SMT-LIB2 define-funs
//   dump_fp lean       -> Lean 4 BitVec defs (model for the structured-proof backend)
//   dump_fp proof OP   -> full proof (define-funs + miter) for OP; z3 -> unsat

fn main() {
    let p = arch::FpCompat::Riscv;
    let mode = std::env::args().nth(1).unwrap_or_default();
    match mode.as_str() {
        "smt" => print!("{}", arch::fp_ir::render_smt(&arch::fp_ops::fp_functions(p))),
        "lean" => {
            print!("namespace ArchFp\n\n");
            print!("{}", arch::fp_ir::render_lean(&arch::fp_ops::fp_functions(p)));
            print!("\nend ArchFp\n");
        }
        "proof" => {
            let op = std::env::args().nth(2).expect("proof OP");
            print!("{}", arch::fp_smt_proof::equiv_proof(&op, p));
        }
        _ => print!("{}", arch::fp_ir::render_sv(&arch::fp_ops::fp_functions(p))),
    }
}
