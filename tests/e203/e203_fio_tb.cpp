#include "VFio.h"
#include <cstdio>
static int fail_count = 0, test_count = 0;
#define CHECK(cond, msg) do { test_count++; if (!(cond)) { printf("FAIL: %s\n", msg); fail_count++; } else { printf("PASS: %s\n", msg); } } while(0)

static void tick(VFio &d) { d.clk = 0; d.eval(); d.clk = 1; d.eval(); }

int main() {
  VFio d;
  d.clk = 0; d.rst_n = 0;
  d.icb_cmd_valid = 0; d.icb_cmd_addr = 0; d.icb_cmd_wdata = 0;
  d.icb_cmd_wmask = 0; d.icb_cmd_read = 0; d.icb_rsp_ready = 1;
  d.fio_in_0 = 0; d.fio_in_1 = 0;
  for (int i = 0; i < 3; i++) tick(d);
  d.rst_n = 1; tick(d);

  // Test 1: Write reg 0 (addr offset 0x00)
  d.icb_cmd_valid = 1; d.icb_cmd_addr = 0x00; d.icb_cmd_wdata = 0xAAAA; d.icb_cmd_read = 0;
  tick(d); d.icb_cmd_valid = 0;
  d.eval();
  CHECK(d.fio_out_0 == 0xAAAA, "Write reg0: fio_out_0=0xAAAA");

  // Test 2: Write reg 1 (addr offset 0x04)
  d.icb_cmd_valid = 1; d.icb_cmd_addr = 0x04; d.icb_cmd_wdata = 0xBBBB; d.icb_cmd_read = 0;
  tick(d); d.icb_cmd_valid = 0;
  d.eval();
  CHECK(d.fio_out_1 == 0xBBBB, "Write reg1: fio_out_1=0xBBBB");

  // Test 3: Read reg 0 back
  d.icb_cmd_valid = 1; d.icb_cmd_addr = 0x00; d.icb_cmd_read = 1;
  tick(d); d.icb_cmd_valid = 0;
  d.eval();
  CHECK(d.icb_rsp_valid == 1, "Read reg0: rsp_valid");
  CHECK(d.icb_rsp_rdata == 0xAAAA, "Read reg0: rdata=0xAAAA");

  // Test 4: Read input reg (offset 0x20 = idx 8)
  d.fio_in_0 = 0x12345678;
  d.icb_cmd_valid = 1; d.icb_cmd_addr = 0x20; d.icb_cmd_read = 1;
  tick(d); d.icb_cmd_valid = 0;
  d.eval();
  CHECK(d.icb_rsp_rdata == 0x12345678, "Read fio_in_0: 0x12345678");

  // Test 5: Read input reg 1 (offset 0x24 = idx 9)
  d.fio_in_1 = 0xDEADBEEF;
  d.icb_cmd_valid = 1; d.icb_cmd_addr = 0x24; d.icb_cmd_read = 1;
  tick(d); d.icb_cmd_valid = 0;
  d.eval();
  CHECK(d.icb_rsp_rdata == 0xDEADBEEF, "Read fio_in_1: 0xDEADBEEF");

  // Test 6: Always ready
  CHECK(d.icb_cmd_ready == 1, "Always ready");

  printf("\n=== FIO: %d/%d passed ===\n", test_count - fail_count, test_count);
  return fail_count ? 1 : 0;
}
