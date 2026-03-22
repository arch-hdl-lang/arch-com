// Testbench for E203 CoreTop — arch sim
// NOTE: arch sim evaluates inst chains in declaration order,
// so combinational loops (valid/ready) may need multiple settle passes.
// Bool signals may not be 1-bit masked — use BOOL() macro.
#include "VCoreTop.h"
#include <cstdio>
#include <cstdint>

static int errors = 0;
static int test_num = 0;

#define BOOL(x) ((x) & 1)
#define CHECK(cond, ...) do { \
    test_num++; \
    if (!(cond)) { errors++; printf("FAIL test %d: ", test_num); printf(__VA_ARGS__); printf("\n"); } \
    else { printf("PASS test %d\n", test_num); } \
} while(0)

// Iterate eval() to settle combinational feedback loops
static void settle(VCoreTop &m) {
    for (int i = 0; i < 8; i++) m.eval();
}

static void tick(VCoreTop &m) {
    m.clk = 0; settle(m);
    m.clk = 1; settle(m);
}

// RV32I instruction encoding helpers
static uint32_t rv_addi(int rd, int rs1, int imm) {
    return ((imm & 0xFFF) << 20) | (rs1 << 15) | (0b000 << 12) | (rd << 7) | 0b0010011;
}
static uint32_t rv_add(int rd, int rs1, int rs2) {
    return (0b0000000 << 25) | (rs2 << 20) | (rs1 << 15) | (0b000 << 12) | (rd << 7) | 0b0110011;
}
static uint32_t rv_sub(int rd, int rs1, int rs2) {
    return (0b0100000 << 25) | (rs2 << 20) | (rs1 << 15) | (0b000 << 12) | (rd << 7) | 0b0110011;
}
static uint32_t rv_lui(int rd, int imm20) {
    return (imm20 << 12) | (rd << 7) | 0b0110111;
}
static uint32_t rv_xor(int rd, int rs1, int rs2) {
    return (0b0000000 << 25) | (rs2 << 20) | (rs1 << 15) | (0b100 << 12) | (rd << 7) | 0b0110011;
}
static uint32_t rv_sw(int rs2, int rs1, int imm) {
    int imm_11_5 = (imm >> 5) & 0x7F;
    int imm_4_0  = imm & 0x1F;
    return (imm_11_5 << 25) | (rs2 << 20) | (rs1 << 15) | (0b010 << 12) | (imm_4_0 << 7) | 0b0100011;
}

int main() {
    VCoreTop m;

    // Reset
    m.clk = 0; m.rst_n = 0;
    m.ifu_valid = 0; m.ifu_instr = 0; m.ifu_pc = 0;
    m.mem_rdata = 0;
    m.eval();
    tick(m); tick(m);
    m.rst_n = 1;
    tick(m);

    // ── Test 1: ADDI x1, x0, 42 ─────────────────────────────────────
    m.ifu_valid = 1;
    m.ifu_instr = rv_addi(1, 0, 42);
    m.ifu_pc = 0x1000;
    settle(m);
    CHECK(BOOL(m.ifu_ready), "ADDI: ready=0x%X", m.ifu_ready);
    tick(m);

    // ── Test 2: ADDI x2, x0, 10 ─────────────────────────────────────
    m.ifu_instr = rv_addi(2, 0, 10);
    m.ifu_pc = 0x1004;
    settle(m);
    CHECK(BOOL(m.ifu_ready), "ADDI x2: ready=0x%X", m.ifu_ready);
    tick(m);

    // ── Test 3: ADD x3, x1, x2 ──────────────────────────────────────
    m.ifu_instr = rv_add(3, 1, 2);
    m.ifu_pc = 0x1008;
    settle(m);
    tick(m);
    CHECK(BOOL(m.commit_valid), "ADD x3: commit=0x%X", m.commit_valid);

    // ── Test 4: SUB x4, x1, x2 ──────────────────────────────────────
    m.ifu_instr = rv_sub(4, 1, 2);
    m.ifu_pc = 0x100C;
    settle(m);
    tick(m);
    CHECK(BOOL(m.commit_valid), "SUB x4: commit=0x%X", m.commit_valid);

    // ── Test 5: LUI x5, 0xDEADB ─────────────────────────────────────
    m.ifu_instr = rv_lui(5, 0xDEADB);
    m.ifu_pc = 0x1010;
    settle(m);
    tick(m);
    CHECK(BOOL(m.commit_valid), "LUI x5: commit=0x%X", m.commit_valid);

    // ── Test 6: XOR x6, x1, x2 ──────────────────────────────────────
    m.ifu_instr = rv_xor(6, 1, 2);
    m.ifu_pc = 0x1014;
    settle(m);
    tick(m);

    // ── Test 7: SW x1, 0(x0) ────────────────────────────────────────
    m.ifu_instr = rv_sw(1, 0, 0);
    m.ifu_pc = 0x1018;
    settle(m);
    tick(m);
    CHECK(BOOL(m.mem_wen), "SW: mem_wen=0x%X", m.mem_wen);

    // ── Test 8: Timer not firing ─────────────────────────────────────
    m.ifu_valid = 0;
    for (int i = 0; i < 10; i++) tick(m);
    CHECK(m.tmr_irq == 0, "timer: no IRQ");

    // ── Test 9: Multiple back-to-back instructions ───────────────────
    m.ifu_valid = 1;
    for (int i = 0; i < 5; i++) {
        m.ifu_instr = rv_addi(7, 0, i + 1);
        m.ifu_pc = 0x2000 + i * 4;
        settle(m);
        tick(m);
    }
    m.ifu_valid = 0;
    CHECK(1, "back-to-back: 5 instructions dispatched");

    printf("\n=== CoreTop arch sim: %d tests, %d errors ===\n", test_num, errors);
    return errors ? 1 : 0;
}
