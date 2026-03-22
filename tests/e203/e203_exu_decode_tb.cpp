// ARCH sim testbench for ExuDecode — RV32I instruction decoder
// Tests all major instruction formats: R-type, I-type, S-type, B-type,
// U-type (LUI/AUIPC), J-type (JAL), and JALR.

#include "VExuDecode.h"
#include <cstdio>
#include <cstdint>
#include <cstdlib>

static int fail_count = 0;

#define CHECK(cond, fmt, ...) do { \
    if (!(cond)) { \
        printf("  FAIL: " fmt "\n", ##__VA_ARGS__); \
        fail_count++; \
    } \
} while(0)

// RV32I instruction encoding helpers
// R-type: funct7[31:25] | rs2[24:20] | rs1[19:15] | funct3[14:12] | rd[11:7] | opcode[6:0]
static uint32_t r_type(uint32_t funct7, uint32_t rs2, uint32_t rs1, uint32_t funct3, uint32_t rd, uint32_t opcode) {
    return (funct7 << 25) | (rs2 << 20) | (rs1 << 15) | (funct3 << 12) | (rd << 7) | opcode;
}
// I-type: imm[31:20] | rs1[19:15] | funct3[14:12] | rd[11:7] | opcode[6:0]
static uint32_t i_type(uint32_t imm12, uint32_t rs1, uint32_t funct3, uint32_t rd, uint32_t opcode) {
    return ((imm12 & 0xFFF) << 20) | (rs1 << 15) | (funct3 << 12) | (rd << 7) | opcode;
}
// S-type: imm[11:5][31:25] | rs2[24:20] | rs1[19:15] | funct3[14:12] | imm[4:0][11:7] | opcode[6:0]
static uint32_t s_type(uint32_t imm12, uint32_t rs2, uint32_t rs1, uint32_t funct3, uint32_t opcode) {
    uint32_t hi = (imm12 >> 5) & 0x7F;
    uint32_t lo = imm12 & 0x1F;
    return (hi << 25) | (rs2 << 20) | (rs1 << 15) | (funct3 << 12) | (lo << 7) | opcode;
}
// B-type: imm[12|10:5][31:25] | rs2[24:20] | rs1[19:15] | funct3[14:12] | imm[4:1|11][11:7] | opcode[6:0]
static uint32_t b_type(int32_t offset, uint32_t rs2, uint32_t rs1, uint32_t funct3, uint32_t opcode) {
    uint32_t imm = (uint32_t)offset;
    uint32_t b12  = (imm >> 12) & 1;
    uint32_t b11  = (imm >> 11) & 1;
    uint32_t b10_5 = (imm >> 5) & 0x3F;
    uint32_t b4_1  = (imm >> 1) & 0xF;
    return (b12 << 31) | (b10_5 << 25) | (rs2 << 20) | (rs1 << 15) |
           (funct3 << 12) | (b4_1 << 8) | (b11 << 7) | opcode;
}
// U-type: imm[31:12] | rd[11:7] | opcode[6:0]
static uint32_t u_type(uint32_t imm20, uint32_t rd, uint32_t opcode) {
    return (imm20 << 12) | (rd << 7) | opcode;
}
// J-type: imm[20|10:1|11|19:12][31:12] | rd[11:7] | opcode[6:0]
static uint32_t j_type(int32_t offset, uint32_t rd, uint32_t opcode) {
    uint32_t imm = (uint32_t)offset;
    uint32_t b20    = (imm >> 20) & 1;
    uint32_t b19_12 = (imm >> 12) & 0xFF;
    uint32_t b11    = (imm >> 11) & 1;
    uint32_t b10_1  = (imm >> 1) & 0x3FF;
    return (b20 << 31) | (b10_1 << 21) | (b11 << 20) | (b19_12 << 12) | (rd << 7) | opcode;
}

int main() {
    VExuDecode* dut = new VExuDecode();

    auto apply = [&](uint32_t instr) {
        dut->instr = instr;
        dut->eval();
    };

    printf("=== ExuDecode testbench ===\n");

    // ── Test 1: ADD x3, x1, x2 (R-type) ──────────────────────────────
    printf("Test 1: ADD x3, x1, x2\n");
    apply(r_type(0x00, 2, 1, 0x0, 3, 0x33));
    CHECK(dut->o_alu == 1,       "o_alu=%d", dut->o_alu);
    CHECK(dut->o_alu_add == 1,   "o_alu_add=%d", dut->o_alu_add);
    CHECK(dut->o_alu_sub == 0,   "o_alu_sub=%d", dut->o_alu_sub);
    CHECK(dut->o_rs1_idx == 1,   "rs1=%d", dut->o_rs1_idx);
    CHECK(dut->o_rs2_idx == 2,   "rs2=%d", dut->o_rs2_idx);
    CHECK(dut->o_rd_idx == 3,    "rd=%d", dut->o_rd_idx);
    CHECK(dut->o_rs1_en == 1,    "rs1_en=%d", dut->o_rs1_en);
    CHECK(dut->o_rs2_en == 1,    "rs2_en=%d", dut->o_rs2_en);
    CHECK(dut->o_rd_en == 1,     "rd_en=%d", dut->o_rd_en);

    // ── Test 2: SUB x5, x3, x4 (R-type) ──────────────────────────────
    printf("Test 2: SUB x5, x3, x4\n");
    apply(r_type(0x20, 4, 3, 0x0, 5, 0x33));
    CHECK(dut->o_alu == 1,       "o_alu=%d", dut->o_alu);
    CHECK(dut->o_alu_sub == 1,   "o_alu_sub=%d", dut->o_alu_sub);
    CHECK(dut->o_alu_add == 0,   "o_alu_add=%d", dut->o_alu_add);

    // ── Test 3: XOR x6, x1, x2 (R-type) ──────────────────────────────
    printf("Test 3: XOR x6, x1, x2\n");
    apply(r_type(0x00, 2, 1, 0x4, 6, 0x33));
    CHECK(dut->o_alu_xor == 1,   "o_alu_xor=%d", dut->o_alu_xor);

    // ── Test 4: SLL x7, x1, x2 (R-type) ──────────────────────────────
    printf("Test 4: SLL x7, x1, x2\n");
    apply(r_type(0x00, 2, 1, 0x1, 7, 0x33));
    CHECK(dut->o_alu_sll == 1,   "o_alu_sll=%d", dut->o_alu_sll);

    // ── Test 5: SRL x8, x1, x2 (R-type) ──────────────────────────────
    printf("Test 5: SRL x8, x1, x2\n");
    apply(r_type(0x00, 2, 1, 0x5, 8, 0x33));
    CHECK(dut->o_alu_srl == 1,   "o_alu_srl=%d", dut->o_alu_srl);
    CHECK(dut->o_alu_sra == 0,   "o_alu_sra=%d", dut->o_alu_sra);

    // ── Test 6: SRA x9, x1, x2 (R-type) ──────────────────────────────
    printf("Test 6: SRA x9, x1, x2\n");
    apply(r_type(0x20, 2, 1, 0x5, 9, 0x33));
    CHECK(dut->o_alu_sra == 1,   "o_alu_sra=%d", dut->o_alu_sra);
    CHECK(dut->o_alu_srl == 0,   "o_alu_srl=%d", dut->o_alu_srl);

    // ── Test 7: OR x10, x1, x2 (R-type) ─────────────────────────────
    printf("Test 7: OR x10, x1, x2\n");
    apply(r_type(0x00, 2, 1, 0x6, 10, 0x33));
    CHECK(dut->o_alu_or == 1,    "o_alu_or=%d", dut->o_alu_or);

    // ── Test 8: AND x11, x1, x2 (R-type) ────────────────────────────
    printf("Test 8: AND x11, x1, x2\n");
    apply(r_type(0x00, 2, 1, 0x7, 11, 0x33));
    CHECK(dut->o_alu_and == 1,   "o_alu_and=%d", dut->o_alu_and);

    // ── Test 9: SLT x12, x1, x2 (R-type) ────────────────────────────
    printf("Test 9: SLT x12, x1, x2\n");
    apply(r_type(0x00, 2, 1, 0x2, 12, 0x33));
    CHECK(dut->o_alu_slt == 1,   "o_alu_slt=%d", dut->o_alu_slt);

    // ── Test 10: SLTU x13, x1, x2 (R-type) ──────────────────────────
    printf("Test 10: SLTU x13, x1, x2\n");
    apply(r_type(0x00, 2, 1, 0x3, 13, 0x33));
    CHECK(dut->o_alu_sltu == 1,  "o_alu_sltu=%d", dut->o_alu_sltu);

    // ── Test 11: ADDI x1, x2, 42 (I-type) ───────────────────────────
    printf("Test 11: ADDI x1, x2, 42\n");
    apply(i_type(42, 2, 0x0, 1, 0x13));
    CHECK(dut->o_alu == 1,       "o_alu=%d", dut->o_alu);
    CHECK(dut->o_alu_add == 1,   "o_alu_add=%d", dut->o_alu_add);
    CHECK(dut->o_imm == 42,      "imm=0x%08X", dut->o_imm);
    CHECK(dut->o_rs1_en == 1,    "rs1_en=%d", dut->o_rs1_en);
    CHECK(dut->o_rs2_en == 0,    "rs2_en=%d", dut->o_rs2_en);

    // ── Test 12: ADDI with negative imm (-5) ─────────────────────────
    printf("Test 12: ADDI x1, x2, -5\n");
    apply(i_type((-5) & 0xFFF, 2, 0x0, 1, 0x13));
    CHECK(dut->o_imm == 0xFFFFFFFB, "imm=0x%08X exp 0xFFFFFFFB", dut->o_imm);

    // ── Test 13: XORI x3, x1, 0x123 (I-type) ────────────────────────
    printf("Test 13: XORI x3, x1, 0x123\n");
    apply(i_type(0x123, 1, 0x4, 3, 0x13));
    CHECK(dut->o_alu_xor == 1,   "o_alu_xor=%d", dut->o_alu_xor);
    CHECK(dut->o_imm == 0x123,   "imm=0x%08X", dut->o_imm);

    // ── Test 14: LUI x5, 0xDEADB (U-type) ───────────────────────────
    printf("Test 14: LUI x5, 0xDEADB\n");
    apply(u_type(0xDEADB, 5, 0x37));
    CHECK(dut->o_alu == 1,       "o_alu=%d", dut->o_alu);
    CHECK(dut->o_alu_lui == 1,   "o_alu_lui=%d", dut->o_alu_lui);
    CHECK(dut->o_imm == 0xDEADB000, "imm=0x%08X exp 0xDEADB000", dut->o_imm);
    CHECK(dut->o_rd_en == 1,     "rd_en=%d", dut->o_rd_en);
    CHECK(dut->o_rs1_en == 0,    "rs1_en=%d", dut->o_rs1_en);

    // ── Test 15: AUIPC x6, 0x12345 (U-type) ─────────────────────────
    printf("Test 15: AUIPC x6, 0x12345\n");
    apply(u_type(0x12345, 6, 0x17));
    CHECK(dut->o_alu == 1,       "o_alu=%d", dut->o_alu);
    CHECK(dut->o_alu_add == 1,   "o_alu_add=%d (AUIPC uses add)", dut->o_alu_add);
    CHECK(dut->o_imm == 0x12345000, "imm=0x%08X exp 0x12345000", dut->o_imm);

    // ── Test 16: BEQ x1, x2, +8 (B-type) ────────────────────────────
    printf("Test 16: BEQ x1, x2, +8\n");
    apply(b_type(8, 2, 1, 0x0, 0x63));
    CHECK(dut->o_bjp == 1,       "o_bjp=%d", dut->o_bjp);
    CHECK(dut->o_beq == 1,       "o_beq=%d", dut->o_beq);
    CHECK(dut->o_imm == 8,       "imm=0x%08X exp 8", dut->o_imm);
    CHECK(dut->o_rs1_en == 1,    "rs1_en=%d", dut->o_rs1_en);
    CHECK(dut->o_rs2_en == 1,    "rs2_en=%d", dut->o_rs2_en);
    CHECK(dut->o_rd_en == 0,     "rd_en=%d", dut->o_rd_en);

    // ── Test 17: BNE x3, x4, -16 (B-type, negative) ─────────────────
    printf("Test 17: BNE x3, x4, -16\n");
    apply(b_type(-16, 4, 3, 0x1, 0x63));
    CHECK(dut->o_bne == 1,       "o_bne=%d", dut->o_bne);
    CHECK(dut->o_imm == 0xFFFFFFF0, "imm=0x%08X exp 0xFFFFFFF0", dut->o_imm);

    // ── Test 18: BLT x1, x2, +64 ────────────────────────────────────
    printf("Test 18: BLT x1, x2, +64\n");
    apply(b_type(64, 2, 1, 0x4, 0x63));
    CHECK(dut->o_blt == 1,       "o_blt=%d", dut->o_blt);
    CHECK(dut->o_imm == 64,      "imm=0x%08X exp 64", dut->o_imm);

    // ── Test 19: BGE x1, x2, +32 ────────────────────────────────────
    printf("Test 19: BGE x1, x2, +32\n");
    apply(b_type(32, 2, 1, 0x5, 0x63));
    CHECK(dut->o_bge == 1,       "o_bge=%d", dut->o_bge);

    // ── Test 20: BLTU x1, x2, +128 ──────────────────────────────────
    printf("Test 20: BLTU x1, x2, +128\n");
    apply(b_type(128, 2, 1, 0x6, 0x63));
    CHECK(dut->o_bltu == 1,      "o_bltu=%d", dut->o_bltu);

    // ── Test 21: BGEU x1, x2, +256 ──────────────────────────────────
    printf("Test 21: BGEU x1, x2, +256\n");
    apply(b_type(256, 2, 1, 0x7, 0x63));
    CHECK(dut->o_bgeu == 1,      "o_bgeu=%d", dut->o_bgeu);

    // ── Test 22: JAL x1, +1024 (J-type) ─────────────────────────────
    printf("Test 22: JAL x1, +1024\n");
    apply(j_type(1024, 1, 0x6F));
    CHECK(dut->o_bjp == 1,       "o_bjp=%d", dut->o_bjp);
    CHECK(dut->o_jump == 1,      "o_jump=%d", dut->o_jump);
    CHECK(dut->o_imm == 1024,    "imm=0x%08X exp 1024", dut->o_imm);
    CHECK(dut->o_rd_en == 1,     "rd_en=%d", dut->o_rd_en);
    CHECK(dut->o_rs1_en == 0,    "rs1_en=%d", dut->o_rs1_en);

    // ── Test 23: JAL with negative offset (-256) ─────────────────────
    printf("Test 23: JAL x1, -256\n");
    apply(j_type(-256, 1, 0x6F));
    CHECK(dut->o_jump == 1,      "o_jump=%d", dut->o_jump);
    CHECK(dut->o_imm == 0xFFFFFF00, "imm=0x%08X exp 0xFFFFFF00", dut->o_imm);

    // ── Test 24: JALR x1, x5, 100 (I-type, opcode 0x67) ─────────────
    printf("Test 24: JALR x1, x5, 100\n");
    apply(i_type(100, 5, 0x0, 1, 0x67));
    CHECK(dut->o_bjp == 1,       "o_bjp=%d", dut->o_bjp);
    CHECK(dut->o_jump == 1,      "o_jump=%d", dut->o_jump);
    CHECK(dut->o_imm == 100,     "imm=0x%08X exp 100", dut->o_imm);
    CHECK(dut->o_rs1_en == 1,    "rs1_en=%d", dut->o_rs1_en);
    CHECK(dut->o_rd_en == 1,     "rd_en=%d", dut->o_rd_en);

    // ── Test 25: LW x3, 0(x1) (Load, I-type) ────────────────────────
    printf("Test 25: LW x3, 0(x1)\n");
    apply(i_type(0, 1, 0x2, 3, 0x03));
    CHECK(dut->o_agu == 1,       "o_agu=%d", dut->o_agu);
    CHECK(dut->o_load == 1,      "o_load=%d", dut->o_load);
    CHECK(dut->o_store == 0,     "o_store=%d", dut->o_store);
    CHECK(dut->o_imm == 0,       "imm=0x%08X", dut->o_imm);
    CHECK(dut->o_rs1_en == 1,    "rs1_en=%d", dut->o_rs1_en);
    CHECK(dut->o_rs2_en == 0,    "rs2_en=%d", dut->o_rs2_en);
    CHECK(dut->o_rd_en == 1,     "rd_en=%d", dut->o_rd_en);

    // ── Test 26: LW x3, -4(x1) (Load with negative offset) ──────────
    printf("Test 26: LW x3, -4(x1)\n");
    apply(i_type((-4) & 0xFFF, 1, 0x2, 3, 0x03));
    CHECK(dut->o_load == 1,      "o_load=%d", dut->o_load);
    CHECK(dut->o_imm == 0xFFFFFFFC, "imm=0x%08X exp 0xFFFFFFFC", dut->o_imm);

    // ── Test 27: SW x2, 16(x1) (Store, S-type) ──────────────────────
    printf("Test 27: SW x2, 16(x1)\n");
    apply(s_type(16, 2, 1, 0x2, 0x23));
    CHECK(dut->o_agu == 1,       "o_agu=%d", dut->o_agu);
    CHECK(dut->o_store == 1,     "o_store=%d", dut->o_store);
    CHECK(dut->o_load == 0,      "o_load=%d", dut->o_load);
    CHECK(dut->o_imm == 16,      "imm=0x%08X exp 16", dut->o_imm);
    CHECK(dut->o_rs1_en == 1,    "rs1_en=%d", dut->o_rs1_en);
    CHECK(dut->o_rs2_en == 1,    "rs2_en=%d", dut->o_rs2_en);
    CHECK(dut->o_rd_en == 0,     "rd_en=%d", dut->o_rd_en);

    // ── Test 28: SW with negative offset (-8) ────────────────────────
    printf("Test 28: SW x2, -8(x1)\n");
    apply(s_type((-8) & 0xFFF, 2, 1, 0x2, 0x23));
    CHECK(dut->o_store == 1,     "o_store=%d", dut->o_store);
    CHECK(dut->o_imm == 0xFFFFFFF8, "imm=0x%08X exp 0xFFFFFFF8", dut->o_imm);

    // ── Test 29: SRAI x3, x1, 5 (I-type, funct7=0x20) ──────────────
    printf("Test 29: SRAI x3, x1, 5\n");
    apply(i_type((0x20 << 5) | 5, 1, 0x5, 3, 0x13));
    CHECK(dut->o_alu_sra == 1,   "o_alu_sra=%d", dut->o_alu_sra);
    CHECK(dut->o_alu_srl == 0,   "o_alu_srl=%d", dut->o_alu_srl);

    // ── Test 30: SRLI x3, x1, 5 (I-type, funct7=0x00) ──────────────
    printf("Test 30: SRLI x3, x1, 5\n");
    apply(i_type(5, 1, 0x5, 3, 0x13));
    CHECK(dut->o_alu_srl == 1,   "o_alu_srl=%d", dut->o_alu_srl);
    CHECK(dut->o_alu_sra == 0,   "o_alu_sra=%d", dut->o_alu_sra);

    // ── Summary ──────────────────────────────────────────────────────
    if (fail_count == 0) {
        printf("\nAll 30 tests PASSED\n");
    } else {
        printf("\n%d test(s) FAILED\n", fail_count);
    }

    delete dut;
    return fail_count ? 1 : 0;
}
