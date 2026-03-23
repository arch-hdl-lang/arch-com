#include "VIrqCtrl.h"
#include <cstdio>
static int fail_count = 0, test_count = 0;
#define CHECK(cond, msg) do { test_count++; if (!(cond)) { printf("FAIL: %s\n", msg); fail_count++; } else { printf("PASS: %s\n", msg); } } while(0)

int main() {
  VIrqCtrl d;
  d.clk = 0; d.rst_n = 1;
  d.ext_irq_i = 0; d.sw_irq_i = 0; d.tmr_irq_i = 0;
  d.mstatus_mie = 0; d.mie_meie = 0; d.mie_mtie = 0; d.mie_msie = 0;
  d.pipe_flush_ack = 0; d.commit_valid = 1;
  d.eval();

  // Test 1: No interrupts
  CHECK(d.irq_req == 0, "No IRQ: irq_req=0");

  // Test 2: Timer IRQ but global disable
  d.tmr_irq_i = 1; d.mie_mtie = 1; d.mstatus_mie = 0;
  d.eval();
  CHECK(d.irq_req == 0, "Timer+MIE=0: irq_req=0");
  CHECK(d.mip_mtip == 1, "mip_mtip reflects tmr_irq");

  // Test 3: Timer IRQ with global enable
  d.mstatus_mie = 1;
  d.eval();
  CHECK(d.irq_req == 1, "Timer+MIE=1: irq_req=1");
  CHECK(d.irq_cause == 0x80000007, "Timer cause=0x80000007");

  // Test 4: External IRQ has higher priority
  d.ext_irq_i = 1; d.mie_meie = 1;
  d.eval();
  CHECK(d.irq_cause == 0x8000000B, "Ext>Timer: cause=0x8000000B");

  // Test 5: Software IRQ only
  d.ext_irq_i = 0; d.mie_meie = 0; d.tmr_irq_i = 0; d.mie_mtie = 0;
  d.sw_irq_i = 1; d.mie_msie = 1;
  d.eval();
  CHECK(d.irq_req == 1, "SW IRQ: irq_req=1");
  CHECK(d.irq_cause == 0x80000003, "SW cause=0x80000003");

  // Test 6: mip reflects external
  d.ext_irq_i = 1;
  d.eval();
  CHECK(d.mip_meip == 1, "mip_meip=1");

  // Test 7: Pipeline not ready (commit_valid=0, flush_ack=0)
  d.commit_valid = 0; d.pipe_flush_ack = 0;
  d.eval();
  CHECK(d.irq_req == 0, "Pipeline busy: irq_req=0");

  // Test 8: Pipeline flush_ack
  d.pipe_flush_ack = 1;
  d.eval();
  CHECK(d.irq_req == 1, "flush_ack: irq_req=1");

  printf("\n=== IrqCtrl: %d/%d passed ===\n", test_count - fail_count, test_count);
  return fail_count ? 1 : 0;
}
