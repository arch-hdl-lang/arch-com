#include "VIfuMinidec.h"
#include <cstdio>
#include <cstdlib>
#include <cstdint>

static VIfuMinidec* dut;
static int fail_count = 0;

#define CHECK(cond, msg) do { \
    if (!(cond)) { printf("FAIL: %s\n", msg); fail_count++; } \
} while(0)

// RV32I instruction builders
static uint32_t rv_jal(int rd, int32_t imm) {
    // J-type: imm[20|10:1|11|19:12] rd opcode
    uint32_t w = 0x6F | ((rd & 0x1F) << 7);
    w |= ((imm >> 12) & 0xFF) << 12;   // imm[19:12]
    w |= ((imm >> 11) & 0x1)  << 20;   // imm[11]
    w |= ((imm >> 1)  & 0x3FF) << 21;  // imm[10:1]
    w |= ((imm >> 20) & 0x1)  << 31;   // imm[20]
    return w;
}

static uint32_t rv_jalr(int rd, int rs1, int32_t imm) {
    // I-type: imm[11:0] rs1 000 rd 1100111
    uint32_t w = 0x67 | ((rd & 0x1F) << 7) | ((rs1 & 0x1F) << 15);
    w |= (imm & 0xFFF) << 20;
    return w;
}

static uint32_t rv_branch(int funct3, int rs1, int rs2, int32_t imm) {
    // B-type: imm[12|10:5] rs2 rs1 funct3 imm[4:1|11] opcode
    uint32_t w = 0x63;
    w |= ((imm >> 11) & 0x1) << 7;     // imm[11]
    w |= ((imm >> 1)  & 0xF) << 8;     // imm[4:1]
    w |= (funct3 & 0x7)      << 12;
    w |= ((rs1 & 0x1F))      << 15;
    w |= ((rs2 & 0x1F))      << 20;
    w |= ((imm >> 5) & 0x3F) << 25;    // imm[10:5]
    w |= ((imm >> 12) & 0x1) << 31;    // imm[12]
    return w;
}

static uint32_t rv_lui(int rd, uint32_t imm20) {
    return 0x37 | ((rd & 0x1F) << 7) | (imm20 << 12);
}

static uint32_t rv_auipc(int rd, uint32_t imm20) {
    return 0x17 | ((rd & 0x1F) << 7) | (imm20 << 12);
}

static uint32_t rv_addi(int rd, int rs1, int32_t imm) {
    return 0x13 | ((rd & 0x1F) << 7) | ((rs1 & 0x1F) << 15) | ((imm & 0xFFF) << 20);
}

static void eval(uint32_t instr) {
    dut->instr = instr;
    dut->eval();
}

// Sign-extend helper: from N bits to int32_t
static int32_t sext(uint32_t val, int bits) {
    uint32_t sign = 1u << (bits - 1);
    return (int32_t)((val ^ sign) - sign);
}

int main() {
    dut = new VIfuMinidec;

    // Test 1: JAL rd=1, imm=+0x100 (256)
    {
        eval(rv_jal(1, 0x100));
        CHECK(dut->o_is_jal,  "JAL: o_is_jal");
        CHECK(dut->o_is_bjp,  "JAL: o_is_bjp");
        CHECK(!dut->o_is_jalr,"JAL: !o_is_jalr");
        CHECK(!dut->o_is_bxx, "JAL: !o_is_bxx");
        int32_t imm = sext(dut->o_bjp_imm & 0x1FFFFF, 21);
        CHECK(imm == 0x100, "JAL: imm == 0x100");
        printf("JAL +256: imm=%d, is_jal=%d OK\n", imm, dut->o_is_jal);
    }

    // Test 2: JAL negative offset (-128)
    {
        eval(rv_jal(1, -128));
        CHECK(dut->o_is_jal, "JAL neg: o_is_jal");
        int32_t imm = sext(dut->o_bjp_imm & 0x1FFFFF, 21);
        CHECK(imm == -128, "JAL neg: imm == -128");
        printf("JAL -128: imm=%d OK\n", imm);
    }

    // Test 3: JALR rs1=5, imm=+64
    {
        eval(rv_jalr(1, 5, 64));
        CHECK(dut->o_is_jalr, "JALR: o_is_jalr");
        CHECK(dut->o_is_bjp,  "JALR: o_is_bjp");
        CHECK(!dut->o_is_jal, "JALR: !o_is_jal");
        CHECK(dut->o_rs1_idx == 5, "JALR: rs1_idx == 5");
        int32_t imm = sext(dut->o_bjp_imm & 0x1FFFFF, 21);
        CHECK(imm == 64, "JALR: imm == 64");
        printf("JALR rs1=5 +64: imm=%d, rs1=%d OK\n", imm, dut->o_rs1_idx);
    }

    // Test 4: JALR negative imm (-4)
    {
        eval(rv_jalr(1, 3, -4));
        CHECK(dut->o_is_jalr, "JALR neg: o_is_jalr");
        int32_t imm = sext(dut->o_bjp_imm & 0x1FFFFF, 21);
        CHECK(imm == -4, "JALR neg: imm == -4");
        printf("JALR -4: imm=%d OK\n", imm);
    }

    // Test 5: BEQ (funct3=0) rs1=2, rs2=3, imm=+8
    {
        eval(rv_branch(0, 2, 3, 8));
        CHECK(dut->o_is_bxx,   "BEQ: o_is_bxx");
        CHECK(dut->o_is_bjp,   "BEQ: o_is_bjp");
        CHECK(!dut->o_is_jal,  "BEQ: !o_is_jal");
        CHECK(!dut->o_is_jalr, "BEQ: !o_is_jalr");
        int32_t imm = sext(dut->o_bjp_imm & 0x1FFFFF, 21);
        CHECK(imm == 8, "BEQ: imm == 8");
        printf("BEQ +8: imm=%d OK\n", imm);
    }

    // Test 6: BNE (funct3=1) negative offset (-16)
    {
        eval(rv_branch(1, 4, 5, -16));
        CHECK(dut->o_is_bxx, "BNE neg: o_is_bxx");
        int32_t imm = sext(dut->o_bjp_imm & 0x1FFFFF, 21);
        CHECK(imm == -16, "BNE neg: imm == -16");
        printf("BNE -16: imm=%d OK\n", imm);
    }

    // Test 7: LUI
    {
        eval(rv_lui(5, 0xDEAD));
        CHECK(dut->o_is_lui,    "LUI: o_is_lui");
        CHECK(!dut->o_is_bjp,   "LUI: !o_is_bjp");
        CHECK(!dut->o_is_auipc, "LUI: !o_is_auipc");
        printf("LUI: is_lui=%d OK\n", dut->o_is_lui);
    }

    // Test 8: AUIPC
    {
        eval(rv_auipc(6, 0x12345));
        CHECK(dut->o_is_auipc, "AUIPC: o_is_auipc");
        CHECK(!dut->o_is_bjp,  "AUIPC: !o_is_bjp");
        CHECK(!dut->o_is_lui,  "AUIPC: !o_is_lui");
        printf("AUIPC: is_auipc=%d OK\n", dut->o_is_auipc);
    }

    // Test 9: ADDI (non-branch) — all flags false
    {
        eval(rv_addi(1, 2, 42));
        CHECK(!dut->o_is_bjp,   "ADDI: !o_is_bjp");
        CHECK(!dut->o_is_jal,   "ADDI: !o_is_jal");
        CHECK(!dut->o_is_jalr,  "ADDI: !o_is_jalr");
        CHECK(!dut->o_is_bxx,   "ADDI: !o_is_bxx");
        CHECK(!dut->o_is_lui,   "ADDI: !o_is_lui");
        CHECK(!dut->o_is_auipc, "ADDI: !o_is_auipc");
        printf("ADDI: all flags false OK\n");
    }

    if (fail_count == 0) {
        printf("\n=== ALL %d TESTS PASSED ===\n", 9);
    } else {
        printf("\n=== %d FAILURES ===\n", fail_count);
    }

    delete dut;
    return fail_count ? 1 : 0;
}
