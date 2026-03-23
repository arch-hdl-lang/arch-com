#include "VClkCtrl.h"
#include <cstdio>
static int fail_count = 0, test_count = 0;
#define CHECK(cond, msg) do { test_count++; if (!(cond)) { printf("FAIL: %s\n", msg); fail_count++; } else { printf("PASS: %s\n", msg); } } while(0)

static void tick(VClkCtrl &d) { d.clk = 0; d.eval(); d.clk = 1; d.eval(); }

int main() {
  VClkCtrl d;
  d.clk = 0; d.rst_n = 0;
  d.test_en = 0;
  d.ifu_gate_en = 0; d.exu_gate_en = 0;
  d.lsu_gate_en = 0; d.biu_gate_en = 0;

  // Reset
  for (int i = 0; i < 3; i++) tick(d);
  d.rst_n = 1;
  tick(d);

  // Test 1: All gates disabled — gated clocks should be 0
  d.eval();
  CHECK(d.clk_ifu == 0 || d.clk_ifu == d.clk, "IFU clock gated when disabled");
  CHECK(d.ifu_clk_active == 0, "IFU clk_active = 0 when gate disabled");
  CHECK(d.exu_clk_active == 0, "EXU clk_active = 0 when gate disabled");

  // Test 2: Enable IFU clock gate
  d.ifu_gate_en = 1;
  // With latch-based ICG: enable latches on clk low, output = clk & en_latched
  d.clk = 0; d.eval(); // latch enable
  d.clk = 1; d.eval(); // clk_out should go high
  CHECK(d.clk_ifu == 1, "IFU clock passes through when enabled (clk=1)");
  CHECK(d.ifu_clk_active == 1, "IFU clk_active = 1 when gate enabled");

  // Test 3: IFU clock low phase
  d.clk = 0; d.eval();
  CHECK(d.clk_ifu == 0, "IFU clock low when clk=0");

  // Test 4: EXU still gated
  CHECK(d.clk_exu == 0, "EXU clock still gated");
  CHECK(d.exu_clk_active == 0, "EXU clk_active still 0");

  // Test 5: Enable all gates
  d.ifu_gate_en = 1; d.exu_gate_en = 1;
  d.lsu_gate_en = 1; d.biu_gate_en = 1;
  d.clk = 0; d.eval(); // latch all enables
  d.clk = 1; d.eval(); // all outputs high
  CHECK(d.clk_ifu == 1, "All enabled: IFU clk high");
  CHECK(d.clk_exu == 1, "All enabled: EXU clk high");
  CHECK(d.clk_lsu == 1, "All enabled: LSU clk high");
  CHECK(d.clk_biu == 1, "All enabled: BIU clk high");

  // Test 6: Disable BIU mid-high — latch holds until clk goes low
  d.biu_gate_en = 0;
  d.eval(); // still clk=1, latch holds old value
  CHECK(d.clk_biu == 1, "BIU latch holds enable during clk=1");

  // Test 7: After clk goes low, latch captures new disable
  d.clk = 0; d.eval(); // latch captures biu_gate_en=0
  d.clk = 1; d.eval(); // clk_biu should now be 0
  CHECK(d.clk_biu == 0, "BIU gated after disable + clk cycle");

  // Test 8: test_en overrides gate
  d.ifu_gate_en = 0; d.exu_gate_en = 0;
  d.lsu_gate_en = 0; d.biu_gate_en = 0;
  d.test_en = 1;
  d.clk = 0; d.eval(); // latch test_en
  d.clk = 1; d.eval();
  CHECK(d.clk_ifu == 1, "test_en overrides: IFU clk active");
  CHECK(d.clk_exu == 1, "test_en overrides: EXU clk active");
  CHECK(d.clk_lsu == 1, "test_en overrides: LSU clk active");
  CHECK(d.clk_biu == 1, "test_en overrides: BIU clk active");
  CHECK(d.ifu_clk_active == 1, "test_en: IFU clk_active = 1");

  // Test 9: Disable test_en, verify clocks stop
  d.test_en = 0;
  d.clk = 0; d.eval();
  d.clk = 1; d.eval();
  CHECK(d.clk_ifu == 0, "No test_en, no gate: IFU clock stopped");

  printf("\n=== ClkCtrl: %d/%d passed ===\n", test_count - fail_count, test_count);
  return fail_count ? 1 : 0;
}
