// arch-sim side of the NVFP4 (`UE4M3` block scale) cross-backend check
// (phase 5b, arch#905).
//
// Prints one line per (vector, signal) in a format the SystemVerilog
// testbench `rtl_diff/tb_nvfp4.sv` reproduces EXACTLY, so
// `fp_nvfp4_sv_matches_sim` can byte-compare the two transcripts. Same shape
// as `tb_scaled_quant.cpp` — blocks as 32-bit words, LSB first.
//
// The vector set targets what is NEW here rather than re-testing the E8M0
// paths: a scale that is not a power of two (so `exact` must differ from
// `floor_pow2`), the scale codes UE4M3 owns and E8M0 does not (`0x7F` is NaN,
// `0x00` is a genuine zero, and the underflow clamp is `0x01` rather than
// `0x00` because clamping to zero would erase the block), and element
// magnitudes that push an E4M3 element past its top code, which is the only
// place the profile-dependent overflow rung fires.
#include "VNvfp4Quant.h"
#include <cstdint>
#include <cstdio>
#include <cstring>

static VNvfp4Quant dut;

static void print_words(const char *name, uint64_t v, int words) {
  printf("%s", name);
  for (int w = 0; w < words; ++w)
    printf(" %08x", (uint32_t)(v >> (32 * w)));
  printf("\n");
}
template <int W>
static void print_words(const char *name, const VlWide<W> &v, int words) {
  printf("%s", name);
  for (int w = 0; w < words; ++w)
    printf(" %08x", const_cast<VlWide<W> &>(v).data()[w]);
  printf("\n");
}
static void print_vec(const char *name, const uint32_t *v, int n) {
  printf("%s", name);
  for (int i = 0; i < n; ++i)
    printf(" %08x", v[i]);
  printf("\n");
}

struct Case {
  const char *why;
  uint32_t v[16]; // raw FP32 bit patterns
  uint32_t v8[8];
};

// Raw bit patterns, never float literals — same reasoning as
// tb_scaled_quant.cpp: a decimal literal would assert the C++ parse too, and
// the SV twin has to drive identical bits.
//
//   1.0=3F800000   2.0=40000000   3.0=40400000   4.0=40800000   6.0=40C00000
//   0.5=3F000000   1.5=3FC00000   0.75=3F400000  8.0=41000000   0.25=3E800000
//   448=43E00000   480=43F00000   224=43600000   0.001=3A83126F 1024=44800000
int main() {
  const Case cases[] = {
      // Block maximum 8.0 is a power of two: `exact` can land on a
      // power-of-two scale too, so this is the case where the three policies
      // have the best chance of agreeing.
      {"pow2",
       {0x3F800000u, 0xC0000000u, 0x40800000u, 0x3F000000u, 0x41000000u, 0xBE800000u,
        0x00000000u, 0x40000000u, 0x3F800000u, 0x40800000u, 0xC1000000u, 0x3F000000u,
        0x40000000u, 0x00000000u, 0x3E800000u, 0x40800000u},
       {0x3F800000u, 0x40000000u, 0x40800000u, 0x3F000000u, 0x41000000u, 0x3F800000u,
        0x40400000u, 0x00000000u}},
      // Maximum 5.0 — chosen so that amax/elem_max is ALSO not a power of
      // two (6.0 would not do: 6/6 is exactly 1.0, so `exact` and
      // `floor_pow2` would agree and the check below would be vacuous).
      // This is the case that separates `exact`, which uses the scale's
      // mantissa, from `floor_pow2`, which throws it away. If a regression
      // restored the global floor default, `q_def` would stop matching
      // `q_exact` here.
      {"nonpow2",
       {0x3F800000u, 0xC0000000u, 0x40400000u, 0x3F000000u, 0x40A00000u, 0xBE800000u,
        0x00000000u, 0x40800000u, 0x3FC00000u, 0x3F400000u, 0xC0400000u, 0x40000000u,
        0x3F800000u, 0x40A00000u, 0x3E800000u, 0x3F000000u},
       {0x40A00000u, 0x40400000u, 0x3FC00000u, 0x3F400000u, 0x40000000u, 0x3F800000u,
        0xC0A00000u, 0x3F000000u}},
      // All zero: scale code 0x00, which for UE4M3 is a genuine ZERO rather
      // than E8M0's minimum scale. Must not be the NaN code.
      {"zeros",
       {0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0},
       {0, 0, 0, 0, 0, 0, 0, 0}},
      // Signed zeros only — still an all-zero block by magnitude.
      {"negzero",
       {0x80000000u, 0, 0x80000000u, 0, 0, 0x80000000u, 0, 0, 0x80000000u, 0, 0, 0,
        0x80000000u, 0, 0, 0},
       {0x80000000u, 0, 0, 0, 0x80000000u, 0, 0, 0}},
      // One NaN anywhere forces the NaN scale — 0x7F for UE4M3, NOT 0xFF:
      // 0xFF would set the padding bit the format requires to be zero.
      {"nan",
       {0x3F800000u, 0x7FC00000u, 0x40400000u, 0x3F000000u, 0x40C00000u, 0xBE800000u,
        0x00000000u, 0x40800000u, 0x3F800000u, 0x40000000u, 0x3F000000u, 0x40400000u,
        0x3F800000u, 0x40800000u, 0x3E800000u, 0x3F000000u},
       {0x3F800000u, 0x7FC00000u, 0x3F800000u, 0x3F800000u, 0x40000000u, 0x3F800000u,
        0x3F800000u, 0x3F800000u}},
      // Inf likewise.
      {"inf",
       {0x3F800000u, 0x7F800000u, 0x40400000u, 0x3F000000u, 0x40C00000u, 0xBE800000u,
        0x00000000u, 0x40800000u, 0x3F800000u, 0x40000000u, 0x3F000000u, 0x40400000u,
        0x3F800000u, 0x40800000u, 0x3E800000u, 0x3F000000u},
       {0xFF800000u, 0x3F800000u, 0x3F800000u, 0x3F800000u, 0x40000000u, 0x3F800000u,
        0x3F800000u, 0x3F800000u}},
      // Subnormal inputs: the scale underflows. UE4M3 clamps at code 0x01,
      // the smallest NONZERO scale — clamping at 0x00 would make the scale a
      // true zero and erase a block whose maximum is nonzero.
      {"subnormal",
       {1u, 2u, 0x007FFFFFu, 0x00400000u, 0u, 3u, 0x00800000u, 1u, 2u, 1u, 0u, 4u, 1u,
        0x00200000u, 0u, 1u},
       {1u, 2u, 3u, 4u, 1u, 0x00800000u, 2u, 1u}},
      // Huge magnitudes: the scale saturates at the top UE4M3 code and the
      // elements clamp.
      {"huge",
       {0x7F7FFFFFu, 0x7F000000u, 0x3F800000u, 0x00000000u, 0xFF7FFFFFu, 0x40000000u,
        0x40800000u, 0x41000000u, 0x7F000000u, 0x3F800000u, 0x00000000u, 0x40000000u,
        0x7F7FFFFFu, 0x40800000u, 0x3F000000u, 0x41000000u},
       {0x7F7FFFFFu, 0x3F800000u, 0x00000000u, 0x40000000u, 0x7F000000u, 0x40800000u,
        0x3F800000u, 0x41000000u}},
      // Wide dynamic range inside one block: the small elements must
      // quantize to zero rather than wrap.
      {"range",
       {0x44800000u, 0x3A83126Fu, 0xC4000000u, 0x00000000u, 0x3F800000u, 0xB8D1B717u,
        0x43800000u, 0x40000000u, 0x44800000u, 0x3A83126Fu, 0x3F800000u, 0x00000000u,
        0xC4000000u, 0x43800000u, 0x40000000u, 0x3F800000u},
       {0x44800000u, 0x3A83126Fu, 0x3F800000u, 0x00000000u, 0xC4000000u, 0x43800000u,
        0x40000000u, 0x3F800000u}},
      // E4M3 elements straddling their own top code: 448 is the largest
      // finite, 464 is the tie that must round DOWN to it, and 480 is the
      // first magnitude that overflows. This is the only vector that fires
      // the overflow rung, whose result is delegated to the element format's
      // own rounder so it follows `--fp-compat`.
      {"e4m3_top",
       {0x43E00000u, 0x43F00000u, 0x43600000u, 0x3F800000u, 0x43E00000u, 0xC3F00000u,
        0x40000000u, 0x43600000u, 0x43E00000u, 0x43F00000u, 0x3F800000u, 0x40800000u,
        0x43600000u, 0x43E00000u, 0x40000000u, 0x3F000000u},
       {0x43E00000u, 0x43F00000u, 0x43680000u, 0x43600000u, 0xC3E00000u, 0xC3F00000u,
        0x3F800000u, 0x40000000u}},
  };

  for (const Case &c : cases) {
    for (int i = 0; i < 16; ++i)
      dut.v[i] = c.v[i];
    for (int i = 0; i < 8; ++i)
      dut.v8[i] = c.v8[i];
    dut.eval();
    printf("== %s\n", c.why);
    print_words("qd ", dut.q_def, 3);
    print_words("qe ", dut.q_exact, 3);
    print_words("qf ", dut.q_floor, 3);
    print_words("qc ", dut.q_ceil, 3);
    print_words("qmx", dut.q_mx, 3);
    print_words("q8 ", dut.q8, 3);
    print_vec("bk ", dut.back, 16);
    print_vec("bmx", dut.back_mx, 16);
    print_vec("bk8", dut.back8, 8);
    printf("dot %08x %08x %08x\n", dut.dot, dut.dot_x, dut.dot8);
  }
  return 0;
}
