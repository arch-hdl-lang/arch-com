//! Shared SV emission for `ExprKind::MethodCall` — the single source of
//! truth for `.trunc<N>()` / `.zext<N>()` / `.sext<N>()` / `.resize<N>()` /
//! `.reverse<C>()` / Vec reductions / float conversions.
//!
//! This match used to exist in three near-identical copies (the main
//! expression emitter in `codegen/mod.rs` plus the two pipeline expression
//! emitters in `codegen/pipeline.rs`), and a fix applied to one copy could
//! silently miss the siblings — that is exactly how the `resize`
//! self-determination miscompile fixed in mod.rs shipped on in the pipeline
//! emitters until PR #839. All three callers now dispatch here.
//!
//! The hosts differ in two ways, both threaded through explicitly:
//! - **Sub-expression emission**: the pipeline emitters rewrite names
//!   (stage regs get prefixed, `Stage.field` becomes `stage_field`), so the
//!   receiver string is produced by an `emit_sub` closure supplied by the
//!   caller, not by `emit_expr_str` directly.
//! - **Host-specific emission shapes**: a few arms intentionally emit
//!   different SV per host (see `MethodCallHost`). Those splits are
//!   preserved byte-for-byte from the pre-dedup copies — unifying any of
//!   them is a semantic change that needs its own verification, not a
//!   refactor.

use super::*;

/// Which expression emitter a method call is being emitted from.
///
/// Host-dependent arms (all preserved as-is from the pre-dedup copies):
/// - `zext`: `Main` wraps the receiver in `$unsigned(...)` before the size
///   cast; `Pipeline` emits the bare cast (same shape as `trunc`).
/// - `sext`: `Main` resolves the receiver width statically via
///   `infer_sv_width_str`; `Pipeline` cannot (stage-substituted receivers
///   are not resolvable through that path) and uses `$bits(...)` on the
///   emitted receiver instead.
/// - Float conversions (`to_fp32`, `to_bf16`, `to_fp8*`, `to_fp4*`,
///   `to_uint`, `to_sint`): emitted for `Main` only; `Pipeline` falls
///   through to the generic `.name()` default, as it always has.
#[derive(Copy, Clone, PartialEq, Eq)]
pub(super) enum MethodCallHost {
    /// The canonical module-code emitter (`emit_expr_str` in mod.rs).
    Main,
    /// Either pipeline emitter (`emit_pipeline_stage_expr_str` /
    /// `emit_pipeline_expr_str`); the two differ only in the closures the
    /// caller supplies, not in emission shape.
    Pipeline,
}

impl<'a> Codegen<'a> {
    /// Emit a method-call expression as an SV string.
    ///
    /// `emit_sub` produces the SV string for a sub-expression (the receiver,
    /// or the unwrapped receiver for `sext`) using the host's own emitter.
    /// `try_reverse_chunked` attempts the arch#808 portable chunked-concat
    /// lowering of `.reverse<C>()`; it receives `(receiver, chunk,
    /// emitted_receiver)` and returns `None` to keep the streaming-concat
    /// fallback. Width/chunk type arguments are compile-time constants and
    /// are always emitted with plain `emit_expr_str`, in every host.
    pub(super) fn emit_method_call_str(
        &self,
        base: &Expr,
        method: &Ident,
        args: &[Expr],
        host: MethodCallHost,
        emit_sub: &dyn Fn(&Expr) -> String,
        try_reverse_chunked: &dyn Fn(&Expr, &Expr, &str) -> Option<String>,
    ) -> String {
        let b = emit_sub(base);
        match method.name.as_str() {
            "trunc" => {
                if let Some(width) = args.first() {
                    let w = self.emit_expr_str(width);
                    // SV size cast: valid on any expression, including compound ones.
                    // e.g. (count_r + 1).trunc<8>() → 8'(count_r + 1)
                    let wp = Self::paren_width(&w);
                    format!("{wp}'({b})")
                } else {
                    b
                }
            }
            "zext" => {
                if let Some(width) = args.first() {
                    let w = self.emit_expr_str(width);
                    let wp = Self::paren_width(&w);
                    match host {
                        // $unsigned prevents context-dependent width expansion before the cast
                        MethodCallHost::Main => format!("{wp}'($unsigned({b}))"),
                        // Pipeline hosts have always emitted the bare size
                        // cast here (shared arm with `trunc` pre-dedup).
                        MethodCallHost::Pipeline => format!("{wp}'({b})"),
                    }
                } else {
                    b
                }
            }
            "sext" => {
                if let Some(width) = args.first() {
                    let w = self.emit_expr_str(width);
                    // Sign-extension: replicate the MSB into the upper
                    // bits. `.sext()` always treats the receiver's own
                    // MSB as the sign bit regardless of the receiver's
                    // declared signedness, so a `signed(...)`/
                    // `unsigned(...)`/`as T` wrapper around the
                    // receiver (e.g. `signed(raw).sext<16>()`,
                    // arch#650's "nested" mitigation example) is inert
                    // here — same bits either way. Unwrap it: besides
                    // being redundant, indexing straight into a cast
                    // result (`$signed(raw)[...]`) is exactly the
                    // "indexed cast" pattern arch#650 flags as
                    // Icarus-hostile, and `$bits($signed(raw))` (the
                    // prior width computation) nests a system-function
                    // call inside `$bits`, which Icarus also does not
                    // reliably accept.
                    let recv = Self::unwrap_reinterpret_cast(base);
                    let rb = emit_sub(recv);
                    let sw = match host {
                        MethodCallHost::Main => Self::paren_width(&self.infer_sv_width_str(recv)),
                        // Stage-substituted receivers can't be resolved by
                        // `infer_sv_width_str`; measure the emitted form.
                        MethodCallHost::Pipeline => format!("$bits({rb})"),
                    };
                    format!("{{{{({w}-{sw}){{{rb}[{sw}-1]}}}}, {rb}}}")
                } else {
                    b
                }
            }
            "resize" => {
                if let Some(width) = args.first() {
                    let w = self.emit_expr_str(width);
                    // Direction-agnostic resize: pads or truncates, preserving
                    // signedness. SV's `N'(expr)` size cast inherits the
                    // signedness of `expr` and — critically — forwards
                    // context-determined evaluation through arithmetic
                    // operators inside it. Earlier emission used
                    // `N'($signed(expr))` / `N'($unsigned(expr))`, but
                    // `$signed`/`$unsigned` evaluate their argument in
                    // self-determined context (LRM §11.6.1, §20.5), which
                    // truncates a multiply like `a * b` to operand width
                    // BEFORE the outer cast widens — silently losing the
                    // upper bits of any product. Dropping the wrapper lets
                    // `N'(a * b)` widen both operands to N before the
                    // multiply. For non-arithmetic `expr` (idents, slices),
                    // the cast still preserves signedness from the
                    // underlying declaration, so no behaviour changes.
                    let wp = Self::paren_width(&w);
                    format!("{wp}'({b})")
                } else {
                    b
                }
            }
            // as_clock removed — use `as Clock<Domain>` cast syntax // identity — 1-bit logic used as clock
            "reverse" => {
                if let Some(chunk) = args.first() {
                    // arch#808: Icarus rejects `{<<N{x}}` in every
                    // context; prefer the portable chunked-concat
                    // lowering whenever the receiver width resolves
                    // (typecheck guarantees const chunk + width, so
                    // the streaming fallback should be unreachable
                    // for checked input — kept for safety; pipeline
                    // hosts keep it for receivers whose width can't
                    // be resolved from the pipeline AST).
                    if let Some(s) = try_reverse_chunked(base, chunk, &b) {
                        s
                    } else {
                        let c = self.emit_expr_str(chunk);
                        format!("{{<<{c}{{{b}}}}}")
                    }
                } else {
                    b
                }
            }
            "any" | "all" | "count" | "contains" | "reduce_or" | "reduce_and" | "reduce_xor"
            | "find_first" => self.emit_vec_method(&b, base, method, args),
            // Float conversions → emitted helper functions. Main host only:
            // the pipeline emitters have always let these fall through to
            // the generic default below (see `MethodCallHost`).
            // `.to_e8m0()` extracts the binary exponent as an MX block
            // scale. Any float widens to f32 first; FP32 goes straight in.
            "to_e8m0" if host == MethodCallHost::Main => {
                self.fp_helpers_used.set(true);
                match self.expr_float_fmt(base) {
                    Some("f32") | None => format!("arch_f32_to_e8m0({b})"),
                    Some(t) => format!("arch_f32_to_e8m0(arch_{t}_to_f32({b}))"),
                }
            }
            "to_fp32" if host == MethodCallHost::Main => {
                self.fp_helpers_used.set(true);
                match self.expr_float_fmt(base) {
                    Some("bf16") => format!("arch_bf16_to_f32({b})"),
                    Some("e4m3") => format!("arch_e4m3_to_f32({b})"),
                    Some("e5m2") => format!("arch_e5m2_to_f32({b})"),
                    Some("e2m1") => format!("arch_e2m1_to_f32({b})"),
                    Some("e2m3") => format!("arch_e2m3_to_f32({b})"),
                    Some("e3m2") => format!("arch_e3m2_to_f32({b})"),
                    Some("f32") => b,
                    // E8M0 is a SCALE type, not a float format, so it has no
                    // float tag; dispatch on the declared type or it falls
                    // into the integer path below and widens the raw
                    // exponent code as an unsigned integer.
                    None if matches!(self.expr_decl_type(base), Some(TypeExpr::E8M0)) => {
                        format!("arch_e8m0_to_f32({b})")
                    }
                    _ => {
                        // int -> f32 (RNE) via the synthesizable helper.
                        if self.expr_is_signed(base) {
                            format!("arch_i64_to_f32(64'($signed({b})))")
                        } else {
                            format!("arch_u64_to_f32(64'($unsigned({b})))")
                        }
                    }
                }
            }
            "to_bf16" if host == MethodCallHost::Main => {
                self.fp_helpers_used.set(true);
                match self.expr_float_fmt(base) {
                    Some("f32") => format!("arch_f32_to_bf16({b})"),
                    Some("bf16") => b,
                    // fp8 -> bf16 is rejected by typecheck in v1
                    // (route via .to_fp32()); keep the arm total.
                    Some("e4m3") => format!("arch_f32_to_bf16(arch_e4m3_to_f32({b}))"),
                    Some("e5m2") => format!("arch_f32_to_bf16(arch_e5m2_to_f32({b}))"),
                    Some("e2m1") => format!("arch_f32_to_bf16(arch_e2m1_to_f32({b}))"),
                    Some("e2m3") => format!("arch_f32_to_bf16(arch_e2m3_to_f32({b}))"),
                    Some("e3m2") => format!("arch_f32_to_bf16(arch_e3m2_to_f32({b}))"),
                    _ => {
                        // int -> f32 (RNE) -> bf16 (RNE). DECLARED semantics
                        // (issue #629, resolved as f32-routed / VR(f32)):
                        // int.to_bf16() == narrow_bf16(f32(i)), matching the
                        // bf16 fma f32-accumulate convention (PR #627). This
                        // is a *double* rounding and is NOT correctly-rounded
                        // int->bf16 for |i| >= 2^24 — the f32 step can land
                        // exactly on a bf16 midpoint and tie-to-even the
                        // wrong way (witness i=16842753 -> 0x4b80, correctly
                        // rounded 0x4b81). Routing via f32 is hardware-
                        // realistic (no direct int->bf16 in RISC-V) and
                        // intended, not a bug — see doc/ARCH_HDL_Specification.md
                        // §3.8 "Rounding convention" and doc/proposal_fp_rounding_semantics.md.
                        // Sim mirror: src/sim_codegen _arch_{i,u}_to_bf16.
                        if self.expr_is_signed(base) {
                            format!("arch_f32_to_bf16(arch_i64_to_f32(64'($signed({b}))))")
                        } else {
                            format!("arch_f32_to_bf16(arch_u64_to_f32(64'($unsigned({b}))))")
                        }
                    }
                }
            }
            "to_fp8e4m3" | "to_fp8e5m2" | "to_fp4e2m1" | "to_fp6e2m3" | "to_fp6e3m2"
                if host == MethodCallHost::Main =>
            {
                self.fp_helpers_used.set(true);
                let (narrow, tgt) = match method.name.as_str() {
                    "to_fp8e4m3" => ("arch_f32_to_e4m3", "e4m3"),
                    "to_fp4e2m1" => ("arch_f32_to_e2m1", "e2m1"),
                    "to_fp6e2m3" => ("arch_f32_to_e2m3", "e2m3"),
                    "to_fp6e3m2" => ("arch_f32_to_e3m2", "e3m2"),
                    _ => ("arch_f32_to_e5m2", "e5m2"),
                };
                match self.expr_float_fmt(base) {
                    Some("f32") => format!("{narrow}({b})"),
                    Some(t) if t == tgt => b,
                    // BF16 / cross-fp8: exact widen, one narrow — CR.
                    Some("bf16") => format!("{narrow}(arch_bf16_to_f32({b}))"),
                    Some("e4m3") => format!("{narrow}(arch_e4m3_to_f32({b}))"),
                    Some("e5m2") => format!("{narrow}(arch_e5m2_to_f32({b}))"),
                    Some("e2m1") => format!("{narrow}(arch_e2m1_to_f32({b}))"),
                    Some("e2m3") => format!("{narrow}(arch_e2m3_to_f32({b}))"),
                    Some("e3m2") => format!("{narrow}(arch_e3m2_to_f32({b}))"),
                    // Integers: int -> f32 (exact for the fp8-relevant
                    // range, far below 2^24) -> one fp8 rounding — CR.
                    _ => {
                        if self.expr_is_signed(base) {
                            format!("{narrow}(arch_i64_to_f32(64'($signed({b}))))")
                        } else {
                            format!("{narrow}(arch_u64_to_f32(64'($unsigned({b}))))")
                        }
                    }
                }
            }
            "to_uint" | "to_sint" if host == MethodCallHost::Main => {
                self.fp_helpers_used.set(true);
                let width = args
                    .first()
                    .map(|w| self.emit_expr_str(w))
                    .unwrap_or_else(|| "32".to_string());
                let wp = Self::paren_width(&width);
                let f32bits = match self.expr_float_fmt(base) {
                    Some("bf16") => format!("arch_bf16_to_f32({b})"),
                    Some("e4m3") => format!("arch_e4m3_to_f32({b})"),
                    Some("e5m2") => format!("arch_e5m2_to_f32({b})"),
                    _ => b.clone(),
                };
                // Toward-zero, saturating to the N-bit target, NaN -> type max
                // (synthesizable helper; returns a 64-bit value truncated to N).
                let conv = if method.name == "to_sint" {
                    "arch_f32_to_sint"
                } else {
                    "arch_f32_to_uint"
                };
                format!("{wp}'({conv}({f32bits}, {width}))")
            }
            _ => format!("{b}.{}()", method.name),
        }
    }
}
