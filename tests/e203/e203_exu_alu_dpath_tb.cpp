// ARCH sim testbench for e203_exu_alu_dpath — E203 shared ALU datapath.
// Tests: regular ALU ops (add/sub/logic/shift/slt/sltu/lui) including signed
// edge cases (INT_MIN, sra sign fill, shift-by-31), BJP compare resolution
// through the shared adder (eq/ne/lt/gt/ltu/gtu) and target-address add, AGU
// AMO ops (swap/and/or/xor/max/min/maxu/minu), MulDiv shared-adder add/sub,
// and the no-reset shared-buffer registers (load on ena, hold otherwise).
//
// NOTE: this replaces a stale tb (VAluDpath.h) that targeted an earlier
// revision of the fixture before the e203 fixtures were rewritten against the
// real E203 RTL, which renamed the construct to `e203_exu_alu_dpath`. The old
// tb has not compiled since. Ported to the current class name
// (Ve203_exu_alu_dpath).
//
// Run with:
//   arch sim tests/e203/e203_exu_alu_dpath.arch --tb tests/e203/e203_exu_alu_dpath_tb.cpp

#include "Ve203_exu_alu_dpath.h"
#include <cstdio>
#include <cstdint>

static int fail_count = 0;

#define CHECK(cond, fmt, ...) do { \
    if (!(cond)) { \
        printf("  FAIL: " fmt "\n", ##__VA_ARGS__); \
        fail_count++; \
    } \
} while(0)

static Ve203_exu_alu_dpath* dut;

static void tick() {
    dut->clk = 0; dut->eval();
    dut->clk = 1; dut->eval();
}

static void clear_inputs() {
    dut->clk = 0;
    dut->rst_n = 1;
    dut->alu_req_alu = 0;
    dut->alu_req_alu_add = 0; dut->alu_req_alu_sub = 0;
    dut->alu_req_alu_xor = 0; dut->alu_req_alu_sll = 0;
    dut->alu_req_alu_srl = 0; dut->alu_req_alu_sra = 0;
    dut->alu_req_alu_or = 0; dut->alu_req_alu_and = 0;
    dut->alu_req_alu_slt = 0; dut->alu_req_alu_sltu = 0;
    dut->alu_req_alu_lui = 0;
    dut->alu_req_alu_op1 = 0; dut->alu_req_alu_op2 = 0;
    dut->bjp_req_alu = 0;
    dut->bjp_req_alu_op1 = 0; dut->bjp_req_alu_op2 = 0;
    dut->bjp_req_alu_cmp_eq = 0; dut->bjp_req_alu_cmp_ne = 0;
    dut->bjp_req_alu_cmp_lt = 0; dut->bjp_req_alu_cmp_gt = 0;
    dut->bjp_req_alu_cmp_ltu = 0; dut->bjp_req_alu_cmp_gtu = 0;
    dut->bjp_req_alu_add = 0;
    dut->agu_req_alu = 0;
    dut->agu_req_alu_op1 = 0; dut->agu_req_alu_op2 = 0;
    dut->agu_req_alu_swap = 0; dut->agu_req_alu_add = 0;
    dut->agu_req_alu_and = 0; dut->agu_req_alu_or = 0;
    dut->agu_req_alu_xor = 0; dut->agu_req_alu_max = 0;
    dut->agu_req_alu_min = 0; dut->agu_req_alu_maxu = 0;
    dut->agu_req_alu_minu = 0;
    dut->agu_sbf_0_ena = 0; dut->agu_sbf_0_nxt = 0;
    dut->agu_sbf_1_ena = 0; dut->agu_sbf_1_nxt = 0;
    dut->muldiv_req_alu = 0;
    dut->muldiv_req_alu_op1 = 0; dut->muldiv_req_alu_op2 = 0;
    dut->muldiv_req_alu_add = 0; dut->muldiv_req_alu_sub = 0;
    dut->muldiv_sbf_0_ena = 0; dut->muldiv_sbf_0_nxt = 0;
    dut->muldiv_sbf_1_ena = 0; dut->muldiv_sbf_1_nxt = 0;
    dut->eval();
}

// Drive a single regular ALU op (op selected by pointer-to-member).
static uint32_t alu_op(uint8_t Ve203_exu_alu_dpath::*sel, uint32_t op1, uint32_t op2) {
    clear_inputs();
    dut->alu_req_alu = 1;
    dut->*sel = 1;
    dut->alu_req_alu_op1 = op1;
    dut->alu_req_alu_op2 = op2;
    dut->eval();
    return dut->alu_req_alu_res;
}

// Drive a single BJP compare and return cmp_res.
static uint8_t bjp_cmp(uint8_t Ve203_exu_alu_dpath::*sel, uint32_t rs1, uint32_t rs2) {
    clear_inputs();
    dut->bjp_req_alu = 1;
    dut->*sel = 1;
    dut->bjp_req_alu_op1 = rs1;
    dut->bjp_req_alu_op2 = rs2;
    dut->eval();
    return dut->bjp_req_alu_cmp_res;
}

// Drive a single AGU AMO op and return the result.
static uint32_t agu_op(uint8_t Ve203_exu_alu_dpath::*sel, uint32_t op1, uint32_t op2) {
    clear_inputs();
    dut->agu_req_alu = 1;
    dut->*sel = 1;
    dut->agu_req_alu_op1 = op1;
    dut->agu_req_alu_op2 = op2;
    dut->eval();
    return dut->agu_req_alu_res;
}

int main() {
    dut = new Ve203_exu_alu_dpath;
    typedef Ve203_exu_alu_dpath D;

    // ── Test 1: ALU arithmetic ───────────────────────────────────────
    printf("Test 1: ALU add/sub\n");
    CHECK(alu_op(&D::alu_req_alu_add, 5, 7) == 12, "5+7 should be 12");
    CHECK(alu_op(&D::alu_req_alu_add, 0xFFFFFFFFu, 1) == 0, "add should wrap mod 2^32");
    CHECK(alu_op(&D::alu_req_alu_add, 0x80000000u, 0x80000000u) == 0, "INT_MIN+INT_MIN wraps to 0");
    CHECK(alu_op(&D::alu_req_alu_sub, 10, 3) == 7, "10-3 should be 7");
    CHECK(alu_op(&D::alu_req_alu_sub, 3, 10) == 0xFFFFFFF9u, "3-10 should be -7 (0xFFFFFFF9)");
    CHECK(alu_op(&D::alu_req_alu_sub, 0, 0x80000000u) == 0x80000000u, "0-INT_MIN wraps to INT_MIN");

    // ── Test 2: ALU logic ────────────────────────────────────────────
    printf("Test 2: ALU logic ops\n");
    CHECK(alu_op(&D::alu_req_alu_xor, 0xFF00FF00u, 0x0FF00FF0u) == 0xF0F0F0F0u, "xor result wrong");
    CHECK(alu_op(&D::alu_req_alu_or, 0xF0F00000u, 0x0F0F0000u) == 0xFFFF0000u, "or result wrong");
    CHECK(alu_op(&D::alu_req_alu_and, 0xFF00FF00u, 0x0FF00FF0u) == 0x0F000F00u, "and result wrong");
    CHECK(alu_op(&D::alu_req_alu_lui, 0, 0xABCDE000u) == 0xABCDE000u, "lui should pass op2");

    // ── Test 3: ALU shifts ───────────────────────────────────────────
    printf("Test 3: ALU shifts\n");
    CHECK(alu_op(&D::alu_req_alu_sll, 1, 31) == 0x80000000u, "1<<31 should be 0x80000000");
    CHECK(alu_op(&D::alu_req_alu_sll, 0x3, 4) == 0x30, "3<<4 should be 0x30");
    CHECK(alu_op(&D::alu_req_alu_srl, 0x80000000u, 31) == 1, "0x80000000>>31 (srl) should be 1");
    CHECK(alu_op(&D::alu_req_alu_sra, 0x80000000u, 31) == 0xFFFFFFFFu, "sra of INT_MIN by 31 should be -1");
    CHECK(alu_op(&D::alu_req_alu_sra, 0x40000000u, 4) == 0x04000000u, "sra of positive should not sign-fill");
    // Shift amount is op2[4:0]: 32 acts as 0.
    CHECK(alu_op(&D::alu_req_alu_sll, 0x1234, 32) == 0x1234, "sll by 32 should use shamt[4:0]=0");

    // ── Test 4: ALU comparisons ──────────────────────────────────────
    printf("Test 4: ALU slt/sltu\n");
    CHECK(alu_op(&D::alu_req_alu_slt, (uint32_t)-1, 1) == 1, "slt(-1,1) should be 1");
    CHECK(alu_op(&D::alu_req_alu_slt, 1, (uint32_t)-1) == 0, "slt(1,-1) should be 0");
    CHECK(alu_op(&D::alu_req_alu_slt, 0x80000000u, 0x7FFFFFFFu) == 1, "slt(INT_MIN,INT_MAX) should be 1");
    CHECK(alu_op(&D::alu_req_alu_slt, 5, 5) == 0, "slt(5,5) should be 0");
    CHECK(alu_op(&D::alu_req_alu_sltu, 1, 0xFFFFFFFFu) == 1, "sltu(1,UINT_MAX) should be 1");
    CHECK(alu_op(&D::alu_req_alu_sltu, 0xFFFFFFFFu, 1) == 0, "sltu(UINT_MAX,1) should be 0");
    CHECK(alu_op(&D::alu_req_alu_sltu, 7, 7) == 0, "sltu(7,7) should be 0");

    // ── Test 5: BJP compares ─────────────────────────────────────────
    printf("Test 5: BJP compare resolution\n");
    CHECK(bjp_cmp(&D::bjp_req_alu_cmp_eq, 42, 42) == 1, "beq(42,42) should take");
    CHECK(bjp_cmp(&D::bjp_req_alu_cmp_eq, 42, 43) == 0, "beq(42,43) should not take");
    CHECK(bjp_cmp(&D::bjp_req_alu_cmp_ne, 42, 43) == 1, "bne(42,43) should take");
    CHECK(bjp_cmp(&D::bjp_req_alu_cmp_ne, 42, 42) == 0, "bne(42,42) should not take");
    CHECK(bjp_cmp(&D::bjp_req_alu_cmp_lt, (uint32_t)-5, 3) == 1, "blt(-5,3) should take");
    CHECK(bjp_cmp(&D::bjp_req_alu_cmp_lt, 3, (uint32_t)-5) == 0, "blt(3,-5) should not take");
    // cmp_gt is strict signed greater (bge is decoded upstream with prdt inversion).
    CHECK(bjp_cmp(&D::bjp_req_alu_cmp_gt, 3, (uint32_t)-5) == 1, "cmp_gt(3,-5) should be 1");
    CHECK(bjp_cmp(&D::bjp_req_alu_cmp_gt, 3, 3) == 0, "cmp_gt(3,3) is strict: 0");
    CHECK(bjp_cmp(&D::bjp_req_alu_cmp_ltu, 1, 0xFFFFFFF0u) == 1, "bltu(1,big) should take");
    CHECK(bjp_cmp(&D::bjp_req_alu_cmp_ltu, 0xFFFFFFF0u, 1) == 0, "bltu(big,1) should not take");
    CHECK(bjp_cmp(&D::bjp_req_alu_cmp_gtu, 0xFFFFFFF0u, 1) == 1, "cmp_gtu(big,1) should be 1");
    CHECK(bjp_cmp(&D::bjp_req_alu_cmp_gtu, 7, 7) == 0, "cmp_gtu(7,7) is strict: 0");

    // ── Test 6: BJP target-address add ───────────────────────────────
    printf("Test 6: BJP adder\n");
    clear_inputs();
    dut->bjp_req_alu = 1;
    dut->bjp_req_alu_add = 1;
    dut->bjp_req_alu_op1 = 0x1000;      // pc
    dut->bjp_req_alu_op2 = 0xFFFFFF00u; // imm = -256
    dut->eval();
    CHECK(dut->bjp_req_alu_add_res == 0xF00, "branch target 0x1000-256 should be 0xF00, got 0x%08x",
          dut->bjp_req_alu_add_res);

    // ── Test 7: AGU AMO ops ──────────────────────────────────────────
    printf("Test 7: AGU AMO ops\n");
    CHECK(agu_op(&D::agu_req_alu_add, 0x100, 0x23) == 0x123, "amo add wrong");
    CHECK(agu_op(&D::agu_req_alu_and, 0xFF0F, 0x0FF0) == 0x0F00, "amo and wrong");
    CHECK(agu_op(&D::agu_req_alu_or, 0xF000, 0x000F) == 0xF00F, "amo or wrong");
    CHECK(agu_op(&D::agu_req_alu_xor, 0xFFFF, 0x0F0F) == 0xF0F0, "amo xor wrong");
    CHECK(agu_op(&D::agu_req_alu_swap, 0x111, 0x222) == 0x222, "amoswap should return op2");
    CHECK(agu_op(&D::agu_req_alu_max, (uint32_t)-3, 2) == 2, "amomax(-3,2) should be 2");
    CHECK(agu_op(&D::agu_req_alu_max, 5, (uint32_t)-9) == 5, "amomax(5,-9) should be 5");
    CHECK(agu_op(&D::agu_req_alu_min, (uint32_t)-3, 2) == (uint32_t)-3, "amomin(-3,2) should be -3");
    CHECK(agu_op(&D::agu_req_alu_maxu, (uint32_t)-3, 2) == (uint32_t)-3, "amomaxu(0xFFFFFFFD,2) should pick big");
    CHECK(agu_op(&D::agu_req_alu_minu, (uint32_t)-3, 2) == 2, "amominu(0xFFFFFFFD,2) should be 2");

    // ── Test 8: MulDiv shared adder ──────────────────────────────────
    printf("Test 8: MulDiv adder\n");
    clear_inputs();
    dut->muldiv_req_alu = 1;
    dut->muldiv_req_alu_add = 1;
    dut->muldiv_req_alu_op1 = 0x40000000u;
    dut->muldiv_req_alu_op2 = 0x40000000u;
    dut->eval();
    CHECK(dut->muldiv_req_alu_res == 0x80000000u, "muldiv add wrong, got 0x%08x", dut->muldiv_req_alu_res);
    dut->muldiv_req_alu_add = 0;
    dut->muldiv_req_alu_sub = 1;
    dut->muldiv_req_alu_op1 = 5;
    dut->muldiv_req_alu_op2 = 9;
    dut->eval();
    CHECK(dut->muldiv_req_alu_res == (uint32_t)-4, "muldiv sub 5-9 should be -4, got 0x%08x", dut->muldiv_req_alu_res);
    // MulDiv has top mux priority: an ALU request in the same cycle must not
    // steal the operands.
    dut->alu_req_alu = 1;
    dut->alu_req_alu_op1 = 0xAAAA;
    dut->alu_req_alu_op2 = 0x5555;
    dut->eval();
    CHECK(dut->muldiv_req_alu_res == (uint32_t)-4, "muldiv result must win the operand mux, got 0x%08x",
          dut->muldiv_req_alu_res);

    // ── Test 9: Shared buffer registers ──────────────────────────────
    printf("Test 9: Shared buffers\n");
    clear_inputs();
    dut->agu_sbf_0_ena = 1; dut->agu_sbf_0_nxt = 0xA0A0A0A0u;
    dut->agu_sbf_1_ena = 1; dut->agu_sbf_1_nxt = 0xB1B1B1B1u;
    dut->muldiv_sbf_0_ena = 1; dut->muldiv_sbf_0_nxt = 0xC2C2C2C2u;
    dut->muldiv_sbf_1_ena = 1; dut->muldiv_sbf_1_nxt = 0xD3D3D3D3u;
    tick();
    dut->agu_sbf_0_ena = 0; dut->agu_sbf_1_ena = 0;
    dut->muldiv_sbf_0_ena = 0; dut->muldiv_sbf_1_ena = 0;
    dut->eval();
    CHECK(dut->agu_sbf_0_r == 0xA0A0A0A0u, "agu sbf0 should load, got 0x%08x", dut->agu_sbf_0_r);
    CHECK(dut->agu_sbf_1_r == 0xB1B1B1B1u, "agu sbf1 should load, got 0x%08x", dut->agu_sbf_1_r);
    CHECK(dut->muldiv_sbf_0_r == 0xC2C2C2C2u, "muldiv sbf0 should load, got 0x%08x", dut->muldiv_sbf_0_r);
    CHECK(dut->muldiv_sbf_1_r == 0xD3D3D3D3u, "muldiv sbf1 should load, got 0x%08x", dut->muldiv_sbf_1_r);
    // Hold with ena low even though nxt changes.
    dut->agu_sbf_0_nxt = 0xDEAD;
    dut->muldiv_sbf_1_nxt = 0xBEEF;
    for (int i = 0; i < 3; i++) tick();
    CHECK(dut->agu_sbf_0_r == 0xA0A0A0A0u, "agu sbf0 must hold without ena, got 0x%08x", dut->agu_sbf_0_r);
    CHECK(dut->muldiv_sbf_1_r == 0xD3D3D3D3u, "muldiv sbf1 must hold without ena, got 0x%08x", dut->muldiv_sbf_1_r);
    // Selective update: only sbf1 enabled.
    dut->agu_sbf_1_ena = 1; dut->agu_sbf_1_nxt = 0x77777777u;
    tick();
    dut->agu_sbf_1_ena = 0;
    dut->eval();
    CHECK(dut->agu_sbf_0_r == 0xA0A0A0A0u, "agu sbf0 must hold on selective update, got 0x%08x", dut->agu_sbf_0_r);
    CHECK(dut->agu_sbf_1_r == 0x77777777u, "agu sbf1 should update, got 0x%08x", dut->agu_sbf_1_r);

    printf("\n=== e203_exu_alu_dpath: %d failure(s) ===\n", fail_count);
    return fail_count ? 1 : 0;
}
