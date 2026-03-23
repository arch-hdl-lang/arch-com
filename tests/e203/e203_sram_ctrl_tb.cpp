#include "VSramCtrl.h"
#include <cstdio>
static int fail_count = 0, test_count = 0;
#define CHECK(cond, msg) do { test_count++; if (!(cond)) { printf("FAIL: %s\n", msg); fail_count++; } else { printf("PASS: %s\n", msg); } } while(0)

static void tick(VSramCtrl &d) { d.clk = 0; d.eval(); d.clk = 1; d.eval(); }

int main() {
  VSramCtrl d;
  d.clk = 0; d.rst_n = 0;
  d.icb_cmd_valid = 0; d.icb_cmd_addr = 0; d.icb_cmd_wdata = 0;
  d.icb_cmd_wmask = 0; d.icb_cmd_read = 0; d.icb_rsp_ready = 1;
  for (int i = 0; i < 3; i++) tick(d);
  d.rst_n = 1; tick(d);

  // Test 1: Always ready
  d.eval();
  CHECK(d.icb_cmd_ready == 1, "Always ready");

  // Test 2: Write word to address 0x100 (word index = 0x40)
  d.icb_cmd_valid = 1; d.icb_cmd_addr = 0x100;
  d.icb_cmd_wdata = 0xDEADBEEF; d.icb_cmd_wmask = 0xF; d.icb_cmd_read = 0;
  tick(d);
  d.icb_cmd_valid = 0;
  d.eval();
  CHECK(d.icb_rsp_valid == 1, "Write: rsp_valid after 1 cycle");

  // Test 3: Read back from same address
  d.icb_cmd_valid = 1; d.icb_cmd_addr = 0x100; d.icb_cmd_read = 1;
  tick(d);
  d.icb_cmd_valid = 0;
  d.eval();
  CHECK(d.icb_rsp_valid == 1, "Read: rsp_valid after 1 cycle");
  CHECK(d.icb_rsp_rdata == 0xDEADBEEF, "Read: data=0xDEADBEEF");

  // Test 4: Write another address
  d.icb_cmd_valid = 1; d.icb_cmd_addr = 0x200;
  d.icb_cmd_wdata = 0x12345678; d.icb_cmd_read = 0;
  tick(d);
  d.icb_cmd_valid = 0;

  // Test 5: Read first address still intact
  d.icb_cmd_valid = 1; d.icb_cmd_addr = 0x100; d.icb_cmd_read = 1;
  tick(d);
  d.icb_cmd_valid = 0;
  d.eval();
  CHECK(d.icb_rsp_rdata == 0xDEADBEEF, "Addr 0x100 still = 0xDEADBEEF");

  // Test 6: Read second address
  d.icb_cmd_valid = 1; d.icb_cmd_addr = 0x200; d.icb_cmd_read = 1;
  tick(d);
  d.icb_cmd_valid = 0;
  d.eval();
  CHECK(d.icb_rsp_rdata == 0x12345678, "Addr 0x200 = 0x12345678");

  // Test 7: No error
  CHECK(d.icb_rsp_err == 0, "No error");

  // Test 8: No cmd → no rsp next cycle
  tick(d);
  d.eval();
  CHECK(d.icb_rsp_valid == 0, "Idle: rsp_valid=0");

  printf("\n=== SramCtrl: %d/%d passed ===\n", test_count - fail_count, test_count);
  return fail_count ? 1 : 0;
}
