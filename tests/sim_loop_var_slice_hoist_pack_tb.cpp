// Behavioral TB for arch#1019 defect #3 — scalar-LHS ← whole-Vec-RHS pack.
//
// `LoopVarSliceHoist` ends with `y_index = wi;` where `wi: Vec<Bool,4>` and
// `y_index: out UInt<4>`. `arch build` treats this as a packed-vector
// assignment (element 0 → bit 0). The C++ sim decomposes `wi` into an array,
// so pre-fix the model emitted `y_index = _let_wi;` (scalar = array) and the
// host compiler rejected it. Post-fix the sim must pack the array into the
// scalar with the same little-endian layout as the SV backend.
//
// wi[i] = (a + v[i]) bit 3  ==  ((a + v[i]) >> 3) & 1
// y_index = wi[0] | wi[1]<<1 | wi[2]<<2 | wi[3]<<3
#include "VLoopVarSliceHoist.h"
#include <cstdio>
#include <cstdint>

static int failures = 0;

static uint8_t expected(uint8_t a, const uint8_t v[4]) {
  uint8_t y = 0;
  for (int i = 0; i < 4; i++) {
    uint8_t bit = ((uint8_t)(a + v[i]) >> 3) & 1;
    y |= bit << i;
  }
  return y & 0xf;
}

static void run(VLoopVarSliceHoist& dut, uint8_t a, const uint8_t v[4]) {
  dut.a = a;
  dut.v_0 = v[0]; dut.v_1 = v[1]; dut.v_2 = v[2]; dut.v_3 = v[3];
  dut.eval();
  uint8_t got = dut.y_index & 0xf;
  uint8_t exp = expected(a, v);
  const char* tag = (got == exp) ? "ok" : "MISMATCH";
  printf("%s a=%u v=[%u,%u,%u,%u] y_index=%u expected=%u\n",
         tag, a, v[0], v[1], v[2], v[3], got, exp);
  if (got != exp) failures++;
}

int main() {
  VLoopVarSliceHoist dut;
  uint8_t v1[4] = {8, 0, 8, 0};   // expect 0b0101 = 5
  uint8_t v2[4] = {7, 15, 0, 8};  // a=1 -> expect 0b1001 = 9
  uint8_t v3[4] = {0, 0, 0, 0};   // expect 0
  run(dut, 0, v1);
  run(dut, 1, v2);
  run(dut, 0, v3);
  if (failures) { printf("FAIL: %d mismatch(es)\n", failures); return 1; }
  printf("PASS\n");
  return 0;
}
