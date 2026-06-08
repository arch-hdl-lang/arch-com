#include "VVselWr.h"
#include <verilated.h>
#include <cstdio>
int main() {
  bool ok = true;
  for (int sel = 0; sel < 4; sel++) {
    VVselWr* d = new VVselWr; d->rst = 0; d->clk = 0; d->sel = sel; d->dv = 0x5A; d->eval(); d->rst = 1;
    for (int i = 0; i < 3; i++) { d->clk = 0; d->eval(); d->clk = 1; d->eval(); }
    for (int l = 0; l < 4; l++) {
      int gv = (d->o_valid >> l) & 1, gd = (d->o_data >> (l * 8)) & 0xff;
      int ev = (l == sel) ? 1 : 0, ed = (l == sel) ? 0x5A : 0;
      if (gv != ev || gd != ed) {
        printf("MISMATCH sel=%d l=%d v=%d(exp %d) d=%x(exp %x)\n", sel, l, gv, ev, gd, ed);
        ok = false;
      }
    }
    delete d;
  }
  printf(ok ? "PASS vselwr_varidx\n" : "FAIL vselwr_varidx\n");
  return ok ? 0 : 1;
}
