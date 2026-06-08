#include "VVselThread.h"
#include <cstdio>
// `wait until o[sel].ready` must observe ready on the selected lane only.
// For each sel, drive ready on lane==sel starting cycle 2; `done` must
// pulse exactly at cycle 2 regardless of sel.
int main() {
  bool ok = true;
  for (int sel = 0; sel < 4; sel++) {
    VVselThread d;
    d.rst = 0; d.clk = 0; d.sel = sel; d.dv = 0x33; d.eval();
    d.rst = 1;
    int fired = -1;
    for (int i = 0; i < 5; i++) {
      for (int l = 0; l < 4; l++) d.o_ready[l] = 0;
      if (i >= 2) d.o_ready[sel] = 1;
      d.clk = 0; d.eval(); d.clk = 1; d.eval();
      if (d.done && fired < 0) fired = i;
    }
    if (fired != 2) { printf("MISMATCH sel=%d done_fired_at=%d (exp 2)\n", sel, fired); ok = false; }
  }
  printf(ok ? "PASS vsel_thread_varidx\n" : "FAIL vsel_thread_varidx\n");
  return ok ? 0 : 1;
}
