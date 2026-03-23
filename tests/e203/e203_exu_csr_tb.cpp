#include "VExuCsr.h"
#include <cstdio>
#include <cstdlib>

static int fail_count = 0;
static int test_count = 0;

#define CHECK(cond, msg) do { \
  test_count++; \
  if (!(cond)) { printf("FAIL: %s\n", msg); fail_count++; } \
  else { printf("PASS: %s\n", msg); } \
} while(0)

static void tick(VExuCsr &d) {
  d.clk = 0; d.eval();
  d.clk = 1; d.eval();
}

int main() {
  VExuCsr dut;
  dut.clk = 0; dut.rst_n = 0;
  dut.csr_addr = 0; dut.csr_wen = 0; dut.csr_wdata = 0;
  dut.trap_taken = 0; dut.trap_cause = 0; dut.trap_pc = 0; dut.trap_val = 0;
  dut.mret_taken = 0; dut.ext_irq = 0; dut.sw_irq = 0; dut.tmr_irq = 0;

  // Reset
  for (int i = 0; i < 3; i++) tick(dut);
  dut.rst_n = 1;
  tick(dut);

  // Test 1: Read mstatus after reset (should be 0)
  dut.csr_addr = 0x300; dut.csr_wen = 0;
  dut.eval();
  CHECK(dut.csr_rdata == 0, "mstatus=0 after reset");

  // Test 2: Write mtvec
  dut.csr_addr = 0x305; dut.csr_wen = 1; dut.csr_wdata = 0x80000100;
  tick(dut);
  dut.csr_wen = 0;
  dut.eval();
  CHECK(dut.mtvec_val == 0x80000100, "mtvec write: 0x80000100");

  // Test 3: Write mscratch and read back
  dut.csr_addr = 0x340; dut.csr_wen = 1; dut.csr_wdata = 0xCAFEBABE;
  tick(dut);
  dut.csr_wen = 0; dut.csr_addr = 0x340;
  dut.eval();
  CHECK(dut.csr_rdata == 0xCAFEBABE, "mscratch read: 0xCAFEBABE");

  // Test 4: Write mstatus with MIE=1 (bit 3)
  dut.csr_addr = 0x300; dut.csr_wen = 1; dut.csr_wdata = 0x8;
  tick(dut);
  dut.csr_wen = 0;
  dut.eval();
  CHECK(dut.mstatus_mie == 1, "mstatus MIE=1");

  // Test 5: Write mie to enable timer interrupt (bit 7)
  dut.csr_addr = 0x304; dut.csr_wen = 1; dut.csr_wdata = 0x80;
  tick(dut);
  dut.csr_wen = 0;

  // Test 6: Timer IRQ → irq_pending
  dut.tmr_irq = 1;
  dut.eval();
  CHECK(dut.irq_pending == 1, "Timer IRQ: irq_pending=1");

  // Test 7: Read mip reflects timer
  dut.csr_addr = 0x344;
  dut.eval();
  CHECK((dut.csr_rdata & 0x80) != 0, "mip: MTIP bit set");

  // Test 8: Trap entry
  dut.tmr_irq = 0;
  dut.trap_taken = 1; dut.trap_cause = 0x80000007; // timer interrupt
  dut.trap_pc = 0x80001000; dut.trap_val = 0;
  tick(dut);
  dut.trap_taken = 0;
  // mepc should be saved
  dut.csr_addr = 0x341;
  dut.eval();
  CHECK(dut.mepc_val == 0x80001000, "Trap: mepc saved");
  CHECK(dut.mstatus_mie == 0, "Trap: MIE cleared");

  // Test 9: mcause saved
  dut.csr_addr = 0x342;
  dut.eval();
  CHECK(dut.csr_rdata == 0x80000007, "Trap: mcause=timer irq");

  // Test 10: MRET restores MIE
  dut.mret_taken = 1;
  tick(dut);
  dut.mret_taken = 0;
  dut.eval();
  CHECK(dut.mstatus_mie == 1, "MRET: MIE restored");

  // Test 11: mcycle increments
  dut.csr_addr = 0xB00;
  dut.eval();
  uint32_t cycle1 = dut.csr_rdata;
  tick(dut);
  dut.eval();
  uint32_t cycle2 = dut.csr_rdata;
  CHECK(cycle2 == cycle1 + 1, "mcycle increments");

  // Test 12: Write mepc directly
  dut.csr_addr = 0x341; dut.csr_wen = 1; dut.csr_wdata = 0xABCD0000;
  tick(dut);
  dut.csr_wen = 0;
  dut.eval();
  CHECK(dut.mepc_val == 0xABCD0000, "mepc write: 0xABCD0000");

  // Test 13: External IRQ in mip
  dut.ext_irq = 1;
  dut.csr_addr = 0x344;
  dut.eval();
  CHECK((dut.csr_rdata & 0x800) != 0, "mip: MEIP bit set");
  dut.ext_irq = 0;

  // Test 14: Unknown CSR reads 0
  dut.csr_addr = 0xFFF;
  dut.eval();
  CHECK(dut.csr_rdata == 0, "Unknown CSR reads 0");

  printf("\n=== CSR: %d/%d passed ===\n", test_count - fail_count, test_count);
  return fail_count ? 1 : 0;
}
