// arch-sim side of the `scaled_quantize` / `scaled_dequantize` cross-backend
// check (phase 2b, arch#884).
//
// Prints one line per (vector, signal) in a format the SystemVerilog
// testbench `rtl_diff/tb_scaled_quant.sv` reproduces EXACTLY, so
// `fp_scaled_quant_sv_matches_sim` can byte-compare the two transcripts.
// Every block is printed as 32-bit words LSB-first — a `%h` of a 72-bit
// value has no portable C++ counterpart, and words make a mismatch land on
// the specific word rather than on one long string.
//
// The vector set is chosen to exercise the branches the block conversion
// actually has: an all-zero block (minimum scale, zero elements), a block
// containing a NaN and one containing an Inf (both force the NaN scale
// 0xFF and don't-care elements), subnormal inputs, a power-of-two maximum
// (where floor_pow2 and ceil_pow2 must agree) and a non-power-of-two
// maximum (where they must differ), and magnitudes far enough apart that
// the small elements quantize to zero.
#include "VScaledQuant.h"
#include <cstdint>
#include <cstdio>
#include <cstring>

static VScaledQuant dut;

// Blocks land in three C++ storage buckets depending on width; print each
// uniformly as words so the transcript shape does not depend on the bucket.
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
  uint32_t v[8];  // raw FP32 bit patterns
  uint32_t v4[4];
};

// Raw bit patterns, never float literals. Two reasons: a testbench written
// with `0.001f` asserts the C++ decimal parse as well as the DUT, and the
// SystemVerilog twin (`rtl_diff/tb_scaled_quant.sv`) has to drive the SAME
// bits — with hex on both sides the two vector tables diff line for line.
//
//   1.0=3F800000  -2.0=C0000000   4.0=40800000   0.5=3F000000  8.0=41000000
//  -0.25=BE800000  2.0=40000000   3.0=40400000   6.0=40C00000  1.5=3FC00000
//   0.75=3F400000  1024=44800000  0.001=3A83126F -512=C4000000
//  -0.0001=B8D1B717 256=43800000
int main() {
  const Case cases[] = {
      // Powers of two throughout: floor_pow2 and ceil_pow2 must agree.
      {"pow2",
       {0x3F800000u, 0xC0000000u, 0x40800000u, 0x3F000000u, 0x41000000u, 0xBE800000u,
        0x00000000u, 0x40000000u},
       {0x3F800000u, 0x40000000u, 0x40800000u, 0x3F000000u}},
      // Max 6.0 is not a power of two: the two scale policies must differ.
      {"nonpow2",
       {0x3F800000u, 0xC0000000u, 0x40400000u, 0x3F000000u, 0x40C00000u, 0xBE800000u,
        0x00000000u, 0x40800000u},
       {0x40C00000u, 0x40400000u, 0x3FC00000u, 0x3F400000u}},
      // All zero: minimum scale 0x00 and zero elements, NOT a NaN scale.
      {"zeros", {0, 0, 0, 0, 0, 0, 0, 0}, {0, 0, 0, 0}},
      // Signed zeros only — still an all-zero block by magnitude.
      {"negzero",
       {0x80000000u, 0, 0x80000000u, 0, 0, 0x80000000u, 0, 0},
       {0x80000000u, 0, 0, 0}},
      // One NaN anywhere forces the NaN scale 0xFF; element bits are
      // don't-care and must still agree bit for bit across backends.
      {"nan",
       {0x3F800000u, 0x7FC00000u, 0x40400000u, 0x3F000000u, 0x40C00000u, 0xBE800000u,
        0x00000000u, 0x40800000u},
       {0x3F800000u, 0x7FC00000u, 0x3F800000u, 0x3F800000u}},
      // Inf likewise: `arch_f32_to_e8m0` maps every non-finite to 0xFF.
      {"inf",
       {0x3F800000u, 0x7F800000u, 0x40400000u, 0x3F000000u, 0x40C00000u, 0xBE800000u,
        0x00000000u, 0x40800000u},
       {0xFF800000u, 0x3F800000u, 0x3F800000u, 0x3F800000u}},
      // Subnormals: the shared scale underflows and clamps at 0x00.
      {"subnormal",
       {1u, 2u, 0x007FFFFFu, 0x00400000u, 0u, 3u, 0x00800000u, 1u},
       {1u, 2u, 3u, 4u}},
      // Huge magnitudes: the scale saturates near the top of E8M0 and the
      // dequantized product is the saturation case (decision #5).
      {"huge",
       {0x7F7FFFFFu, 0x7F000000u, 0x3F800000u, 0x00000000u, 0xFF7FFFFFu, 0x40000000u,
        0x40800000u, 0x41000000u},
       {0x7F7FFFFFu, 0x3F800000u, 0x00000000u, 0x40000000u}},
      // Wide dynamic range inside one block: the small elements must
      // quantize to zero rather than to a wrapped code.
      {"range",
       {0x44800000u, 0x3A83126Fu, 0xC4000000u, 0x00000000u, 0x3F800000u, 0xB8D1B717u,
        0x43800000u, 0x40000000u},
       {0x44800000u, 0x3A83126Fu, 0x3F800000u, 0x00000000u}},
  };

  for (const Case &c : cases) {
    for (int i = 0; i < 8; ++i)
      dut.v[i] = c.v[i];
    for (int i = 0; i < 4; ++i)
      dut.v4[i] = c.v4[i];
    dut.eval();
    printf("== %s\n", c.why);
    print_words("q4f", dut.q4_floor, 2);
    print_words("q4c", dut.q4_ceil, 2);
    print_words("q6 ", dut.q6, 2);
    print_words("q8 ", dut.q8, 3);
    print_words("q4s", dut.q4s, 1);
    print_vec("b4 ", dut.back4, 8);
    print_vec("b6 ", dut.back6, 8);
    print_vec("b8 ", dut.back8, 8);
  }
  return 0;
}
