//! Block-scaled (`ScaledVec`) operations — the `scaled_quantize` /
//! `scaled_dequantize` / `scaled_dot` lowering shared by `arch build` and
//! `arch sim`.
//!
//! **Why one module emits both backends.** The numerics here are *not* new:
//! every float operation used below is an existing helper with an exhaustive
//! SMT miter behind it (`arch_e8m0_to_f32`, `arch_f32_to_e8m0`,
//! `arch_f32_mul`, `arch_f32_to_<elem>`, `arch_<elem>_to_f32` — see
//! `src/fp_ops.rs` and `src/fp_smt_proof.rs`). What is new is the *glue*:
//! a magnitude reduction, an exponent subtraction, and the bit packing. Glue
//! is exactly where an SV emitter and a C++ emitter written in different
//! files drift, so both renderings live here, adjacent, generated from one
//! descriptor. `fp_block_sv_and_cpp_agree_on_shape` pins their structure and
//! the `tests/fp_v1/rtl_diff` harness pins their *values* by running the
//! emitted SV under Verilator against this same C++ as the DPI reference.
//!
//! **The glue carries no float semantics of its own.** Two deliberate
//! choices make that true:
//!
//! 1. The block maximum is an **unsigned integer** max over the sign-cleared
//!    words. For non-negative IEEE-754, bit-pattern order *is* numeric order,
//!    and every NaN sorts above `+Inf` — so one integer compare yields both
//!    the magnitude maximum and the "any non-finite in this block" test, with
//!    no float compare and therefore no NaN-ordering subtlety to get wrong.
//! 2. The per-element division by the shared scale is done as a multiply by
//!    `2^-(code-127) == 2^((254-code)-127)`, i.e. by *another E8M0 code*.
//!    A power-of-two multiply is exact in FP32, so the element narrow that
//!    follows rounds exactly once, from the true `v_i / X`. That single
//!    rounding is what makes the round-trip error bound provable (arch#884).
//!
//! `scaled_dot` (phase 3) rests on the same principle from the other side:
//! every element-pair product is *exact* in FP32 — machine-checked
//! exhaustively per format by `fp_smt_proof::MX_DOT` — so all of a block dot's
//! rounding is in the summation. That is what makes [`dot_schedule`]'s choice
//! of accumulation order the only implementation freedom left, and therefore
//! worth defining rather than leaving open.
//!
//! Layout is the one fixed in phase 2a (`fp_format::scaled_vec_width`):
//! `{ scale[SW-1:0], P[N-1], …, P[1], P[0] }` — scale in the high bits,
//! element `i` at `[i*EW +: EW]`.

use crate::ast::{RoundMode, ScalePolicy, TypeExpr};
use crate::fp_format::{by_type_expr, FpFormatId};
use std::fmt::Write as _;

/// The scale format of a block. One variant today; `UE4M3` (NVFP4) joins it
/// in phase 5. An enum rather than a bool so that addition forces every
/// `match` here to be revisited — the shared-exponent arithmetic below is
/// specific to E8M0's "same bias as FP32, no zero, `0xFF` is NaN" shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BlockScale {
    E8m0,
}

impl BlockScale {
    fn tag(self) -> &'static str {
        match self {
            BlockScale::E8m0 => "e8m0",
        }
    }
    fn width(self) -> u32 {
        match self {
            BlockScale::E8m0 => 8,
        }
    }
}

/// A block's static shape: what `scaled_dequantize` needs, and the part of
/// `scaled_quantize` that is not a policy knob.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct BlockShape {
    pub elem: FpFormatId,
    pub n: u32,
    pub scale: BlockScale,
}

/// One emitted helper. Ordered so the emitters can keep a `BTreeSet` and
/// produce byte-identical output run to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BlockHelper {
    /// `Vec<FP32, N>` → block.
    Quantize {
        shape: BlockShape,
        policy: ScalePolicy,
        round: RoundMode,
    },
    /// Block → `Vec<FP32, N>`.
    Dequantize { shape: BlockShape },
    /// Block · block → `FP32`. The one operation OCP MX defines normatively
    /// (§6.2): `X^A · X^B · Σᵢ(Pᵢ^A × Pᵢ^B)`.
    Dot { shape: BlockShape },
}

/// Resolve a `ScaledVec<Elem, N, Scale>` surface type to a block shape.
///
/// Returns `None` for anything the block predicates reject, so a caller can
/// never silently proceed on a half-understood type — the failure mode that
/// turned `ScaledVec<UInt<8>,4,E8M0>` into a 40-bit SV port against an 8-bit
/// sim variable in phase 2a.
pub fn shape_of(elem: &TypeExpr, n: u32, scale: &TypeExpr) -> Option<BlockShape> {
    let elem = by_type_expr(elem)?.id;
    if !matches!(
        elem,
        FpFormatId::E2m1
            | FpFormatId::E2m3
            | FpFormatId::E3m2
            | FpFormatId::E4m3
            | FpFormatId::E5m2
    ) {
        return None;
    }
    let scale = match scale {
        TypeExpr::E8M0 => BlockScale::E8m0,
        _ => return None,
    };
    Some(BlockShape { elem, n, scale })
}

/// Resolve a `ScaledVec<Elem, N, Scale>` **TypeExpr** to a block shape.
///
/// The one resolver both backends use. It was briefly duplicated per emitter,
/// which is how a block ends up sized one way in the SV and another in the
/// sim; there is nothing here worth having two copies of.
///
/// `None` — never a guess — when the type is not a block, `N` does not fold to
/// a literal, or a member type is not a legal block member. Callers turn that
/// into a panic: typecheck has already accepted the type, so a `None` is a
/// compiler bug rather than a user error.
pub fn shape_of_type(ty: &TypeExpr) -> Option<BlockShape> {
    let TypeExpr::ScaledVec(elem, size, scale) = ty else {
        return None;
    };
    let n = match &size.kind {
        crate::ast::ExprKind::Literal(crate::ast::LitKind::Dec(n))
        | crate::ast::ExprKind::Literal(crate::ast::LitKind::Hex(n)) => *n as u32,
        _ => return None,
    };
    shape_of(elem, n, scale)
}

impl BlockShape {
    fn desc(self) -> &'static crate::fp_format::FpFormat {
        crate::fp_format::FORMATS
            .iter()
            .find(|f| f.id == self.elem)
            .expect("FpFormatId always has a row — fp_format::FORMATS is total over the enum")
    }
    fn elem_tag(self) -> &'static str {
        self.desc().tag
    }
    fn elem_width(self) -> u32 {
        self.desc().width
    }
    /// Total packed width, `scale_w + N * elem_w`.
    pub fn bits(self) -> u32 {
        self.scale.width() + self.n * self.elem_width()
    }
    /// Width of the `Vec<FP32, N>` side, packed.
    fn vec_bits(self) -> u32 {
        self.n * 32
    }
    /// The element format's top normal binade exponent: `floor(log2(max))`.
    ///
    /// Derived from `max_finite` rather than hand-tabled, so it cannot drift
    /// from the format table. E2M1's max is `6.0 = 1.5 * 2^2` → 2; E5M2's is
    /// `57344 = 1.75 * 2^15` → 15. This is the amount the block's shared
    /// exponent is shifted down by, so that the largest element in the block
    /// normalizes into the format's top binade (OCP §6.3).
    fn elem_emax(self) -> u32 {
        let m = self.desc().max_finite;
        debug_assert!(m > 0.0 && m.is_finite());
        m.log2().floor() as u32
    }
}

impl BlockHelper {
    fn shape(self) -> BlockShape {
        match self {
            BlockHelper::Quantize { shape, .. }
            | BlockHelper::Dequantize { shape }
            | BlockHelper::Dot { shape } => shape,
        }
    }

    /// The SystemVerilog function name. The C++ name is this with a leading
    /// underscore, matching the existing `arch_f32_mul` / `_arch_f32_mul`
    /// convention.
    pub fn sv_name(self) -> String {
        let s = self.shape();
        match self {
            BlockHelper::Quantize { policy, round, .. } => format!(
                "arch_scaled_quantize_{}_{}_{}_{}_{}",
                s.elem_tag(),
                s.n,
                s.scale.tag(),
                policy_tag(policy),
                round_tag(round),
            ),
            BlockHelper::Dequantize { .. } => format!(
                "arch_scaled_dequantize_{}_{}_{}",
                s.elem_tag(),
                s.n,
                s.scale.tag(),
            ),
            BlockHelper::Dot { .. } => {
                format!("arch_scaled_dot_{}_{}_{}", s.elem_tag(), s.n, s.scale.tag(),)
            }
        }
    }

    pub fn cpp_name(self) -> String {
        format!("_{}", self.sv_name())
    }
}

fn policy_tag(p: ScalePolicy) -> &'static str {
    match p {
        ScalePolicy::FloorPow2 => "floor",
        ScalePolicy::CeilPow2 => "ceil",
    }
}

fn round_tag(r: RoundMode) -> &'static str {
    match r {
        RoundMode::Rne => "rne",
        RoundMode::Rtz => "rtz",
        RoundMode::Rna => "rna",
    }
}

/// The summation schedule for a block dot product of `n` elements.
///
/// **This is a definition, not a derivation.** OCP MX §6.2 fixes the dot's
/// *value* as `X^A · X^B · Σᵢ(Pᵢ^A × Pᵢ^B)` but leaves the accumulation
/// implementation-defined — and FP32 addition is not associative, so an
/// unstated order is an unstated result. ARCH defines it here, once, and both
/// backends render this same schedule.
///
/// The order is **balanced pairwise**: each round adds adjacent pairs, a lone
/// trailing value passes through untouched, repeat until one remains. Chosen
/// over a running serial accumulator because it is what a dot-product datapath
/// synthesizes to anyway (⌈log₂ N⌉ deep rather than N), and because pairwise
/// summation has a strictly smaller error bound — `O(log N)` growth against
/// serial's `O(N)`.
///
/// Temps `0..n` are the element-pair products; each returned `(l, r)` defines
/// the next temp in order, so temp `n + k` is `adds[k]`. Returns the adds and
/// the index of the final temp (which is `0` when `n == 1`: no adds at all).
fn dot_schedule(n: u32) -> (Vec<(usize, usize)>, usize) {
    let mut cur: Vec<usize> = (0..n as usize).collect();
    let mut adds: Vec<(usize, usize)> = Vec::new();
    let mut next_id = n as usize;
    while cur.len() > 1 {
        let mut nxt = Vec::with_capacity(cur.len().div_ceil(2));
        let mut i = 0;
        while i + 1 < cur.len() {
            adds.push((cur[i], cur[i + 1]));
            nxt.push(next_id);
            next_id += 1;
            i += 2;
        }
        if i < cur.len() {
            // Odd count: the last value skips this round rather than being
            // paired with an implicit zero. Adding zero is not a no-op in
            // FP32 (it turns -0.0 into +0.0), so a "harmless" pad would be a
            // real value change.
            nxt.push(cur[i]);
        }
        cur = nxt;
    }
    (adds, cur[0])
}

/// `[hi:0]` in the `logic [hi:0] ` declaration style `render_sv` uses.
fn sv_w(bits: u32) -> String {
    if bits == 1 {
        String::new()
    } else {
        format!("[{}:0] ", bits - 1)
    }
}

// ── SystemVerilog ───────────────────────────────────────────────────────────

/// The `$unit`-scope SV definition. Emitted alongside `fp::fp_sv_helpers`,
/// which supplies every `arch_*` callee referenced here.
pub fn sv_definition(h: BlockHelper) -> String {
    match h {
        BlockHelper::Quantize {
            shape,
            policy,
            round,
        } => sv_quantize(shape, policy, round),
        BlockHelper::Dequantize { shape } => sv_dequantize(shape),
        BlockHelper::Dot { shape } => sv_dot(shape),
    }
}

/// `scaled_dot` — OCP MX §6.2's `X^A · X^B · Σᵢ(Pᵢ^A × Pᵢ^B)`.
///
/// The summation tree is **unrolled at generation time** rather than emitted
/// as a loop over an array. That is what makes the two backends comparable by
/// inspection: both print the identical sequence of named temporaries from
/// the identical [`dot_schedule`], so a reviewer can diff them line for line
/// and a divergence cannot hide inside differing loop semantics.
fn sv_dot(s: BlockShape) -> String {
    let name = BlockHelper::Dot { shape: s }.sv_name();
    let (bw, ew, sw) = (s.bits(), s.elem_width(), s.scale.width());
    let (n, tag) = (s.n, s.elem_tag());
    let (adds, last) = dot_schedule(n);
    let mut o = String::new();
    let _ = writeln!(
        o,
        "// {name}: OCP MX \u{a7}6.2 block dot, X^A * X^B * sum(P^A_i * P^B_i)."
    );
    let _ = writeln!(
        o,
        "function automatic logic {}{name}(input logic {}a, input logic {}b);",
        sv_w(32),
        sv_w(bw),
        sv_w(bw)
    );
    for t in 0..(n as usize + adds.len()) {
        let _ = writeln!(o, "  logic {}t{t};", sv_w(32));
    }
    // Element-pair products. EXACT in FP32 for every element format — the
    // widest significand product is E4M3's 4x4 = 8 bits against FP32's 24,
    // and the widest exponent range is E5M2's 2^-32 .. 2^31.6, comfortably
    // inside FP32's normals. So every rounding in a block dot happens in the
    // summation below, none in these multiplies (`fp_block_dot_products_are_exact`).
    for i in 0..n as usize {
        let _ = writeln!(
            o,
            "  t{i} = arch_f32_mul(arch_{tag}_to_f32(a[{lo} +: {ew}]), \
             arch_{tag}_to_f32(b[{lo} +: {ew}]));",
            lo = i * ew as usize
        );
    }
    for (k, (l, r)) in adds.iter().enumerate() {
        let _ = writeln!(o, "  t{} = arch_f32_add(t{l}, t{r});", n as usize + k);
    }
    // Scales applied ONE AT A TIME, not as a pre-formed `X^A * X^B`.
    // Each is a power of two, so each multiply is exact absent over/underflow
    // — but forming `X^A * X^B` first can overflow to Inf or flush to zero
    // (the two E8M0 exponents span 2^-127..2^127, so their product spans
    // 2^-254..2^254, well outside FP32) even when the final result is
    // perfectly representable. Applying them separately has a strictly wider
    // exact domain. A NaN scale (0xFF) widens to NaN and poisons the result
    // here, which is the block value rule.
    let _ = writeln!(
        o,
        "  {name} = arch_f32_mul(arch_f32_mul(t{last}, arch_e8m0_to_f32(a[{hi}:{lo}])), \
         arch_e8m0_to_f32(b[{hi}:{lo}]));",
        hi = bw - 1,
        lo = bw - sw
    );
    let _ = writeln!(o, "endfunction");
    o
}

fn sv_quantize(s: BlockShape, policy: ScalePolicy, round: RoundMode) -> String {
    assert_eq!(
        round,
        RoundMode::Rne,
        "only RNE narrowing is lowered today; typecheck refuses the others \
         (arch#890) — reaching here means that gate was removed \
         without adding the rounder variants"
    );
    let name = BlockHelper::Quantize {
        shape: s,
        policy,
        round,
    }
    .sv_name();
    let (bw, vw, ew, sw) = (s.bits(), s.vec_bits(), s.elem_width(), s.scale.width());
    let (n, emax) = (s.n, s.elem_emax());
    let tag = s.elem_tag();
    let mut o = String::new();
    let _ = writeln!(
        o,
        "// {name}: Vec<FP32,{n}> -> {{scale, P[{}..0]}}. OCP MX \u{a7}6.3 conversion.",
        n - 1
    );
    let _ = writeln!(
        o,
        "function automatic logic {}{name}(input logic {}v);",
        sv_w(bw),
        sv_w(vw)
    );
    let _ = writeln!(o, "  logic {}amax;", sv_w(32));
    let _ = writeln!(o, "  logic {}mag;", sv_w(32));
    let _ = writeln!(o, "  logic {}inv;", sv_w(32));
    let _ = writeln!(o, "  logic {}ecode;", sv_w(8));
    let _ = writeln!(o, "  logic {}code;", sv_w(8));
    let _ = writeln!(o, "  logic {}r;", sv_w(bw));
    // 1. amax as an unsigned integer max of |v_i| — see the module header.
    let _ = writeln!(o, "  amax = {}'h0;", 32);
    let _ = writeln!(o, "  for (int unsigned i = 0; i < {n}; i = i + 1) begin");
    let _ = writeln!(o, "    mag = v[i*32 +: 32] & 32'h7FFFFFFF;");
    let _ = writeln!(o, "    if (mag > amax) amax = mag;");
    let _ = writeln!(o, "  end");
    let _ = writeln!(o, "  r = {bw}'h0;");
    // 2. Non-finite anywhere in the block => NaN scale, element bits are
    //    don't-care by the block value rule and are emitted as zero.
    let _ = writeln!(o, "  if (amax >= 32'h7F800000) begin");
    let _ = writeln!(o, "    r[{}:{}] = 8'hFF;", bw - 1, bw - sw);
    // 3. All-zero block => minimum scale, zero elements.
    let _ = writeln!(o, "  end else if (amax == 32'h0) begin");
    let _ = writeln!(o, "    r[{}:{}] = 8'h00;", bw - 1, bw - sw);
    let _ = writeln!(o, "  end else begin");
    let _ = writeln!(o, "    ecode = arch_f32_to_e8m0(amax);");
    if policy == ScalePolicy::CeilPow2 {
        // Round the scale UP when amax is not already an exact power of two.
        // `arch_e8m0_to_f32(ecode)` is the floor power of two as an f32 bit
        // pattern; both operands are finite and positive, so the unsigned
        // compare is the numeric compare. `0xFE` is the largest non-NaN code.
        let _ = writeln!(
            o,
            "    if (amax > arch_e8m0_to_f32(ecode) && ecode != 8'hFE) ecode = ecode + 8'h01;"
        );
    }
    // 4. Shift the shared exponent down so the block maximum normalizes into
    //    the element format's top binade; clamp underflow at the minimum
    //    scale (overflow is structurally impossible: ecode <= 0xFE, emax >= 2).
    let _ = writeln!(
        o,
        "    code = (ecode > 8'd{emax}) ? (ecode - 8'd{emax}) : 8'h00;"
    );
    // 5. Multiply by the reciprocal scale, itself an E8M0 code, so the
    //    scaling is exact and the narrow below rounds exactly once.
    let _ = writeln!(o, "    inv = arch_e8m0_to_f32(8'd254 - code);");
    let _ = writeln!(o, "    for (int unsigned i = 0; i < {n}; i = i + 1) begin");
    let _ = writeln!(
        o,
        "      r[i*{ew} +: {ew}] = arch_f32_to_{tag}(arch_f32_mul(v[i*32 +: 32], inv));"
    );
    let _ = writeln!(o, "    end");
    let _ = writeln!(o, "    r[{}:{}] = code;", bw - 1, bw - sw);
    let _ = writeln!(o, "  end");
    let _ = writeln!(o, "  {name} = r;");
    let _ = writeln!(o, "endfunction");
    o
}

fn sv_dequantize(s: BlockShape) -> String {
    let name = BlockHelper::Dequantize { shape: s }.sv_name();
    let (bw, vw, ew, sw) = (s.bits(), s.vec_bits(), s.elem_width(), s.scale.width());
    let (n, tag) = (s.n, s.elem_tag());
    let mut o = String::new();
    let _ = writeln!(o, "// {name}: block -> Vec<FP32,{n}>, scale applied.");
    let _ = writeln!(
        o,
        "function automatic logic {}{name}(input logic {}b);",
        sv_w(vw),
        sv_w(bw)
    );
    let _ = writeln!(o, "  logic {}x;", sv_w(32));
    let _ = writeln!(o, "  logic {}w;", sv_w(32));
    let _ = writeln!(o, "  logic {}p;", sv_w(32));
    let _ = writeln!(o, "  logic {}r;", sv_w(vw));
    // A NaN scale (code 0xFF) widens to a NaN f32, so every product below is
    // NaN and the element bits are ignored: the block value rule falls out of
    // the multiply rather than needing a branch.
    let _ = writeln!(o, "  x = arch_e8m0_to_f32(b[{}:{}]);", bw - 1, bw - sw);
    let _ = writeln!(o, "  r = {vw}'h0;");
    let _ = writeln!(o, "  for (int unsigned i = 0; i < {n}; i = i + 1) begin");
    let _ = writeln!(o, "    w = arch_{tag}_to_f32(b[i*{ew} +: {ew}]);");
    let _ = writeln!(o, "    p = arch_f32_mul(x, w);");
    // A finite element scaled by a finite scale saturates to +-FP32_MAX
    // rather than overflowing to infinity. An element that is ITSELF Inf or
    // NaN (E4M3/E5M2 can be) keeps its own non-finite result — hence the
    // guard on `w` and not just on the product.
    let _ = writeln!(
        o,
        "    if (p[30:23] == 8'hFF && p[22:0] == 23'h0 && w[30:23] != 8'hFF)"
    );
    let _ = writeln!(o, "      p = {{p[31], 31'h7F7FFFFF}};");
    let _ = writeln!(o, "    r[i*32 +: 32] = p;");
    let _ = writeln!(o, "  end");
    let _ = writeln!(o, "  {name} = r;");
    let _ = writeln!(o, "endfunction");
    o
}

// ── C++ (arch sim) ──────────────────────────────────────────────────────────

/// Bit insert/extract over a little-endian `uint32_t` word array, matching
/// `VlWide`'s layout (`_data[0]` is bits 31:0).
///
/// Bit-at-a-time on purpose: element widths are 4, 6 and 8 bits, and 6 does
/// not divide 32, so a field can straddle a word boundary. A width-agnostic
/// loop is obviously correct where a shift-and-mask fast path would need a
/// straddle case that only FP6 blocks exercise.
/// Bit and word access over a little-endian `uint32_t` array, matching
/// `VlWide`'s layout (`_data[0]` is bits 31:0).
///
/// `_arch_blk_ins`/`_arch_blk_ext` work a bit at a time on purpose: element
/// widths are 4, 6 and 8 bits, and 6 does not divide 32, so a field can
/// straddle a word boundary. A width-agnostic loop is obviously correct where
/// a shift-and-mask fast path would need a straddle case that only FP6 blocks
/// exercise.
///
/// `_arch_blk_get`/`_arch_blk_put` are overloaded rather than switched on a
/// width, because the sim gives one block type TWO different C++
/// representations: a 72-bit block is `VlWide<3>` as a port but `_arch_u128`
/// as an internal wire. Any width table here would therefore be right for
/// ports and wrong for wires — the `_ => 32` failure shape again. Letting the
/// destination's own type select the overload means there is nothing to keep
/// in sync: the scalar overload takes `unsigned __int128`, which every
/// `uint8/16/32/64_t` and `_arch_u128` promotes to, and the `VlWide<W>`
/// template is the more specialized match for wide storage.
pub const CPP_PRELUDE: &str = "\
// ── ScaledVec block bit access (see src/fp_block.rs) ──
static inline void _arch_blk_ins(uint32_t* w, unsigned lo, unsigned width, uint32_t val) {
  for (unsigned k = 0; k < width; ++k) {
    unsigned b = lo + k;
    w[b >> 5] = (w[b >> 5] & ~(1u << (b & 31))) | (((val >> k) & 1u) << (b & 31));
  }
}
static inline uint32_t _arch_blk_ext(const uint32_t* w, unsigned lo, unsigned width) {
  uint32_t r = 0;
  for (unsigned k = 0; k < width; ++k) {
    unsigned b = lo + k;
    r |= ((w[b >> 5] >> (b & 31)) & 1u) << k;
  }
  return r;
}
static inline void _arch_blk_get(uint32_t* w, int nw, unsigned __int128 v) {
  for (int k = 0; k < nw; ++k) w[k] = (k < 4) ? (uint32_t)(v >> (32 * k)) : 0u;
}
template <int W> static inline void _arch_blk_get(uint32_t* w, int nw, const VlWide<W>& v) {
  for (int k = 0; k < nw; ++k) w[k] = (k < W) ? v.data()[k] : 0u;
}
template <typename T> static inline void _arch_blk_put(T& dst, const uint32_t* w, int nw) {
  unsigned __int128 acc = 0;
  for (int k = 0; k < nw && k < 4; ++k) acc |= ((unsigned __int128)w[k]) << (32 * k);
  dst = (T)acc;
}
template <int W> static inline void _arch_blk_put(VlWide<W>& dst, const uint32_t* w, int nw) {
  for (int k = 0; k < W; ++k) dst.data()[k] = (k < nw) ? w[k] : 0u;
}
";

fn cpp_words(bits: u32) -> u32 {
    bits.div_ceil(32)
}

/// The C++ definition. Signature is `(input, output&)` rather than a return
/// value because both sides are aggregates in the sim: a wide block is a
/// `VlWide` or an `_arch_u128`, and a `Vec<FP32,N>` is a plain C array,
/// neither of which composes into an expression the way the SV function
/// return does. The block side is a template parameter so the caller's own
/// storage type selects the access overload — see [`CPP_PRELUDE`].
pub fn cpp_definition(h: BlockHelper) -> String {
    match h {
        BlockHelper::Quantize {
            shape,
            policy,
            round,
        } => cpp_quantize(shape, policy, round),
        BlockHelper::Dequantize { shape } => cpp_dequantize(shape),
        BlockHelper::Dot { shape } => cpp_dot(shape),
    }
}

/// C++ twin of [`sv_dot`]. Same schedule, same temp numbering, same order of
/// scale application — compare the two side by side.
///
/// Returns by value (unlike quantize/dequantize): the result is a scalar
/// `FP32`, so it composes as an ordinary expression on both backends. Both
/// block operands are template parameters because the same block type has
/// different C++ storage as a port and as an internal wire, and a dot can mix
/// the two.
fn cpp_dot(s: BlockShape) -> String {
    let name = BlockHelper::Dot { shape: s }.cpp_name();
    let (bw, ew, sw) = (s.bits(), s.elem_width(), s.scale.width());
    let (n, tag) = (s.n, s.elem_tag());
    let nw = cpp_words(bw);
    let (adds, last) = dot_schedule(n);
    let mut o = String::new();
    let _ = writeln!(
        o,
        "// {name}: OCP MX §6.2 block dot. Mirrors sv_dot in src/fp_block.rs."
    );
    let _ = writeln!(o, "template <typename BA, typename BB>");
    let _ = writeln!(
        o,
        "static inline uint32_t {name}(const BA& a, const BB& b) {{"
    );
    let _ = writeln!(o, "  uint32_t _wa[{nw}], _wb[{nw}];");
    let _ = writeln!(o, "  _arch_blk_get(_wa, {nw}, a);");
    let _ = writeln!(o, "  _arch_blk_get(_wb, {nw}, b);");
    // Exact products — see the note in sv_dot.
    for i in 0..n as usize {
        let _ = writeln!(
            o,
            "  uint32_t t{i} = _arch_f32_mul(_arch_{tag}_to_f32((uint8_t)_arch_blk_ext(_wa, {lo}u, {ew}u)), \
             _arch_{tag}_to_f32((uint8_t)_arch_blk_ext(_wb, {lo}u, {ew}u)));",
            lo = i * ew as usize
        );
    }
    for (k, (l, r)) in adds.iter().enumerate() {
        let _ = writeln!(
            o,
            "  uint32_t t{} = _arch_f32_add(t{l}, t{r});",
            n as usize + k
        );
    }
    // Scales one at a time — see the note in sv_dot.
    let _ = writeln!(
        o,
        "  uint32_t xa = _arch_e8m0_to_f32((uint8_t)_arch_blk_ext(_wa, {lo}u, {sw}u));",
        lo = bw - sw
    );
    let _ = writeln!(
        o,
        "  uint32_t xb = _arch_e8m0_to_f32((uint8_t)_arch_blk_ext(_wb, {lo}u, {sw}u));",
        lo = bw - sw
    );
    let _ = writeln!(o, "  return _arch_f32_mul(_arch_f32_mul(t{last}, xa), xb);");
    let _ = writeln!(o, "}}");
    o
}

fn cpp_quantize(s: BlockShape, policy: ScalePolicy, round: RoundMode) -> String {
    assert_eq!(
        round,
        RoundMode::Rne,
        "only RNE narrowing is lowered today; typecheck refuses the others \
         (arch#890) — reaching here means that gate was removed \
         without adding the rounder variants"
    );
    let name = BlockHelper::Quantize {
        shape: s,
        policy,
        round,
    }
    .cpp_name();
    let (bw, ew, sw) = (s.bits(), s.elem_width(), s.scale.width());
    let (n, emax, tag) = (s.n, s.elem_emax(), s.elem_tag());
    let nw = cpp_words(bw);
    let mut o = String::new();
    let _ = writeln!(
        o,
        "// {name}: Vec<FP32,{n}> -> block ({bw} bits). Mirrors sv_quantize in src/fp_block.rs."
    );
    let _ = writeln!(o, "template <typename BT>");
    let _ = writeln!(
        o,
        "static inline void {name}(const uint32_t* v, BT& out) {{"
    );
    let _ = writeln!(o, "  uint32_t _w[{nw}];");
    let _ = writeln!(o, "  for (int k = 0; k < {nw}; ++k) _w[k] = 0u;");
    let _ = writeln!(o, "  uint32_t amax = 0u;");
    let _ = writeln!(o, "  for (unsigned i = 0; i < {n}u; ++i) {{");
    let _ = writeln!(o, "    uint32_t mag = v[i] & 0x7FFFFFFFu;");
    let _ = writeln!(o, "    if (mag > amax) amax = mag;");
    let _ = writeln!(o, "  }}");
    let _ = writeln!(o, "  if (amax >= 0x7F800000u) {{");
    let _ = writeln!(o, "    _arch_blk_ins(_w, {}u, {sw}u, 0xFFu);", bw - sw);
    let _ = writeln!(o, "  }} else if (amax == 0u) {{");
    let _ = writeln!(o, "    _arch_blk_ins(_w, {}u, {sw}u, 0x00u);", bw - sw);
    let _ = writeln!(o, "  }} else {{");
    let _ = writeln!(o, "    uint8_t ecode = _arch_f32_to_e8m0(amax);");
    if policy == ScalePolicy::CeilPow2 {
        let _ = writeln!(
            o,
            "    if (amax > _arch_e8m0_to_f32(ecode) && ecode != 0xFEu) ecode = (uint8_t)(ecode + 1u);"
        );
    }
    let _ = writeln!(
        o,
        "    uint8_t code = (ecode > {emax}u) ? (uint8_t)(ecode - {emax}u) : (uint8_t)0u;"
    );
    let _ = writeln!(
        o,
        "    uint32_t inv = _arch_e8m0_to_f32((uint8_t)(254u - code));"
    );
    let _ = writeln!(o, "    for (unsigned i = 0; i < {n}u; ++i) {{");
    let _ = writeln!(
        o,
        "      uint32_t p = (uint32_t)_arch_f32_to_{tag}(_arch_f32_mul(v[i], inv));"
    );
    let _ = writeln!(o, "      _arch_blk_ins(_w, i * {ew}u, {ew}u, p);");
    let _ = writeln!(o, "    }}");
    let _ = writeln!(
        o,
        "    _arch_blk_ins(_w, {}u, {sw}u, (uint32_t)code);",
        bw - sw
    );
    let _ = writeln!(o, "  }}");
    let _ = writeln!(o, "  _arch_blk_put(out, _w, {nw});");
    let _ = writeln!(o, "}}");
    o
}

fn cpp_dequantize(s: BlockShape) -> String {
    let name = BlockHelper::Dequantize { shape: s }.cpp_name();
    let (bw, ew, sw) = (s.bits(), s.elem_width(), s.scale.width());
    let (n, tag) = (s.n, s.elem_tag());
    let nw = cpp_words(bw);
    let mut o = String::new();
    let _ = writeln!(
        o,
        "// {name}: block ({bw} bits) -> Vec<FP32,{n}>. Mirrors sv_dequantize in src/fp_block.rs."
    );
    let _ = writeln!(o, "template <typename BT>");
    let _ = writeln!(
        o,
        "static inline void {name}(const BT& b, uint32_t* out) {{"
    );
    let _ = writeln!(o, "  uint32_t _w[{nw}];");
    let _ = writeln!(o, "  _arch_blk_get(_w, {nw}, b);");
    let _ = writeln!(
        o,
        "  uint32_t x = _arch_e8m0_to_f32((uint8_t)_arch_blk_ext(_w, {}u, {sw}u));",
        bw - sw
    );
    let _ = writeln!(o, "  for (unsigned i = 0; i < {n}u; ++i) {{");
    let _ = writeln!(
        o,
        "    uint32_t w = _arch_{tag}_to_f32((uint8_t)_arch_blk_ext(_w, i * {ew}u, {ew}u));"
    );
    let _ = writeln!(o, "    uint32_t p = _arch_f32_mul(x, w);");
    let _ = writeln!(
        o,
        "    if ((p & 0x7FFFFFFFu) == 0x7F800000u && (w & 0x7F800000u) != 0x7F800000u)"
    );
    let _ = writeln!(o, "      p = (p & 0x80000000u) | 0x7F7FFFFFu;");
    let _ = writeln!(o, "    out[i] = p;");
    let _ = writeln!(o, "  }}");
    let _ = writeln!(o, "}}");
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shape(elem: FpFormatId, n: u32) -> BlockShape {
        BlockShape {
            elem,
            n,
            scale: BlockScale::E8m0,
        }
    }

    /// `elem_emax` is derived from `max_finite` rather than hand-tabled, so
    /// pin every value it can produce. These are the amounts the shared
    /// exponent is shifted by; an off-by-one here is a silent 2x scale error
    /// across the whole block.
    #[test]
    fn fp_block_emax_matches_each_format_top_binade() {
        for (id, want, why) in [
            (FpFormatId::E2m1, 2u32, "max 6.0 = 1.5 * 2^2"),
            (FpFormatId::E2m3, 2, "max 7.5 = 1.875 * 2^2"),
            (FpFormatId::E3m2, 4, "max 28 = 1.75 * 2^4"),
            (FpFormatId::E4m3, 8, "max 448 = 1.75 * 2^8"),
            (FpFormatId::E5m2, 15, "max 57344 = 1.75 * 2^15"),
        ] {
            assert_eq!(shape(id, 32).elem_emax(), want, "{id:?}: {why}");
        }
    }

    /// The packed width must agree with `fp_format::scaled_vec_width`, which
    /// is what the type checker and both backends size ports from. Two
    /// independent width computations for one layout is exactly the phase-2a
    /// divergence this keeps closed.
    #[test]
    fn fp_block_bits_agree_with_scaled_vec_width() {
        for (id, ty) in [
            (FpFormatId::E2m1, TypeExpr::FP4E2M1),
            (FpFormatId::E2m3, TypeExpr::FP6E2M3),
            (FpFormatId::E3m2, TypeExpr::FP6E3M2),
            (FpFormatId::E4m3, TypeExpr::FP8E4M3),
            (FpFormatId::E5m2, TypeExpr::FP8E5M2),
        ] {
            for n in [1u32, 4, 16, 32] {
                assert_eq!(
                    Some(shape(id, n).bits()),
                    crate::fp_format::scaled_vec_width(&ty, n, &TypeExpr::E8M0),
                    "{id:?} x {n}"
                );
            }
        }
        // MXFP4 is the headline number from the proposal.
        assert_eq!(shape(FpFormatId::E2m1, 32).bits(), 136);
    }

    /// `shape_of` must reject exactly what the type checker rejects. A `None`
    /// here is a refusal; a `Some` for a non-block type would resurrect the
    /// "measured fine in one backend, `unwrap_or(0)` in the other" divergence.
    #[test]
    fn fp_block_shape_of_rejects_non_block_members() {
        assert!(shape_of(&TypeExpr::FP4E2M1, 32, &TypeExpr::E8M0).is_some());
        assert!(shape_of(&TypeExpr::FP32, 32, &TypeExpr::E8M0).is_none());
        assert!(shape_of(&TypeExpr::BF16, 32, &TypeExpr::E8M0).is_none());
        assert!(shape_of(&TypeExpr::Bool, 32, &TypeExpr::E8M0).is_none());
        // FP8E4M3 is a legal ELEMENT but NVFP4's scale is UE4M3, a different
        // format — so it must not be accepted in the scale slot.
        assert!(shape_of(&TypeExpr::FP4E2M1, 32, &TypeExpr::FP8E4M3).is_none());
    }

    /// The two renderings are generated from one descriptor, so what has to
    /// be pinned is that they stay *structurally* parallel: same constants,
    /// same branch order, same helper callees. Values are pinned separately
    /// by the Verilator differential harness, which runs the emitted SV
    /// against this same C++ as the DPI reference.
    #[test]
    fn fp_block_sv_and_cpp_agree_on_shape() {
        for policy in [ScalePolicy::FloorPow2, ScalePolicy::CeilPow2] {
            for (id, tag) in [
                (FpFormatId::E2m1, "e2m1"),
                (FpFormatId::E2m3, "e2m3"),
                (FpFormatId::E3m2, "e3m2"),
                (FpFormatId::E4m3, "e4m3"),
                (FpFormatId::E5m2, "e5m2"),
            ] {
                let s = shape(id, 32);
                let q = BlockHelper::Quantize {
                    shape: s,
                    policy,
                    round: RoundMode::Rne,
                };
                let (sv, cpp) = (sv_definition(q), cpp_definition(q));
                // Both must call the SAME numeric helpers — the whole point
                // of the design is that neither invents arithmetic.
                for callee in ["f32_to_e8m0", "e8m0_to_f32", "f32_mul"] {
                    assert!(sv.contains(callee), "SV {tag} must call {callee}");
                    assert!(cpp.contains(callee), "C++ {tag} must call {callee}");
                }
                assert!(sv.contains(&format!("arch_f32_to_{tag}(")));
                assert!(cpp.contains(&format!("_arch_f32_to_{tag}(")));
                // Same shared constants on both sides.
                assert!(sv.contains("7F800000") && cpp.contains("7F800000u"));
                assert!(sv.contains("8'd254 - code") && cpp.contains("254u - code"));
                let emax = s.elem_emax();
                assert!(sv.contains(&format!("8'd{emax}")));
                assert!(cpp.contains(&format!("{emax}u")));
                // The ceil bump is present iff the policy asks for it.
                assert_eq!(
                    sv.contains("ecode + 8'h01"),
                    policy == ScalePolicy::CeilPow2
                );
                assert_eq!(cpp.contains("ecode + 1u"), policy == ScalePolicy::CeilPow2);

                let d = BlockHelper::Dequantize { shape: s };
                let (dsv, dcpp) = (sv_definition(d), cpp_definition(d));
                assert!(dsv.contains(&format!("arch_{tag}_to_f32(")));
                assert!(dcpp.contains(&format!("_arch_{tag}_to_f32(")));
                // Saturation to +-FP32_MAX, guarded on a finite element.
                assert!(dsv.contains("7F7FFFFF") && dcpp.contains("7F7FFFFFu"));
            }
        }
    }

    /// The emitted C++ must NOT name a concrete block storage type. The sim
    /// gives one block type two different C++ representations — a 72-bit
    /// block is `VlWide<3>` as a port but `_arch_u128` as an internal wire —
    /// so any storage type baked in here would be right for one and a silent
    /// truncation for the other. The destination's own type must select the
    /// access overload.
    #[test]
    fn fp_block_cpp_is_generic_over_block_storage() {
        for bits_case in [
            shape(FpFormatId::E2m1, 4),  // 24 bits — a scalar either way
            shape(FpFormatId::E4m3, 8),  // 72 bits — VlWide as a port, _arch_u128 as a wire
            shape(FpFormatId::E2m1, 32), // 136 bits — VlWide either way
        ] {
            for cpp in [
                cpp_definition(BlockHelper::Quantize {
                    shape: bits_case,
                    policy: ScalePolicy::FloorPow2,
                    round: RoundMode::Rne,
                }),
                cpp_definition(BlockHelper::Dequantize { shape: bits_case }),
            ] {
                assert!(
                    cpp.contains("template <typename BT>"),
                    "block side must be a template parameter:\n{cpp}"
                );
                for banned in ["VlWide<", "_arch_u128", "uint64_t& ", "uint32_t& "] {
                    assert!(
                        !cpp.contains(banned),
                        "emitted C++ names the concrete storage type `{banned}`, which is \
                         right for only one of port/wire storage:\n{cpp}"
                    );
                }
                // …and it must route through the overloaded accessors.
                assert!(cpp.contains("_arch_blk_get(") || cpp.contains("_arch_blk_put("));
            }
        }
    }

    /// Names must be unique per (op, element, N, scale, policy, rounding) —
    /// a collision would silently give two different designs one definition.
    #[test]
    fn fp_block_names_are_injective() {
        let mut seen = std::collections::BTreeSet::new();
        for id in [
            FpFormatId::E2m1,
            FpFormatId::E2m3,
            FpFormatId::E3m2,
            FpFormatId::E4m3,
            FpFormatId::E5m2,
        ] {
            for n in [4u32, 16, 32] {
                for policy in [ScalePolicy::FloorPow2, ScalePolicy::CeilPow2] {
                    let s = shape(id, n);
                    assert!(seen.insert(
                        BlockHelper::Quantize {
                            shape: s,
                            policy,
                            round: RoundMode::Rne
                        }
                        .sv_name()
                    ));
                    // Dequantize does not depend on policy, so the second
                    // insert is expected to collide — assert it does, which
                    // is what makes the emitters' dedup correct.
                    let d = BlockHelper::Dequantize { shape: s }.sv_name();
                    assert_eq!(seen.insert(d), policy == ScalePolicy::FloorPow2);
                }
            }
        }
    }
}
