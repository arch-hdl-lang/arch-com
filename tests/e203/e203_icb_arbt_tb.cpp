#include "VIcbArbt.h"
#include <cstdio>
static int fail_count = 0, test_count = 0;
#define CHECK(cond, msg) do { test_count++; if (!(cond)) { printf("FAIL: %s\n", msg); fail_count++; } else { printf("PASS: %s\n", msg); } } while(0)

static void tick(VIcbArbt &d) { d.clk = 0; d.eval(); d.clk = 1; d.eval(); }

int main() {
  VIcbArbt d;
  d.clk = 0; d.rst_n = 0;
  d.m0_cmd_valid = 0; d.m0_cmd_addr = 0; d.m0_cmd_wdata = 0; d.m0_cmd_wmask = 0; d.m0_cmd_read = 0; d.m0_rsp_ready = 1;
  d.m1_cmd_valid = 0; d.m1_cmd_addr = 0; d.m1_cmd_wdata = 0; d.m1_cmd_wmask = 0; d.m1_cmd_read = 0; d.m1_rsp_ready = 1;
  d.s_cmd_ready = 1; d.s_rsp_valid = 0; d.s_rsp_rdata = 0; d.s_rsp_err = 0;
  for (int i = 0; i < 3; i++) tick(d);
  d.rst_n = 1; tick(d);

  // Test 1: M0 only
  d.m0_cmd_valid = 1; d.m0_cmd_addr = 0x1000; d.m0_cmd_read = 1;
  d.eval();
  CHECK(d.s_cmd_valid == 1, "M0 only: s_cmd_valid=1");
  CHECK(d.s_cmd_addr == 0x1000, "M0 only: addr=0x1000");
  CHECK(d.m0_cmd_ready == 1, "M0 only: m0_ready=1");
  CHECK(d.m1_cmd_ready == 0, "M0 only: m1_ready=0");
  tick(d);

  // Test 2: M1 only
  d.m0_cmd_valid = 0; d.m1_cmd_valid = 1; d.m1_cmd_addr = 0x2000;
  d.eval();
  CHECK(d.s_cmd_addr == 0x2000, "M1 only: addr=0x2000");
  CHECK(d.m1_cmd_ready == 1, "M1 only: m1_ready=1");
  tick(d);

  // Test 3: Both request — M0 wins first (last_grant was m1)
  d.m0_cmd_valid = 1; d.m0_cmd_addr = 0xA000;
  d.m1_cmd_valid = 1; d.m1_cmd_addr = 0xB000;
  d.eval();
  CHECK(d.s_cmd_addr == 0xA000, "Both: M0 wins (round-robin)");
  CHECK(d.m0_cmd_ready == 1, "Both: m0_ready=1");
  CHECK(d.m1_cmd_ready == 0, "Both: m1_ready=0");
  tick(d);

  // Test 4: Both request again — M1 wins (last_grant was m0)
  d.eval();
  CHECK(d.s_cmd_addr == 0xB000, "Both2: M1 wins (round-robin)");
  CHECK(d.m1_cmd_ready == 1, "Both2: m1_ready=1");
  tick(d);

  // Test 5: Response routing to m0
  d.m0_cmd_valid = 0; d.m1_cmd_valid = 0;
  d.s_rsp_valid = 1; d.s_rsp_rdata = 0xDEAD;
  // rsp_owner was set to m1 from last grant
  d.eval();
  CHECK(d.m1_rsp_valid == 1, "Rsp routed to M1");
  CHECK(d.m1_rsp_rdata == 0xDEAD, "M1 rsp_rdata=0xDEAD");
  CHECK(d.m0_rsp_valid == 0, "M0 rsp_valid=0");

  // Test 6: Slave not ready (backpressure)
  d.s_cmd_ready = 0;
  d.m0_cmd_valid = 1; d.m0_cmd_addr = 0xC000;
  d.s_rsp_valid = 0;
  d.eval();
  CHECK(d.m0_cmd_ready == 0, "Backpressure: m0_ready=0");

  printf("\n=== IcbArbt: %d/%d passed ===\n", test_count - fail_count, test_count);
  return fail_count ? 1 : 0;
}
