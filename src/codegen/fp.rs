//! Synthesizable SystemVerilog floating-point helpers (FP32 / BF16).
//!
//! These replace the v1 *behavioral* helpers (which used
//! `$bitstoshortreal` / `$shortrealtobits` / `$rtoi` and were simulation-only,
//! not synthesizable). Every function here is plain RTL — decode, integer
//! mantissa arithmetic, leading-zero normalization, round-to-nearest-even with
//! guard/round/sticky, and pack — so the emitted design synthesizes and can be
//! formally checked against the SMT `FloatingPoint` theory (doc/plan_fp_types.md
//! §7–§8).
//!
//! Semantics: IEEE-754 binary32, round-to-nearest-even, full subnormals, single
//! canonical quiet NaN (`0x7FC00000` / `0x7FC0`), float→int toward-zero and
//! saturating with NaN→type-max — i.e. the RISC-V special-value profile the sim
//! backend already implements.
//!
//! BF16 arithmetic is `widen → f32 op → narrow` (innocuous double rounding holds
//! since the f32 significand 24 ≥ 2·8 + 2), so there is no separate BF16
//! datapath. A single shared rounder (`arch_f32_normround`) backs mul, add, sub,
//! fma and int→float.
//!
//! **Verification**: differentially co-simulated under Verilator against a host
//! IEEE-754 (DPI-C) reference over the §8.2 corner vectors plus millions of
//! randomized and cancellation-prone pairs — bit-exact for every op, compare,
//! conversion (n ∈ {8,16,24,32,53,64}) and BF16 wrapper. (The §8.1 SMT
//! equivalence proof is the remaining formal sign-off.)
//!
//! Emitted once at `$unit` scope ahead of the modules that use FP, gated by
//! `Codegen::fp_helpers_used`.

pub(super) const FP_SV_HELPERS: &str = r#"// ── arch floating-point helpers (synthesizable RTL; see doc/plan_fp_types.md §7) ──
typedef struct packed {
  logic        sign;
  logic [23:0] mant;        // 24-bit significand (implicit bit included for normals)
  logic signed [15:0] eunb; // value = mant * 2^eunb  (for finite, nonzero)
  logic        is_zero;
  logic        is_inf;
  logic        is_nan;
} arch_f32_up_t;
function automatic arch_f32_up_t arch_f32_decode(input logic [31:0] x);
  arch_f32_up_t u;
  logic [7:0]  e;
  logic [22:0] f;
  e = x[30:23];
  f = x[22:0];
  u.sign    = x[31];
  u.is_nan  = (e == 8'hFF) && (f != 23'b0);
  u.is_inf  = (e == 8'hFF) && (f == 23'b0);
  u.is_zero = (e == 8'h00) && (f == 23'b0);
  if (e == 8'h00) begin
    u.mant = {1'b0, f};                 // subnormal (or zero)
    u.eunb = -16'sd149;
  end else begin
    u.mant = {1'b1, f};                 // normal
    u.eunb = $signed({8'b0, e}) - 16'sd150;
  end
  return u;
endfunction
// Round value = (sig * 2^e0) to nearest f32 (RNE). Handles overflow->inf and
// underflow->subnormal/zero. Caller handles NaN/Inf specials.
function automatic logic [31:0] arch_f32_normround(input logic sign,
                                                   input logic [511:0] sig,
                                                   input integer e0);
  integer p, E, biased, k, sh, i;
  logic [513:0] kept;
  logic guard, sticky, roundup;
  logic [7:0]  bexp;
  logic [22:0] frac;
  if (sig == 512'b0) return {sign, 31'b0};
  p = 0;
  for (i = 511; i >= 0; i = i - 1) begin
    if (sig[i]) begin p = i; break; end
  end
  E = p + e0;
  biased = E + 127;
  if (biased <= 0) k = -149;            // subnormal path
  else             k = E - 23;          // normal path
  sh = k - e0;                          // low bits of sig to drop
  if (sh <= 0) begin
    kept   = {2'b0, sig} << (-sh);
    guard  = 1'b0;
    sticky = 1'b0;
  end else begin
    kept   = {2'b0, (sig >> sh)};
    guard  = (sh - 1 < 512) ? sig[sh - 1] : 1'b0;
    if (sh >= 1) sticky = |(sig & ((512'd1 << (sh - 1)) - 512'd1));
    else         sticky = 1'b0;
  end
  roundup = guard & (sticky | kept[0]);
  kept = kept + roundup;
  if (biased <= 0) begin
    // subnormal: kept is the value in units of 2^-149; the {exp,frac} encoding
    // carries cleanly up to the smallest normal when kept reaches 2^23.
    return {sign, 8'b0, 23'b0} | {sign, kept[30:0]};
  end else begin
    if (kept[24]) begin                 // rounding carried into bit 24
      biased = biased + 1;
      kept   = kept >> 1;
    end
    if (biased >= 255) return {sign, 8'hFF, 23'b0};  // overflow -> inf
    bexp = biased[7:0];
    frac = kept[22:0];
    return {sign, bexp, frac};
  end
endfunction
function automatic logic [31:0] arch_f32_canon(input logic [31:0] x);
  arch_f32_canon = ((x[30:23] == 8'hFF) && (x[22:0] != 23'b0)) ? 32'h7FC00000 : x;
endfunction
function automatic logic [31:0] arch_f32_mul(input logic [31:0] a, input logic [31:0] b);
  arch_f32_up_t ua, ub;
  logic sy;
  logic [47:0] mp;
  integer e0;
  ua = arch_f32_decode(a);
  ub = arch_f32_decode(b);
  sy = ua.sign ^ ub.sign;
  if (ua.is_nan || ub.is_nan) return 32'h7FC00000;
  if ((ua.is_inf && ub.is_zero) || (ub.is_inf && ua.is_zero)) return 32'h7FC00000;
  if (ua.is_inf || ub.is_inf) return {sy, 8'hFF, 23'b0};
  if (ua.is_zero || ub.is_zero) return {sy, 31'b0};
  mp = ua.mant * ub.mant;
  e0 = ua.eunb + ub.eunb;
  return arch_f32_normround(sy, {464'b0, mp}, e0);
endfunction
function automatic logic arch_f32_isnan(input logic [31:0] x);
  arch_f32_isnan = (x[30:23] == 8'hFF) && (x[22:0] != 23'b0);
endfunction
function automatic logic arch_f32_iszero(input logic [31:0] x);
  arch_f32_iszero = (x[30:0] == 31'b0);
endfunction
function automatic logic arch_f32_eq(input logic [31:0] a, input logic [31:0] b);
  if (arch_f32_isnan(a) || arch_f32_isnan(b)) return 1'b0;
  return (a == b) || (arch_f32_iszero(a) && arch_f32_iszero(b));
endfunction
function automatic logic arch_f32_ne(input logic [31:0] a, input logic [31:0] b);
  return !arch_f32_eq(a, b);
endfunction
function automatic logic arch_f32_lt(input logic [31:0] a, input logic [31:0] b);
  logic sa, sb;
  if (arch_f32_isnan(a) || arch_f32_isnan(b)) return 1'b0;
  if (arch_f32_iszero(a) && arch_f32_iszero(b)) return 1'b0;
  sa = a[31]; sb = b[31];
  if (sa != sb) return sa;              // negative < positive
  if (sa == 1'b0) return (a[30:0] < b[30:0]);
  return (a[30:0] > b[30:0]);
endfunction
function automatic logic arch_f32_gt(input logic [31:0] a, input logic [31:0] b);
  return arch_f32_lt(b, a);
endfunction
function automatic logic arch_f32_le(input logic [31:0] a, input logic [31:0] b);
  return arch_f32_lt(a, b) || arch_f32_eq(a, b);
endfunction
function automatic logic arch_f32_ge(input logic [31:0] a, input logic [31:0] b);
  return arch_f32_lt(b, a) || arch_f32_eq(a, b);
endfunction
function automatic logic [31:0] arch_bf16_to_f32(input logic [15:0] h);
  arch_bf16_to_f32 = arch_f32_canon({h, 16'b0});
endfunction
function automatic logic [15:0] arch_f32_to_bf16(input logic [31:0] x);
  logic lsb, rbit, sticky, roundup;
  logic [31:0] sum;
  if (arch_f32_isnan(x)) return 16'h7FC0;
  lsb     = x[16];
  rbit    = x[15];
  sticky  = |x[14:0];
  roundup = rbit & (sticky | lsb);
  sum = x + (roundup ? 32'h0001_0000 : 32'b0);
  return sum[31:16];
endfunction
function automatic logic [31:0] arch_f32_add(input logic [31:0] a, input logic [31:0] b);
  arch_f32_up_t ua, ub, hi, lo;
  integer diff, e_lo;
  logic [511:0] HI, LO, mag;
  logic sresult;
  ua = arch_f32_decode(a);
  ub = arch_f32_decode(b);
  if (ua.is_nan || ub.is_nan) return 32'h7FC00000;
  if (ua.is_inf && ub.is_inf) begin
    if (ua.sign == ub.sign) return {ua.sign, 8'hFF, 23'b0};
    return 32'h7FC00000;                            // inf + (-inf)
  end
  if (ua.is_inf) return {ua.sign, 8'hFF, 23'b0};
  if (ub.is_inf) return {ub.sign, 8'hFF, 23'b0};
  if (ua.eunb >= ub.eunb) begin hi = ua; lo = ub; end
  else                    begin hi = ub; lo = ua; end
  diff = hi.eunb - lo.eunb;
  e_lo = lo.eunb;
  HI = {488'b0, hi.mant} << diff;
  LO = {488'b0, lo.mant};
  if (hi.sign == lo.sign) begin
    mag = HI + LO; sresult = hi.sign;
  end else if (HI > LO) begin
    mag = HI - LO; sresult = hi.sign;
  end else if (LO > HI) begin
    mag = LO - HI; sresult = lo.sign;
  end else begin
    return 32'h00000000;                            // exact cancellation -> +0 (RNE)
  end
  return arch_f32_normround(sresult, mag, e_lo);
endfunction
function automatic logic [31:0] arch_f32_sub(input logic [31:0] a, input logic [31:0] b);
  return arch_f32_add(a, {~b[31], b[30:0]});
endfunction
// fused multiply-add: round(a*b + c) with a single rounding.
function automatic logic [31:0] arch_fma_f32(input logic [31:0] a, input logic [31:0] b, input logic [31:0] c);
  arch_f32_up_t ua, ub, uc;
  logic sp, prod_inf, prod_zero, sresult;
  logic [47:0] mp;
  integer ep, e_lo;
  logic [511:0] PT, CT, mag;
  ua = arch_f32_decode(a);
  ub = arch_f32_decode(b);
  uc = arch_f32_decode(c);
  if (ua.is_nan || ub.is_nan || uc.is_nan) return 32'h7FC00000;
  if ((ua.is_inf && ub.is_zero) || (ua.is_zero && ub.is_inf)) return 32'h7FC00000; // 0*inf
  sp        = ua.sign ^ ub.sign;
  prod_inf  = ua.is_inf || ub.is_inf;
  prod_zero = ua.is_zero || ub.is_zero;
  if (prod_inf) begin
    if (uc.is_inf && (uc.sign != sp)) return 32'h7FC00000;   // inf - inf
    return {sp, 8'hFF, 23'b0};
  end
  if (uc.is_inf) return {uc.sign, 8'hFF, 23'b0};
  if (prod_zero) return arch_f32_add({sp, 31'b0}, c);        // (+/-0) + c
  mp = ua.mant * ub.mant;
  ep = ua.eunb + ub.eunb;
  if (uc.is_zero) return arch_f32_normround(sp, {464'b0, mp}, ep);
  if (ep >= uc.eunb) begin
    e_lo = uc.eunb;
    PT   = {464'b0, mp} << (ep - uc.eunb);
    CT   = {488'b0, uc.mant};
  end else begin
    e_lo = ep;
    PT   = {464'b0, mp};
    CT   = {488'b0, uc.mant} << (uc.eunb - ep);
  end
  if (sp == uc.sign) begin
    mag = PT + CT; sresult = sp;
  end else if (PT > CT) begin
    mag = PT - CT; sresult = sp;
  end else if (CT > PT) begin
    mag = CT - PT; sresult = uc.sign;
  end else begin
    return 32'h00000000;                                      // exact cancellation -> +0
  end
  return arch_f32_normround(sresult, mag, e_lo);
endfunction
// int -> float (RNE, single rounding via the shared rounder).
function automatic logic [31:0] arch_i64_to_f32(input logic signed [63:0] v);
  logic sign;
  logic [63:0] mag;
  if (v == 64'sd0) return 32'h00000000;
  sign = v[63];
  mag  = sign ? (~v + 64'd1) : v;
  return arch_f32_normround(sign, {448'b0, mag}, 0);
endfunction
function automatic logic [31:0] arch_u64_to_f32(input logic [63:0] v);
  if (v == 64'd0) return 32'h00000000;
  return arch_f32_normround(1'b0, {448'b0, v}, 0);
endfunction
// float -> int (toward zero, saturating to n bits, NaN -> type max).
function automatic logic [63:0] arch_f32_to_sint(input logic [31:0] x, input integer n);
  arch_f32_up_t u;
  logic [127:0] mag, lim_pos, lim_neg;
  integer sh;
  u = arch_f32_decode(x);
  lim_pos = (128'd1 << (n - 1)) - 128'd1;   //  2^(n-1)-1
  lim_neg = (128'd1 << (n - 1));            // |min| = 2^(n-1)
  if (u.is_nan)  return lim_pos[63:0];                       // NaN -> int max
  if (u.is_zero) return 64'd0;
  if (u.is_inf)  return u.sign ? (~lim_neg[63:0] + 64'd1) : lim_pos[63:0];
  if (u.eunb >= 64)      mag = {128{1'b1}};                  // |value| >= 2^64 -> saturate
  else if (u.eunb >= 0)  mag = ({104'b0, u.mant} << u.eunb);
  else begin sh = -u.eunb; mag = (sh >= 128) ? 128'd0 : ({104'b0, u.mant} >> sh); end
  if (!u.sign) begin
    if (mag > lim_pos) return lim_pos[63:0];
    return mag[63:0];
  end else begin
    if (mag > lim_neg) return (~lim_neg[63:0] + 64'd1);      // INT_MIN
    return (~mag[63:0] + 64'd1);                             // -mag
  end
endfunction
function automatic logic [63:0] arch_f32_to_uint(input logic [31:0] x, input integer n);
  arch_f32_up_t u;
  logic [127:0] mag, lim;
  integer sh;
  u = arch_f32_decode(x);
  lim = (128'd1 << n) - 128'd1;                              // 2^n - 1
  if (u.is_nan)  return lim[63:0];                           // NaN -> uint max
  if (u.is_zero) return 64'd0;
  if (u.sign)    return 64'd0;                               // negative (incl -inf) -> 0
  if (u.is_inf)  return lim[63:0];
  if (u.eunb >= 64)      mag = {128{1'b1}};
  else if (u.eunb >= 0)  mag = ({104'b0, u.mant} << u.eunb);
  else begin sh = -u.eunb; mag = (sh >= 128) ? 128'd0 : ({104'b0, u.mant} >> sh); end
  if (mag > lim) return lim[63:0];
  return mag[63:0];
endfunction
// bf16 arithmetic = widen -> f32 op -> narrow (innocuous double rounding).
function automatic logic [15:0] arch_bf16_add(input logic [15:0] a, input logic [15:0] b);
  arch_bf16_add = arch_f32_to_bf16(arch_f32_add(arch_bf16_to_f32(a), arch_bf16_to_f32(b)));
endfunction
function automatic logic [15:0] arch_bf16_sub(input logic [15:0] a, input logic [15:0] b);
  arch_bf16_sub = arch_f32_to_bf16(arch_f32_sub(arch_bf16_to_f32(a), arch_bf16_to_f32(b)));
endfunction
function automatic logic [15:0] arch_bf16_mul(input logic [15:0] a, input logic [15:0] b);
  arch_bf16_mul = arch_f32_to_bf16(arch_f32_mul(arch_bf16_to_f32(a), arch_bf16_to_f32(b)));
endfunction
function automatic logic [15:0] arch_fma_bf16(input logic [15:0] a, input logic [15:0] b, input logic [15:0] c);
  arch_fma_bf16 = arch_f32_to_bf16(arch_fma_f32(arch_bf16_to_f32(a), arch_bf16_to_f32(b), arch_bf16_to_f32(c)));
endfunction
function automatic logic arch_bf16_eq(input logic [15:0] a, input logic [15:0] b);
  arch_bf16_eq = arch_f32_eq(arch_bf16_to_f32(a), arch_bf16_to_f32(b));
endfunction
function automatic logic arch_bf16_ne(input logic [15:0] a, input logic [15:0] b);
  arch_bf16_ne = arch_f32_ne(arch_bf16_to_f32(a), arch_bf16_to_f32(b));
endfunction
function automatic logic arch_bf16_lt(input logic [15:0] a, input logic [15:0] b);
  arch_bf16_lt = arch_f32_lt(arch_bf16_to_f32(a), arch_bf16_to_f32(b));
endfunction
function automatic logic arch_bf16_gt(input logic [15:0] a, input logic [15:0] b);
  arch_bf16_gt = arch_f32_gt(arch_bf16_to_f32(a), arch_bf16_to_f32(b));
endfunction
function automatic logic arch_bf16_le(input logic [15:0] a, input logic [15:0] b);
  arch_bf16_le = arch_f32_le(arch_bf16_to_f32(a), arch_bf16_to_f32(b));
endfunction
function automatic logic arch_bf16_ge(input logic [15:0] a, input logic [15:0] b);
  arch_bf16_ge = arch_f32_ge(arch_bf16_to_f32(a), arch_bf16_to_f32(b));
endfunction

"#;
