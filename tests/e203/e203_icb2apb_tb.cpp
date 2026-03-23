#include "VIcb2Apb.h"
#include <cstdio>
static int fail_count = 0, test_count = 0;
#define CHECK(cond, msg) do { test_count++; if (!(cond)) { printf("FAIL: %s\n", msg); fail_count++; } else { printf("PASS: %s\n", msg); } } while(0)

static void tick(VIcb2Apb &d) { d.clk = 0; d.eval(); d.clk = 1; d.eval(); }

int main() {
  VIcb2Apb d;
  d.clk = 0; d.rst_n = 0;
  d.icb_cmd_valid = 0; d.icb_cmd_addr = 0; d.icb_cmd_wdata = 0;
  d.icb_cmd_wmask = 0; d.icb_cmd_read = 0; d.icb_rsp_ready = 1;
  d.prdata = 0; d.pready = 1; d.pslverr = 0;
  for (int i = 0; i < 3; i++) tick(d);
  d.rst_n = 1; tick(d);

  // Test 1: Idle state
  d.eval();
  CHECK(d.icb_cmd_ready == 1, "Idle: cmd_ready=1");
  CHECK(d.psel == 0, "Idle: psel=0");
  CHECK(d.icb_rsp_valid == 0, "Idle: rsp_valid=0");

  // Test 2: Issue a write command
  d.icb_cmd_valid = 1; d.icb_cmd_addr = 0x40000000;
  d.icb_cmd_wdata = 0xCAFE; d.icb_cmd_wmask = 0xF; d.icb_cmd_read = 0;
  tick(d); // IDLE → SETUP
  d.icb_cmd_valid = 0;
  d.eval();
  CHECK(d.psel == 1, "Setup: psel=1");
  CHECK(d.penable == 0, "Setup: penable=0");
  CHECK(d.paddr == 0x40000000, "Setup: paddr");
  CHECK(d.pwrite == 1, "Setup: pwrite=1 (write)");

  // Test 3: ACCESS phase
  tick(d); // SETUP → ACCESS
  d.eval();
  CHECK(d.psel == 1, "Access: psel=1");
  CHECK(d.penable == 1, "Access: penable=1");
  CHECK(d.pwdata == 0xCAFE, "Access: pwdata=0xCAFE");

  // Test 4: pready → response
  d.pready = 1;
  tick(d); // ACCESS → IDLE (pready=1)
  d.eval();
  CHECK(d.icb_rsp_valid == 1, "Rsp valid after access");
  CHECK(d.icb_rsp_err == 0, "Rsp: no error");

  // Test 5: Response consumed
  tick(d);
  d.eval();
  CHECK(d.icb_rsp_valid == 0, "Rsp cleared after ready");

  // Test 6: Read transaction
  d.icb_cmd_valid = 1; d.icb_cmd_addr = 0x50000000;
  d.icb_cmd_read = 1;
  tick(d); // IDLE → SETUP
  d.icb_cmd_valid = 0;
  tick(d); // SETUP → ACCESS
  d.prdata = 0xBEEF;
  tick(d); // ACCESS → IDLE (pready=1)
  d.eval();
  CHECK(d.icb_rsp_valid == 1, "Read rsp valid");
  CHECK(d.icb_rsp_rdata == 0xBEEF, "Read rsp_rdata=0xBEEF");

  // Test 7: pready wait (slow peripheral)
  tick(d); // consume rsp
  d.icb_cmd_valid = 1; d.icb_cmd_addr = 0x6000;
  d.icb_cmd_read = 1;
  tick(d); // → SETUP
  d.icb_cmd_valid = 0;
  tick(d); // → ACCESS
  d.pready = 0; // peripheral not ready
  tick(d); // still ACCESS
  d.eval();
  CHECK(d.penable == 1, "Wait: still in access");
  CHECK(d.icb_rsp_valid == 0, "Wait: no rsp yet");
  d.pready = 1; d.prdata = 0x1234;
  tick(d); // → IDLE
  d.eval();
  CHECK(d.icb_rsp_valid == 1, "Wait done: rsp valid");
  CHECK(d.icb_rsp_rdata == 0x1234, "Wait done: rdata=0x1234");

  // Test 8: APB error
  tick(d);
  d.icb_cmd_valid = 1; d.icb_cmd_addr = 0x7000; d.icb_cmd_read = 1;
  tick(d); d.icb_cmd_valid = 0;
  tick(d); // ACCESS
  d.pslverr = 1; d.pready = 1;
  tick(d);
  d.eval();
  CHECK(d.icb_rsp_err == 1, "APB error: rsp_err=1");

  printf("\n=== Icb2Apb: %d/%d passed ===\n", test_count - fail_count, test_count);
  return fail_count ? 1 : 0;
}
