#include "VVsel.h"
#include <cstdio>
// Variable-index Vec<Bus> comb write: for each sel, lane[sel] must carry
// (valid=1, data=dv) and every other lane must stay (0,0).
int main() {
  bool ok = true;
  for (int sel = 0; sel < 4; sel++) {
    VVsel d;
    d.sel = sel;
    d.dv = 0xAB;
    d.eval();
    for (int l = 0; l < 4; l++) {
      int exp_v = (l == sel) ? 1 : 0;
      int exp_d = (l == sel) ? 0xAB : 0;
      if (d.o_valid[l] != exp_v || d.o_data[l] != exp_d) {
        printf("MISMATCH sel=%d lane=%d valid=%d(exp %d) data=%x(exp %x)\n",
               sel, l, d.o_valid[l], exp_v, d.o_data[l], exp_d);
        ok = false;
      }
    }
  }
  printf(ok ? "PASS vsel_varidx\n" : "FAIL vsel_varidx\n");
  return ok ? 0 : 1;
}
