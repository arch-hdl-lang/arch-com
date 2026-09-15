// Minimal TB for arch#1019 regression: construct + eval is enough to force
// the host compiler to build the generated model, which is where the
// undeclared-`din` error surfaced pre-fix.
#include "VFsmVecPortTrace.h"
#include <cstdio>

int main() {
  VFsmVecPortTrace dut;
  dut.rst = 1; dut.eval();
  dut.rst = 0; dut.start = 1; dut.din_0 = 42; dut.eval();
  dut.start = 0; dut.eval(); dut.eval();
  printf("ok dout=%llu\n", (unsigned long long)dut.dout);
  return 0;
}
