pub mod ast;
pub mod codegen;
pub mod comb_graph;
pub mod construct_formal_ir;
pub mod construct_proof_cert;
pub mod diagnostics;
pub mod elaborate;
pub mod formal;
pub mod fp_ir;
pub mod fp_lit;
pub mod fp_ops;
pub mod fp_smt_proof;
pub mod graph;
pub mod interface;
pub mod learn;
pub mod lexer;
pub mod parser;
pub mod pipelined_ops;
pub mod resolve;
pub mod signal_flow;
pub mod sim_codegen;
pub mod sim_credit_channel;
pub mod thread_map;
pub mod thread_proof_cert;
pub mod type_alias;
pub mod typecheck;
pub mod width;

/// Floating-point special-value compatibility profile (doc/plan_fp_types.md §6.2).
///
/// Both profiles share an identical IEEE-754 RNE arithmetic core, full subnormal
/// support, and toward-zero in-range float→int conversion. They differ only in
/// the two GPU-divergent corners:
///
/// | profile | canonical NaN (f32 / bf16) | NaN → int |
/// |---|---|---|
/// | `Riscv` (default) | `0x7FC00000` / `0x7FC0` | type max |
/// | `Cuda` | `0x7FFFFFFF` / `0x7FFF` | `0` |
///
/// Selected at compile time by `--fp-compat=riscv|cuda` on `arch build`/`sim`/
/// `formal`; honored identically by the sim and SV backends so they never
/// disagree. It is a thin output shim, NOT a second arithmetic path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FpCompat {
    #[default]
    Riscv,
    Cuda,
}

impl FpCompat {
    /// Parse the `--fp-compat` value.
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "riscv" => Ok(Self::Riscv),
            "cuda" => Ok(Self::Cuda),
            other => Err(format!(
                "--fp-compat: expected `riscv` or `cuda`, got `{other}`"
            )),
        }
    }
    pub fn is_cuda(self) -> bool {
        matches!(self, Self::Cuda)
    }
}
