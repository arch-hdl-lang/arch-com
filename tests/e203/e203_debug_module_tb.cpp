#include "VDebugModule.h"
#include <cstdio>
static int fail_count = 0, test_count = 0;
#define CHECK(cond, msg) do { test_count++; if (!(cond)) { printf("FAIL: %s\n", msg); fail_count++; } else { printf("PASS: %s\n", msg); } } while(0)

static void tick(VDebugModule &d) { d.clk = 0; d.eval(); d.clk = 1; d.eval(); }
static void apb_write(VDebugModule &d, uint32_t addr, uint32_t data) {
  d.psel = 1; d.penable = 0; d.paddr = addr; d.pwdata = data; d.pwrite = 1;
  tick(d);
  d.penable = 1; tick(d);
  d.psel = 0; d.penable = 0;
}
static uint32_t apb_read(VDebugModule &d, uint32_t addr) {
  d.psel = 1; d.penable = 0; d.paddr = addr; d.pwrite = 0;
  tick(d);
  d.penable = 1; d.eval();
  uint32_t v = d.prdata;
  tick(d);
  d.psel = 0; d.penable = 0;
  return v;
}

int main() {
  VDebugModule d;
  d.clk = 0; d.rst_n = 0;
  d.psel = 0; d.penable = 0; d.paddr = 0; d.pwdata = 0; d.pwrite = 0;
  d.hart_halted = 0; d.hart_running = 1;
  d.dbg_reg_rdata = 0;
  for (int i = 0; i < 3; i++) tick(d);
  d.rst_n = 1; tick(d);

  // Test 1: pready always 1
  d.eval();
  CHECK(d.pready == 1, "pready=1");

  // Test 2: Initially no halt/resume request
  CHECK(d.halt_req == 0, "No halt_req initially");
  CHECK(d.resume_req == 0, "No resume_req initially");

  // Test 3: Activate DM (dmcontrol bit 0 = dmactive)
  apb_write(d, 0x10, 0x00000001);
  uint32_t v = apb_read(d, 0x10);
  CHECK((v & 1) == 1, "dmactive=1");

  // Test 4: Request halt (bit 31 of dmcontrol)
  apb_write(d, 0x10, 0x80000001); // haltreq + dmactive
  d.eval();
  CHECK(d.halt_req == 1, "halt_req asserted");

  // Test 5: Read dmstatus - core is running
  v = apb_read(d, 0x11);
  CHECK((v & 0x0C) != 0, "dmstatus: running bits set");

  // Test 6: Core halts
  d.hart_halted = 1; d.hart_running = 0;
  v = apb_read(d, 0x11);
  CHECK((v & 0x300) != 0, "dmstatus: halted bits set");

  // Test 7: Write data0
  apb_write(d, 0x04, 0xDEADBEEF);
  v = apb_read(d, 0x04);
  CHECK(v == 0xDEADBEEF, "data0 = 0xDEADBEEF");

  // Test 8: Issue register write command (bit 16=write, reg addr in [15:0])
  apb_write(d, 0x17, 0x00010301); // write=1, regno=0x0301 (mstatus)
  d.eval();
  CHECK(d.dbg_reg_wen == 1, "dbg_reg_wen=1");
  CHECK(d.dbg_reg_addr == 0x0301, "dbg_reg_addr=0x0301");
  CHECK(d.dbg_reg_wdata == 0xDEADBEEF, "dbg_reg_wdata=data0");
  tick(d); // cmd auto-clears
  d.eval();
  CHECK(d.dbg_reg_wen == 0, "dbg_reg_wen cleared after 1 cycle");

  // Test 9: Issue register read command
  d.dbg_reg_rdata = 0x12345678;
  apb_write(d, 0x17, 0x00000300); // write=0, regno=0x0300
  tick(d); // cmd_valid_r is set, next cycle captures rdata into data0
  d.eval();
  // data0 should now have dbg_reg_rdata
  v = apb_read(d, 0x04);
  printf("  data0 after reg read = 0x%08X\n", v);
  CHECK(v == 0x12345678, "data0 captured reg read");

  // Test 10: Resume request (bit 30 of dmcontrol)
  apb_write(d, 0x10, 0x40000001); // resumereq + dmactive
  d.eval();
  CHECK(d.resume_req == 1, "resume_req asserted");
  tick(d); // auto-clear
  d.eval();
  CHECK(d.resume_req == 0, "resume_req auto-cleared");

  // Test 11: Deactivate DM
  apb_write(d, 0x10, 0x80000000); // haltreq but dmactive=0
  d.eval();
  CHECK(d.halt_req == 0, "halt_req gated by dmactive");

  printf("\n=== DebugModule: %d/%d passed ===\n", test_count - fail_count, test_count);
  return fail_count ? 1 : 0;
}
