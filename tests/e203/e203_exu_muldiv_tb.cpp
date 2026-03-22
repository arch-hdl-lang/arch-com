// ARCH sim testbench for ExuMuldiv — RV32M multiply/divide unit
// Tests: MUL, MULH, MULHSU, MULHU, DIV, DIVU, REM, REMU, divide-by-zero

#include "VExuMuldiv.h"
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

static VExuMuldiv* dut;

static void tick() {
    dut->clk = 0; dut->eval();
    dut->clk = 1; dut->eval();
}

static void reset() {
    dut->rst_n = 0;
    dut->i_valid = 0;
    dut->o_ready = 0;
    for (int i = 0; i < 3; i++) tick();
    dut->rst_n = 1;
    tick();
}

// Issue an operation and wait for result. Returns o_wdat.
static uint32_t run_op(uint32_t rs1, uint32_t rs2,
                       bool mul, bool mulh, bool mulhsu, bool mulhu,
                       bool div, bool divu, bool rem, bool remu) {
    // Wait until ready
    while (!dut->i_ready) tick();

    dut->i_valid = 1;
    dut->i_rs1 = rs1;
    dut->i_rs2 = rs2;
    dut->i_mul = mul; dut->i_mulh = mulh;
    dut->i_mulhsu = mulhsu; dut->i_mulhu = mulhu;
    dut->i_div = div; dut->i_divu = divu;
    dut->i_rem = rem; dut->i_remu = remu;
    tick();
    dut->i_valid = 0;
    dut->i_mul = 0; dut->i_mulh = 0; dut->i_mulhsu = 0; dut->i_mulhu = 0;
    dut->i_div = 0; dut->i_divu = 0; dut->i_rem = 0; dut->i_remu = 0;

    // Wait for result
    int cycles = 0;
    while (!dut->o_valid && cycles < 100) { tick(); cycles++; }
    if (!dut->o_valid) { printf("  TIMEOUT!\n"); fail_count++; return 0; }

    uint32_t result = dut->o_wdat;
    dut->o_ready = 1;
    tick();
    dut->o_ready = 0;
    return result;
}

static uint32_t do_mul(uint32_t a, uint32_t b) { return run_op(a,b,1,0,0,0,0,0,0,0); }
static uint32_t do_mulh(uint32_t a, uint32_t b) { return run_op(a,b,0,1,0,0,0,0,0,0); }
static uint32_t do_mulhsu(uint32_t a, uint32_t b) { return run_op(a,b,0,0,1,0,0,0,0,0); }
static uint32_t do_mulhu(uint32_t a, uint32_t b) { return run_op(a,b,0,0,0,1,0,0,0,0); }
static uint32_t do_div(uint32_t a, uint32_t b) { return run_op(a,b,0,0,0,0,1,0,0,0); }
static uint32_t do_divu(uint32_t a, uint32_t b) { return run_op(a,b,0,0,0,0,0,1,0,0); }
static uint32_t do_rem(uint32_t a, uint32_t b) { return run_op(a,b,0,0,0,0,0,0,1,0); }
static uint32_t do_remu(uint32_t a, uint32_t b) { return run_op(a,b,0,0,0,0,0,0,0,1); }

int main() {
    dut = new VExuMuldiv();
    reset();

    uint32_t r;

    printf("=== ExuMuldiv testbench ===\n");

    // ── MUL tests ────────────────────────────────────────────────────
    printf("Test 1: MUL 7 * 3 = 21\n");
    r = do_mul(7, 3);
    CHECK(r == 21, "got 0x%08X exp 21", r);

    printf("Test 2: MUL 0xFFFF * 0xFFFF = 0xFFFE0001\n");
    r = do_mul(0xFFFF, 0xFFFF);
    CHECK(r == 0xFFFE0001, "got 0x%08X exp 0xFFFE0001", r);

    printf("Test 3: MUL (-3) * 5 = -15 (lower 32)\n");
    r = do_mul((uint32_t)-3, 5);
    CHECK(r == (uint32_t)-15, "got 0x%08X exp 0xFFFFFFF1", r);

    printf("Test 4: MUL (-7) * (-6) = 42 (lower 32)\n");
    r = do_mul((uint32_t)-7, (uint32_t)-6);
    CHECK(r == 42, "got 0x%08X exp 42", r);

    // ── MULHU test ───────────────────────────────────────────────────
    printf("Test 5: MULHU 0xFFFFFFFF * 0xFFFFFFFF (upper 32)\n");
    r = do_mulhu(0xFFFFFFFF, 0xFFFFFFFF);
    CHECK(r == 0xFFFFFFFE, "got 0x%08X exp 0xFFFFFFFE", r);

    printf("Test 6: MULHU 0x80000000 * 2 (upper 32)\n");
    r = do_mulhu(0x80000000, 2);
    CHECK(r == 1, "got 0x%08X exp 1", r);

    // ── MULH test ────────────────────────────────────────────────────
    printf("Test 7: MULH (-1) * (-1) (upper 32, signed)\n");
    r = do_mulh((uint32_t)-1, (uint32_t)-1);
    CHECK(r == 0, "got 0x%08X exp 0", r);

    printf("Test 8: MULH 0x40000000 * 4 (upper 32, signed)\n");
    r = do_mulh(0x40000000, 4);
    CHECK(r == 1, "got 0x%08X exp 1", r);

    // ── DIVU tests ───────────────────────────────────────────────────
    printf("Test 9: DIVU 20 / 3 = 6\n");
    r = do_divu(20, 3);
    CHECK(r == 6, "got 0x%08X exp 6", r);

    printf("Test 10: DIVU 100 / 7 = 14\n");
    r = do_divu(100, 7);
    CHECK(r == 14, "got 0x%08X exp 14", r);

    printf("Test 11: DIVU 0xFFFFFFFF / 2 = 0x7FFFFFFF\n");
    r = do_divu(0xFFFFFFFF, 2);
    CHECK(r == 0x7FFFFFFF, "got 0x%08X exp 0x7FFFFFFF", r);

    // ── REMU tests ───────────────────────────────────────────────────
    printf("Test 12: REMU 20 %% 3 = 2\n");
    r = do_remu(20, 3);
    CHECK(r == 2, "got 0x%08X exp 2", r);

    printf("Test 13: REMU 100 %% 7 = 2\n");
    r = do_remu(100, 7);
    CHECK(r == 2, "got 0x%08X exp 2", r);

    // ── DIV (signed) tests ───────────────────────────────────────────
    printf("Test 14: DIV (-20) / 3 = -6\n");
    r = do_div((uint32_t)-20, 3);
    CHECK(r == (uint32_t)-6, "got 0x%08X exp 0xFFFFFFFA", r);

    printf("Test 15: DIV 20 / (-3) = -6\n");
    r = do_div(20, (uint32_t)-3);
    CHECK(r == (uint32_t)-6, "got 0x%08X exp 0xFFFFFFFA", r);

    printf("Test 16: DIV (-20) / (-3) = 6\n");
    r = do_div((uint32_t)-20, (uint32_t)-3);
    CHECK(r == 6, "got 0x%08X exp 6", r);

    // ── REM (signed) tests ───────────────────────────────────────────
    printf("Test 17: REM (-20) %% 3 = -2\n");
    r = do_rem((uint32_t)-20, 3);
    CHECK(r == (uint32_t)-2, "got 0x%08X exp 0xFFFFFFFE", r);

    printf("Test 18: REM 20 %% (-3) = 2\n");
    r = do_rem(20, (uint32_t)-3);
    CHECK(r == 2, "got 0x%08X exp 2", r);

    // ── Divide by zero ───────────────────────────────────────────────
    printf("Test 19: DIVU x / 0 = 0xFFFFFFFF\n");
    r = do_divu(42, 0);
    CHECK(r == 0xFFFFFFFF, "got 0x%08X exp 0xFFFFFFFF", r);

    printf("Test 20: REMU x %% 0 = x\n");
    r = do_remu(42, 0);
    CHECK(r == 42, "got 0x%08X exp 42", r);

    // ── Edge cases ───────────────────────────────────────────────────
    printf("Test 21: MUL 0 * 12345 = 0\n");
    r = do_mul(0, 12345);
    CHECK(r == 0, "got 0x%08X exp 0", r);

    printf("Test 22: MUL 1 * 1 = 1\n");
    r = do_mul(1, 1);
    CHECK(r == 1, "got 0x%08X exp 1", r);

    printf("Test 23: DIVU 0 / 5 = 0\n");
    r = do_divu(0, 5);
    CHECK(r == 0, "got 0x%08X exp 0", r);

    printf("Test 24: DIVU 5 / 5 = 1\n");
    r = do_divu(5, 5);
    CHECK(r == 1, "got 0x%08X exp 1", r);

    // ── Summary ──────────────────────────────────────────────────────
    if (fail_count == 0) {
        printf("\nAll 24 tests PASSED\n");
    } else {
        printf("\n%d test(s) FAILED\n", fail_count);
    }

    delete dut;
    return fail_count ? 1 : 0;
}
