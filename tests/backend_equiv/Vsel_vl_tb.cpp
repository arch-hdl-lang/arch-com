#include "VVsel.h"
#include <verilated.h>
#include <cstdio>
int main() {
  bool ok = true;
  for (int sel = 0; sel < 4; sel++) {
    VVsel* d = new VVsel;
    d->sel = sel;
    d->dv = 0xAB;
    d->eval();
    for (int l = 0; l < 4; l++) {
      int got_v = (d->o_valid >> l) & 1;
      int got_d = (d->o_data >> (l * 8)) & 0xff;
      int exp_v = (l == sel) ? 1 : 0;
      int exp_d = (l == sel) ? 0xAB : 0;
      if (got_v != exp_v || got_d != exp_d) {
        printf("MISMATCH sel=%d lane=%d valid=%d(exp %d) data=%x(exp %x)\n",
               sel, l, got_v, exp_v, got_d, exp_d);
        ok = false;
      }
    }
    delete d;
  }
  printf(ok ? "PASS vsel_varidx\n" : "FAIL vsel_varidx\n");
  return ok ? 0 : 1;
}
