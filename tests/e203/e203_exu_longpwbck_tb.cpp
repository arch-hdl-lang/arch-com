#include "VExuLongpWbck.h"
#include <cstdio>
#include <cstdlib>

static int fail_count = 0;
static int test_count = 0;

#define CHECK(cond, msg) do { \
  test_count++; \
  if (!(cond)) { printf("FAIL: %s\n", msg); fail_count++; } \
  else { printf("PASS: %s\n", msg); } \
} while(0)

int main() {
  VExuLongpWbck dut;
  dut.lsu_wbck_valid = 0; dut.lsu_wbck_wdat = 0; dut.lsu_wbck_rd_idx = 0; dut.lsu_wbck_rd_en = 0;
  dut.mdv_wbck_valid = 0; dut.mdv_wbck_wdat = 0; dut.mdv_wbck_rd_idx = 0; dut.mdv_wbck_rd_en = 0;
  dut.o_ready = 1;

  // Test 1: Neither valid
  dut.eval();
  CHECK(dut.o_valid == 0, "Both idle: o_valid=0");

  // Test 2: MulDiv only
  dut.mdv_wbck_valid = 1; dut.mdv_wbck_wdat = 0x42; dut.mdv_wbck_rd_idx = 7; dut.mdv_wbck_rd_en = 1;
  dut.eval();
  CHECK(dut.o_valid == 1, "MulDiv only: o_valid=1");
  CHECK(dut.o_wdat == 0x42, "MulDiv only: wdat=0x42");
  CHECK(dut.o_rd_idx == 7, "MulDiv only: rd_idx=7");
  CHECK(dut.mdv_wbck_ready == 1, "MulDiv only: mdv_ready=1");
  CHECK(dut.lsu_wbck_ready == 0, "MulDiv only: lsu_ready=0");

  // Test 3: LSU only
  dut.mdv_wbck_valid = 0;
  dut.lsu_wbck_valid = 1; dut.lsu_wbck_wdat = 0xBEEF; dut.lsu_wbck_rd_idx = 3; dut.lsu_wbck_rd_en = 1;
  dut.eval();
  CHECK(dut.o_valid == 1, "LSU only: o_valid=1");
  CHECK(dut.o_wdat == 0xBEEF, "LSU only: wdat=0xBEEF");
  CHECK(dut.o_rd_idx == 3, "LSU only: rd_idx=3");
  CHECK(dut.lsu_wbck_ready == 1, "LSU only: lsu_ready=1");

  // Test 4: Both valid — LSU wins
  dut.mdv_wbck_valid = 1; dut.mdv_wbck_wdat = 0xAAAA; dut.mdv_wbck_rd_idx = 10;
  dut.lsu_wbck_valid = 1; dut.lsu_wbck_wdat = 0xBBBB; dut.lsu_wbck_rd_idx = 5;
  dut.eval();
  CHECK(dut.o_wdat == 0xBBBB, "Both: LSU wins (wdat=0xBBBB)");
  CHECK(dut.o_rd_idx == 5, "Both: LSU rd_idx=5");
  CHECK(dut.lsu_wbck_ready == 1, "Both: lsu_ready=1");
  CHECK(dut.mdv_wbck_ready == 0, "Both: mdv_ready=0 (blocked)");

  // Test 5: Backpressure — o_ready=0
  dut.o_ready = 0;
  dut.eval();
  CHECK(dut.lsu_wbck_ready == 0, "Backpressure: lsu_ready=0");
  CHECK(dut.mdv_wbck_ready == 0, "Backpressure: mdv_ready=0");

  printf("\n=== LongpWbck: %d/%d passed ===\n", test_count - fail_count, test_count);
  return fail_count ? 1 : 0;
}
