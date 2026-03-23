#include "VExuAgu.h"
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
  VExuAgu dut;

  // Defaults
  dut.i_valid = 0; dut.i_rs1 = 0; dut.i_rs2 = 0; dut.i_imm = 0;
  dut.i_load = 0; dut.i_store = 0; dut.i_rd_idx = 0; dut.i_rd_en = 0;
  dut.i_funct3 = 0; dut.icb_cmd_ready = 1; dut.icb_rsp_valid = 0;
  dut.icb_rsp_rdata = 0; dut.o_ready = 1;

  // Test 1: Word load address calc (base=0x1000 + offset=0x10 = 0x1010)
  dut.i_valid = 1; dut.i_load = 1; dut.i_store = 0;
  dut.i_rs1 = 0x1000; dut.i_imm = 0x10; dut.i_funct3 = 2; // LW
  dut.i_rd_idx = 5; dut.i_rd_en = 1;
  dut.eval();
  CHECK(dut.icb_cmd_valid == 1, "LW: cmd valid");
  CHECK(dut.icb_cmd_addr == 0x1010, "LW: addr = 0x1010");
  CHECK(dut.icb_cmd_read == 1, "LW: read=1");
  CHECK(dut.icb_cmd_wmask == 0, "LW: wmask=0 (read)");

  // Test 2: Word store (base=0x2000, offset=4, data=0xDEADBEEF)
  dut.i_load = 0; dut.i_store = 1; dut.i_funct3 = 2; // SW
  dut.i_rs1 = 0x2000; dut.i_imm = 4; dut.i_rs2 = 0xDEADBEEF;
  dut.eval();
  CHECK(dut.icb_cmd_addr == 0x2004, "SW: addr = 0x2004");
  CHECK(dut.icb_cmd_wdata == 0xDEADBEEF, "SW: wdata");
  CHECK(dut.icb_cmd_wmask == 0xF, "SW: wmask=0xF");
  CHECK(dut.icb_cmd_read == 0, "SW: read=0");

  // Test 3: Byte store at offset 1 (SB, byte_off=1)
  dut.i_funct3 = 0; // SB
  dut.i_rs1 = 0x3000; dut.i_imm = 1; dut.i_rs2 = 0xAB;
  dut.eval();
  CHECK(dut.icb_cmd_addr == 0x3000, "SB@1: addr aligned");
  CHECK(dut.icb_cmd_wmask == 0x2, "SB@1: wmask=0x2");
  CHECK((dut.icb_cmd_wdata & 0xFF00) == 0xAB00, "SB@1: data shifted to byte 1");

  // Test 4: Halfword store at offset 2
  dut.i_funct3 = 1; // SH
  dut.i_rs1 = 0x4000; dut.i_imm = 2; dut.i_rs2 = 0x1234;
  dut.eval();
  CHECK(dut.icb_cmd_wmask == 0xC, "SH@2: wmask=0xC");

  // Test 5: Load response — word
  dut.i_load = 1; dut.i_store = 0; dut.i_funct3 = 2;
  dut.i_rs1 = 0x5000; dut.i_imm = 0;
  dut.icb_rsp_valid = 1; dut.icb_rsp_rdata = 0x12345678;
  dut.eval();
  CHECK(dut.o_valid == 1, "LW rsp: o_valid");
  CHECK(dut.o_wdat == 0x12345678, "LW rsp: o_wdat");

  // Test 6: Load byte unsigned from byte 2
  dut.i_funct3 = 4; // LBU
  dut.i_rs1 = 0x6002; dut.i_imm = 0;
  dut.icb_rsp_rdata = 0xAB000000; // byte 2 has 0 in the shifted result
  // byte_off = 2, rdata_shifted = rdata >> 16 = 0xAB00, load_byte = 0x00
  // Actually: addr=0x6002, byte_off=2, rdata>>16 = 0xAB00, trunc<7,0>=0x00
  // Let me use a cleaner example
  dut.i_rs1 = 0x6000; dut.i_imm = 2;
  dut.icb_rsp_rdata = 0x00CD0000; // byte at offset 2 = 0xCD
  dut.eval();
  // byte_off=2, rdata_shifted = 0x00CD0000 >> 16 = 0x0000_00CD, load_byte=0xCD
  CHECK(dut.o_wdat == 0xCD, "LBU@2: unsigned byte = 0xCD");

  // Test 7: Load byte signed (negative)
  dut.i_funct3 = 0; // LB
  dut.i_rs1 = 0x7000; dut.i_imm = 0;
  dut.icb_rsp_rdata = 0x000000F0; // byte at offset 0 = 0xF0 (negative)
  dut.eval();
  CHECK(dut.o_wdat == 0xFFFFFFF0, "LB: sign-extended 0xF0 -> 0xFFFFFFF0");

  // Test 8: Load halfword signed
  dut.i_funct3 = 1; // LH
  dut.i_rs1 = 0x8000; dut.i_imm = 0;
  dut.icb_rsp_rdata = 0x0000FFFE;
  dut.eval();
  CHECK(dut.o_wdat == 0xFFFFFFFE, "LH: sign-extended 0xFFFE -> 0xFFFFFFFE");

  // Test 9: Handshake — not valid
  dut.i_valid = 0; dut.i_load = 0; dut.i_store = 0;
  dut.icb_rsp_valid = 0;
  dut.eval();
  CHECK(dut.icb_cmd_valid == 0, "No op: cmd_valid=0");
  CHECK(dut.o_valid == 0, "No op: o_valid=0");

  // Test 10: Store does not writeback
  dut.i_valid = 1; dut.i_store = 1; dut.i_load = 0; dut.i_funct3 = 2;
  dut.icb_rsp_valid = 1;
  dut.eval();
  CHECK(dut.o_valid == 0, "Store: no writeback (o_valid=0)");

  printf("\n=== AGU: %d/%d passed ===\n", test_count - fail_count, test_count);
  return fail_count ? 1 : 0;
}
