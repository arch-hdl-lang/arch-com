#include "VPpi.h"
#include <cstdio>
static int fail_count = 0, test_count = 0;
#define CHECK(cond, msg) do { test_count++; if (!(cond)) { printf("FAIL: %s\n", msg); fail_count++; } else { printf("PASS: %s\n", msg); } } while(0)

static void tick(VPpi &d) { d.clk = 0; d.eval(); d.clk = 1; d.eval(); }

int main() {
  VPpi d;
  d.clk = 0; d.rst_n = 0;
  d.icb_cmd_valid = 0; d.icb_cmd_addr = 0; d.icb_cmd_wdata = 0;
  d.icb_cmd_wmask = 0; d.icb_cmd_read = 0; d.icb_rsp_ready = 1;
  d.apb0_prdata = 0; d.apb0_pready = 1;
  d.apb1_prdata = 0; d.apb1_pready = 1;
  d.apb2_prdata = 0; d.apb2_pready = 1;
  d.apb3_prdata = 0; d.apb3_pready = 1;
  for (int i = 0; i < 3; i++) tick(d);
  d.rst_n = 1; tick(d);

  // Test 1: Idle
  d.eval();
  CHECK(d.icb_cmd_ready == 1, "Idle: cmd_ready");

  // Test 2: Write to GPIO (0x10012000)
  d.icb_cmd_valid = 1; d.icb_cmd_addr = 0x10012000;
  d.icb_cmd_wdata = 0xFF; d.icb_cmd_read = 0;
  tick(d); d.icb_cmd_valid = 0;
  // SETUP
  d.eval();
  CHECK(d.apb0_psel == 1, "GPIO: psel=1");
  CHECK(d.apb0_penable == 0, "GPIO setup: penable=0");
  tick(d); // ACCESS
  d.eval();
  CHECK(d.apb0_penable == 1, "GPIO access: penable=1");
  CHECK(d.apb0_pwrite == 1, "GPIO: pwrite=1");
  tick(d); // complete
  d.eval();
  CHECK(d.icb_rsp_valid == 1, "GPIO write: rsp_valid");

  // Test 3: Read from UART (0x10013000)
  tick(d); // consume rsp
  d.icb_cmd_valid = 1; d.icb_cmd_addr = 0x10013004;
  d.icb_cmd_read = 1;
  tick(d); d.icb_cmd_valid = 0;
  tick(d); // ACCESS
  d.apb1_prdata = 0xABCD;
  tick(d); // complete
  d.eval();
  CHECK(d.icb_rsp_valid == 1, "UART read: rsp_valid");
  CHECK(d.icb_rsp_rdata == 0xABCD, "UART read: rdata=0xABCD");

  // Test 4: SPI (0x10014000) — psel goes to apb2
  tick(d);
  d.icb_cmd_valid = 1; d.icb_cmd_addr = 0x10014008; d.icb_cmd_read = 0;
  tick(d); d.icb_cmd_valid = 0;
  d.eval();
  CHECK(d.apb2_psel == 1, "SPI: apb2_psel=1");
  CHECK(d.apb0_psel == 0, "SPI: apb0_psel=0");
  tick(d); tick(d);

  // Test 5: Timer (0x02000000)
  tick(d);
  d.icb_cmd_valid = 1; d.icb_cmd_addr = 0x02000000; d.icb_cmd_read = 1;
  tick(d); d.icb_cmd_valid = 0;
  d.eval();
  CHECK(d.apb3_psel == 1, "Timer: apb3_psel=1");
  tick(d); tick(d);

  // Test 6: No error
  d.eval();
  CHECK(d.icb_rsp_err == 0, "No error");

  printf("\n=== PPI: %d/%d passed ===\n", test_count - fail_count, test_count);
  return fail_count ? 1 : 0;
}
