// ScaledVec<Elem,N,Scale> storage round-trip in the native sim.
//
// Phase 2a ships the TYPE, not the operations, so what has to hold is that a
// block survives as one packed value: 136 bits for MXFP4 (8 scale + 32*4
// elements), 264 for MXFP8, and that the `split` boundary re-assembles
// {scale, elems} into exactly the block the packed port carries.
#include "VScaledVecType.h"
#include <cstdio>
#include <cstring>
static VScaledVecType dut;
static int pass = 0, fail = 0;
#define CHECK(c, m, ...)                                                       \
  do {                                                                         \
    if (c) {                                                                   \
      printf("  PASS: " m "\n", ##__VA_ARGS__);                                \
      ++pass;                                                                  \
    } else {                                                                   \
      printf("  FAIL: " m "\n", ##__VA_ARGS__);                                \
      ++fail;                                                                  \
    }                                                                          \
  } while (0)

// Word helpers: wide blocks are VlWide, whose storage is reached via .data().
template <typename T> static void set_word(T &v, int w, uint32_t x) {
  v.data()[w] = x;
}
template <typename T> static uint32_t get_word(const T &v, int w) {
  return const_cast<T &>(v).data()[w];
}

int main() {
  // --- packed MXFP4: 136 bits = 5 words (last word holds 8 bits) ---
  // Element plane [127:0] = a recognizable pattern; scale [135:128] = 0x7F
  // (E8M0 code 127 == 2^0 == 1.0, the identity scale).
  for (int w = 0; w < 4; ++w)
    set_word(dut.a, w, 0x11223344u + w);
  set_word(dut.a, 4, 0x7Fu);
  dut.eval();
  bool ok = true;
  for (int w = 0; w < 4; ++w)
    ok &= (get_word(dut.y, w) == 0x11223344u + (uint32_t)w);
  ok &= (get_word(dut.y, 4) == 0x7Fu);
  CHECK(ok, "MXFP4 packed block survives as one 136-bit value");

  // The scale really is in the HIGH bits: changing only word 4 must not
  // disturb the element plane.
  set_word(dut.a, 4, 0x00u); // E8M0 0x00 == minimum scale 2^-127, NOT zero
  dut.eval();
  CHECK(get_word(dut.y, 4) == 0x00u && get_word(dut.y, 0) == 0x11223344u,
        "scale field is independent of the element plane");

  // --- packed MXFP8: 264 bits = 9 words ---
  for (int w = 0; w < 8; ++w)
    set_word(dut.big, w, 0xA0B0C0D0u ^ (uint32_t)w);
  set_word(dut.big, 8, 0x81u);
  dut.eval();
  ok = true;
  for (int w = 0; w < 8; ++w)
    ok &= (get_word(dut.big_out, w) == (0xA0B0C0D0u ^ (uint32_t)w));
  ok &= (get_word(dut.big_out, 8) == 0x81u);
  CHECK(ok, "MXFP8 packed block (264 bits) survives");

  // --- `split` port, sim side ---
  // `split` is an SV-BOUNDARY shape: the SV module exposes `s_scale`/`s_elems`,
  // but arch sim has no SV boundary, so the block stays one packed member and
  // the ARCH-level value is identical. That is what makes a single testbench
  // meaningful across both backends for the packed semantics below.
  for (int w = 0; w < 4; ++w)
    set_word(dut.s, w, 0xDEADBEEFu + w);
  set_word(dut.s, 4, 0x7Fu);
  dut.eval();
  ok = true;
  for (int w = 0; w < 4; ++w)
    ok &= (get_word(dut.s_out, w) == 0xDEADBEEFu + (uint32_t)w);
  ok &= (get_word(dut.s_out, 4) == 0x7Fu);
  CHECK(ok, "split-declared port round-trips as one block in sim");

  printf("=== %d pass / %d fail ===\n", pass, fail);
  return fail == 0 ? 0 : 1;
}
