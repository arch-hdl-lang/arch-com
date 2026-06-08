#include "VVselThread.h"
#include <verilated.h>
#include <cstdio>
int main() {
  bool ok = true;
  for (int sel = 0; sel < 4; sel++) {
    VVselThread* d = new VVselThread;
    d->rst = 0; d->clk = 0; d->sel = sel; d->dv = 0x33; d->eval();
    d->rst = 1;
    int fired = -1;
    for (int i = 0; i < 5; i++) {
      d->o_ready = (i >= 2) ? (1u << sel) : 0;
      d->clk = 0; d->eval(); d->clk = 1; d->eval();
      if (d->done && fired < 0) fired = i;
    }
    if (fired != 2) { printf("MISMATCH sel=%d done_fired_at=%d (exp 2)\n", sel, fired); ok = false; }
    delete d;
  }
  printf(ok ? "PASS vsel_thread_varidx\n" : "FAIL vsel_thread_varidx\n");
  return ok ? 0 : 1;
}
