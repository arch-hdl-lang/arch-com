// ARCH sim testbench for e203_ifu_litedec — E203 lightweight pre-decoder.
// Tests: 16/32-bit length detection, branch/JAL/JALR/LUI/AUIPC classification
// over real RV32IC encodings, rs1/rd field extraction, rs1_en gating, and
// sign-extended JAL/branch immediate reconstruction.
//
// NOTE: this replaces a stale tb (VIfuLiteDec.h) that targeted an earlier
// revision of this fixture; the fixture was renamed to `e203_ifu_litedec`
// when the e203 corpus was rewritten and the old tb has not compiled since.
//
// Fixture scope note (not a bug): this simplified litedec classifies 32-bit
// branches/jumps only. RVC jumps (c.j/c.jal/c.beqz/...) report is_bjp=0 —
// the full RVC expansion lives in e203_ifu_minidec. The tb asserts that.
//
// Run with:
//   arch sim tests/e203/e203_ifu_litedec.arch --tb tests/e203/e203_ifu_litedec_tb.cpp

#include "Ve203_ifu_litedec.h"
#include <cstdio>
#include <cstdint>

static int fail_count = 0;

#define CHECK(cond, fmt, ...) do { \
    if (!(cond)) { \
        printf("  FAIL: " fmt "\n", ##__VA_ARGS__); \
        fail_count++; \
    } \
} while(0)

static Ve203_ifu_litedec* dut;

// Decode `instr` and check the classification one-hot + length.
static void classify(uint32_t instr, int is32, int bjp, int jal, int jalr,
                     int branch, int lui, int auipc, const char* what) {
    dut->instr = instr;
    dut->eval();
    CHECK(dut->is_32bit == is32, "%s: is_32bit=%d, expected %d", what, dut->is_32bit, is32);
    CHECK(dut->is_bjp == bjp, "%s: is_bjp=%d, expected %d", what, dut->is_bjp, bjp);
    CHECK(dut->is_jal == jal, "%s: is_jal=%d, expected %d", what, dut->is_jal, jal);
    CHECK(dut->is_jalr == jalr, "%s: is_jalr=%d, expected %d", what, dut->is_jalr, jalr);
    CHECK(dut->is_branch == branch, "%s: is_branch=%d, expected %d", what, dut->is_branch, branch);
    CHECK(dut->is_lui == lui, "%s: is_lui=%d, expected %d", what, dut->is_lui, lui);
    CHECK(dut->is_auipc == auipc, "%s: is_auipc=%d, expected %d", what, dut->is_auipc, auipc);
}

int main() {
    dut = new Ve203_ifu_litedec;

    // ── Test 1: Classification over real RV32I encodings ─────────────
    printf("Test 1: Classification\n");
    //                                        is32 bjp jal jalr br lui auipc
    classify(0x00000013, 1, 0, 0, 0, 0, 0, 0, "addi x0,x0,0 (nop)");
    classify(0x002081B3, 1, 0, 0, 0, 0, 0, 0, "add x3,x1,x2");
    classify(0x100000EF, 1, 1, 1, 0, 0, 0, 0, "jal x1,+0x100");
    classify(0x00008067, 1, 1, 0, 1, 0, 0, 0, "jalr x0,0(x1) (ret)");
    classify(0xFE208CE3, 1, 1, 0, 0, 1, 0, 0, "beq x1,x2,-8");
    classify(0x00419863, 1, 1, 0, 0, 1, 0, 0, "bne x3,x4,+16");
    classify(0x123452B7, 1, 0, 0, 0, 0, 1, 0, "lui x5,0x12345");
    classify(0x00001317, 1, 0, 0, 0, 0, 0, 1, "auipc x6,0x1");
    classify(0x0000A103, 1, 0, 0, 0, 0, 0, 0, "lw x2,0(x1)");

    // ── Test 2: RVC (16-bit) detection ───────────────────────────────
    printf("Test 2: RVC length detection\n");
    // bits[1:0] != 2'b11 -> 16-bit; classification outputs must all gate off.
    classify(0x00004501, 0, 0, 0, 0, 0, 0, 0, "c.li x10,0");
    classify(0x00000405, 0, 0, 0, 0, 0, 0, 0, "c.addi x8,1");
    // c.j / c.beqz are jumps in RVC space, but this litedec only classifies
    // 32-bit encodings — is_bjp must still be 0 (minidec handles RVC).
    classify(0x0000A001, 0, 0, 0, 0, 0, 0, 0, "c.j 0");
    classify(0x0000C001, 0, 0, 0, 0, 0, 0, 0, "c.beqz x8,0");

    // ── Test 3: Register field extraction + rs1_en ───────────────────
    printf("Test 3: Register fields\n");
    // jalr x1, 8(x5): opcode 0x67, rd=1, rs1=5 -> rs1_en (jalr uses rs1)
    dut->instr = (8u << 20) | (5u << 15) | (0u << 12) | (1u << 7) | 0x67;
    dut->eval();
    CHECK(dut->rs1_idx == 5, "jalr rs1_idx should be 5, got %d", dut->rs1_idx);
    CHECK(dut->rd_idx == 1, "jalr rd_idx should be 1, got %d", dut->rd_idx);
    CHECK(dut->rs1_en == 1, "jalr should assert rs1_en, got %d", dut->rs1_en);

    // beq x7, x9, +4: rs1=7 -> rs1_en (branch uses rs1)
    dut->instr = (9u << 20) | (7u << 15) | (0u << 12) | (2u << 8) | 0x63;
    dut->eval();
    CHECK(dut->rs1_idx == 7, "beq rs1_idx should be 7, got %d", dut->rs1_idx);
    CHECK(dut->rs1_en == 1, "beq should assert rs1_en, got %d", dut->rs1_en);

    // jal x3, +8: no rs1 use
    dut->instr = ((8u >> 1) & 0x3FFu) << 21 | (3u << 7) | 0x6F;
    dut->eval();
    CHECK(dut->rd_idx == 3, "jal rd_idx should be 3, got %d", dut->rd_idx);
    CHECK(dut->rs1_en == 0, "jal should not assert rs1_en, got %d", dut->rs1_en);

    // add x3, x1, x2: neither jalr nor branch
    dut->instr = 0x002081B3;
    dut->eval();
    CHECK(dut->rs1_en == 0, "add should not assert rs1_en, got %d", dut->rs1_en);
    // rs1_idx/rd_idx are raw field extracts regardless of opcode.
    CHECK(dut->rs1_idx == 1, "add rs1 field should be 1, got %d", dut->rs1_idx);
    CHECK(dut->rd_idx == 3, "add rd field should be 3, got %d", dut->rd_idx);

    // ── Test 4: JAL immediate reconstruction ─────────────────────────
    printf("Test 4: JAL immediate\n");
    dut->instr = 0x100000EF;            // jal x1, +0x100
    dut->eval();
    CHECK(dut->bjp_imm == 0x100, "jal +0x100 imm should be 0x100, got 0x%08x", dut->bjp_imm);
    dut->instr = 0xFFDFF06F;            // jal x0, -4
    dut->eval();
    CHECK(dut->bjp_imm == 0xFFFFFFFCu, "jal -4 imm should sign-extend to 0xFFFFFFFC, got 0x%08x",
          dut->bjp_imm);
    // Large forward: jal x0, +0xFF000 (imm[19:12] path)
    dut->instr = (0xFFu << 12) | 0x6F;
    dut->eval();
    CHECK(dut->bjp_imm == 0xFF000, "jal +0xFF000 imm should be 0xFF000, got 0x%08x", dut->bjp_imm);

    // ── Test 5: Branch immediate reconstruction ──────────────────────
    printf("Test 5: Branch immediate\n");
    dut->instr = 0xFE208CE3;            // beq x1, x2, -8
    dut->eval();
    CHECK(dut->bjp_imm == 0xFFFFFFF8u, "beq -8 imm should sign-extend to 0xFFFFFFF8, got 0x%08x",
          dut->bjp_imm);
    dut->instr = 0x00419863;            // bne x3, x4, +16
    dut->eval();
    CHECK(dut->bjp_imm == 0x10, "bne +16 imm should be 0x10, got 0x%08x", dut->bjp_imm);
    // imm[11] lives in instr[7]: branch with only that bit -> imm = 0x800
    dut->instr = (1u << 7) | 0x63;
    dut->eval();
    CHECK(dut->bjp_imm == 0x800, "branch imm[11] (instr[7]) should give 0x800, got 0x%08x",
          dut->bjp_imm);

    printf("\n=== e203_ifu_litedec: %d failure(s) ===\n", fail_count);
    return fail_count ? 1 : 0;
}
