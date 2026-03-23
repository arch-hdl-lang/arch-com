#include "VIfuLiteDec.h"
#include <cstdio>
#include <cstdlib>

static int fail_count = 0;
static int test_count = 0;

#define CHECK(cond, msg) do { \
  test_count++; \
  if (!(cond)) { printf("FAIL: %s\n", msg); fail_count++; } \
  else { printf("PASS: %s\n", msg); } \
} while(0)

// RV32I instruction encoders
static uint32_t enc_jal(int rd, int32_t imm) {
  uint32_t i = ((imm >> 20) & 1) << 31 | ((imm >> 1) & 0x3FF) << 21 |
               ((imm >> 11) & 1) << 20 | ((imm >> 12) & 0xFF) << 12 |
               (rd & 0x1F) << 7 | 0x6F;
  return i;
}

static uint32_t enc_jalr(int rd, int rs1, int32_t imm) {
  return ((imm & 0xFFF) << 20) | ((rs1 & 0x1F) << 15) | (0 << 12) |
         ((rd & 0x1F) << 7) | 0x67;
}

static uint32_t enc_branch(int rs1, int rs2, int32_t imm, int funct3) {
  uint32_t i = ((imm >> 12) & 1) << 31 | ((imm >> 5) & 0x3F) << 25 |
               (rs2 & 0x1F) << 20 | (rs1 & 0x1F) << 15 | (funct3 << 12) |
               ((imm >> 1) & 0xF) << 8 | ((imm >> 11) & 1) << 7 | 0x63;
  return i;
}

int main() {
  VIfuLiteDec dut;

  // Test 1: 32-bit NOP (ADDI x0, x0, 0) — opcode 0x13
  dut.instr = 0x00000013;
  dut.eval();
  CHECK(dut.is_32bit == 1, "ADDI: 32-bit");
  CHECK(dut.is_bjp == 0, "ADDI: not BJP");
  CHECK(dut.is_lui == 0, "ADDI: not LUI");

  // Test 2: LUI x5, 0x12345
  dut.instr = 0x12345037 | (5 << 7); // LUI rd=5
  dut.instr = (0x12345 << 12) | (5 << 7) | 0x37;
  dut.eval();
  CHECK(dut.is_32bit == 1, "LUI: 32-bit");
  CHECK(dut.is_lui == 1, "LUI: detected");
  CHECK(dut.rd_idx == 5, "LUI: rd=5");

  // Test 3: AUIPC
  dut.instr = (0x12345 << 12) | (3 << 7) | 0x17;
  dut.eval();
  CHECK(dut.is_auipc == 1, "AUIPC: detected");

  // Test 4: JAL x1, +8
  dut.instr = enc_jal(1, 8);
  dut.eval();
  CHECK(dut.is_jal == 1, "JAL: detected");
  CHECK(dut.is_bjp == 1, "JAL: is_bjp=1");
  CHECK(dut.rd_idx == 1, "JAL: rd=1");
  CHECK(dut.bjp_imm == 8, "JAL: imm=8");

  // Test 5: JAL with negative offset (-16)
  // JAL x0, -16: imm[20]=1, imm[10:1]=1111111000, imm[11]=1, imm[19:12]=11111111
  dut.instr = 0xFF1FF06F;
  dut.eval();
  CHECK(dut.is_jal == 1, "JAL-neg: detected");
  CHECK(dut.bjp_imm == 0xFFFFFFF0, "JAL-neg: imm=-16 (0xFFFFFFF0)");

  // Test 6: JALR x1, x5, 0
  dut.instr = enc_jalr(1, 5, 0);
  dut.eval();
  CHECK(dut.is_jalr == 1, "JALR: detected");
  CHECK(dut.rs1_idx == 5, "JALR: rs1=5");
  CHECK(dut.rs1_en == 1, "JALR: rs1_en=1");

  // Test 7: BEQ x1, x2, +4
  dut.instr = enc_branch(1, 2, 4, 0);
  dut.eval();
  CHECK(dut.is_branch == 1, "BEQ: detected");
  CHECK(dut.is_bjp == 1, "BEQ: is_bjp=1");
  CHECK(dut.rs1_en == 1, "BEQ: rs1_en=1");
  CHECK(dut.bjp_imm == 4, "BEQ: imm=4");

  // Test 8: BEQ with negative offset (-8)
  // BEQ x3, x4, -8: imm[12]=1, imm[10:5]=111111, imm[4:1]=1100, imm[11]=1
  dut.instr = 0xFE418CE3;
  dut.eval();
  CHECK(dut.bjp_imm == 0xFFFFFFF8, "BEQ-neg: imm=-8");

  // Test 9: 16-bit instruction (bits[1:0] != 11)
  dut.instr = 0x0000C002; // compressed instruction (bits[1:0] = 10)
  dut.eval();
  CHECK(dut.is_32bit == 0, "Compressed: not 32-bit");
  CHECK(dut.is_bjp == 0, "Compressed: is_bjp=0 (not decoded as 32-bit)");

  // Test 10: Another 16-bit (bits[1:0] = 01)
  dut.instr = 0x00004001;
  dut.eval();
  CHECK(dut.is_32bit == 0, "Compressed 01: not 32-bit");

  printf("\n=== LiteDec: %d/%d passed ===\n", test_count - fail_count, test_count);
  return fail_count ? 1 : 0;
}
