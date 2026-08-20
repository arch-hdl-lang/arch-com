#include "VUe4m3Probe.h"
#include <cstdio>
#include <cstring>
#include <cmath>
static VUe4m3Probe dut;
static uint32_t B(float f){ uint32_t b; memcpy(&b,&f,4); return b; }
static float FL(uint32_t b){ float f; memcpy(&f,&b,4); return f; }
int main(){
  int bad = 0;
  // The property that caught 4 of the 5 earlier scale-type bugs: the bit
  // test and the widen must agree on what NaN is.
  for (int c = 0; c < 256; ++c) {
    dut.s = (uint8_t)c; dut.eval();
    float v = FL(dut.val);
    bool by_widen = std::isnan(v);
    bool by_test  = dut.nan != 0;
    if (by_widen != by_test) { printf("MISMATCH code 0x%02x: widen nan=%d test=%d\n", c, by_widen, by_test); ++bad; }
    if (!by_widen && v < 0.0f) { printf("NEGATIVE scale at 0x%02x: %g\n", c, v); ++bad; }
  }
  // Round trip on exactly representable scales.
  for (int c = 0; c <= 0x7E; ++c) {
    dut.s = (uint8_t)c; dut.eval();
    float v = FL(dut.val);
    dut.v = B(v); dut.eval();
    if (dut.enc != c) { printf("ROUNDTRIP 0x%02x -> %g -> 0x%02x\n", c, v, dut.enc); ++bad; }
  }
  printf(bad ? "FAIL %d\n" : "ALL PASS\n", bad);
  return bad ? 1 : 0;
}
