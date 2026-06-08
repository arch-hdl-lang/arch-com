#include "VVselWr.h"
#include <cstdio>
// Registered one-hot drive: after a few cycles, lane `sel` carries
// (valid=1, data=dv), every other lane stays (0,0).
int main() {
  bool ok = true;
  for (int sel = 0; sel < 4; sel++) {
    VVselWr d; d.rst = 0; d.clk = 0; d.sel = sel; d.dv = 0x5A; d.eval(); d.rst = 1;
    for (int i = 0; i < 3; i++) { d.clk = 0; d.eval(); d.clk = 1; d.eval(); }
    for (int l = 0; l < 4; l++) {
      int ev = (l == sel) ? 1 : 0, ed = (l == sel) ? 0x5A : 0;
      if (d.o_valid[l] != ev || d.o_data[l] != ed) {
        printf("MISMATCH sel=%d l=%d v=%d(exp %d) d=%x(exp %x)\n", sel, l, d.o_valid[l], ev, d.o_data[l], ed);
        ok = false;
      }
    }
  }
  printf(ok ? "PASS vselwr_varidx\n" : "FAIL vselwr_varidx\n");
  return ok ? 0 : 1;
}
