//! SystemVerilog floating-point helpers — rendered from the shared bit-vector
//! IR (`crate::fp_ops` over `crate::fp_ir`).
//!
//! The operators are defined exactly once, in `src/fp_ops.rs`, and that single
//! source renders to BOTH this synthesizable SystemVerilog and the SMT-LIB2
//! equivalence model used by the §8.1 proofs — so the simulated/synthesized RTL
//! and the formally-checked model cannot drift (doc/plan_fp_types.md §8). The
//! `FpCompat` profile selects the canonical-NaN / NaN→int constants (§6.2).
//!
//! Emitted once at `$unit` scope ahead of the modules that use FP, gated by
//! `Codegen::fp_helpers_used`.

use crate::FpCompat;

/// The FP helper block for the given special-value profile, rendered from the
/// shared IR. Drop-in for the modules that call `arch_f32_*` / `arch_bf16_*`.
pub(super) fn fp_sv_helpers(profile: FpCompat) -> String {
    let mut s = String::from(
        "// ── arch floating-point helpers — generated from src/fp_ops.rs via the\n\
         // shared bit-vector IR (the same source emits the SMT-LIB equivalence\n\
         // model; see doc/plan_fp_types.md §8). Do not edit by hand. ──\n",
    );
    s.push_str(&crate::fp_ir::render_sv(&crate::fp_ops::fp_functions(profile)));
    s
}
