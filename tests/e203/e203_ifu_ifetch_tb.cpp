// Testbench for E203 IfuIfetch — arch sim
#include "VIfuIfetch.h"
#include <cstdio>
#include <cstdlib>

static int errors = 0;
static int test_num = 0;

#define CHECK(cond, ...) do { \
    test_num++; \
    if (!(cond)) { errors++; printf("FAIL test %d: ", test_num); printf(__VA_ARGS__); printf("\n"); } \
    else { printf("PASS test %d\n", test_num); } \
} while(0)

static void tick(VIfuIfetch &m) {
    m.clk = 0; m.eval();
    m.clk = 1; m.eval();
}

int main() {
    VIfuIfetch m;
    m.clk = 0; m.rst = 0;  // async low reset = asserted
    m.req_ready = 0; m.rsp_valid = 0; m.rsp_instr = 0; m.rsp_err = 0;
    m.redirect = 0; m.redirect_pc = 0; m.o_ready = 0;
    m.eval();

    // Apply reset for 2 cycles
    tick(m); tick(m);

    // Release reset
    m.rst = 1;

    // ── Cycle 1: Idle → WaitGnt (loads RESET_PC) ────────────
    tick(m);
    // After Idle state, PC should be RESET_PC = 0x80000000
    // Now in WaitGnt — should see req_valid=1
    CHECK(m.req_valid == 1, "c1: req_valid should be 1, got %d", m.req_valid);
    CHECK(m.req_addr == 0x80000000u, "c1: req_addr=0x%08X, exp 0x80000000", m.req_addr);

    // ── Cycle 2: WaitGnt, grant arrives → WaitRsp ───────────
    m.req_ready = 1;
    tick(m);
    m.req_ready = 0;
    // Should be in WaitRsp now
    CHECK(m.rsp_ready == 1, "c2: rsp_ready should be 1 (WaitRsp)");
    CHECK(m.req_valid == 0, "c2: req_valid should be 0 (not WaitGnt)");

    // ── Cycle 3: WaitRsp, response arrives → WaitGnt ────────
    m.rsp_valid = 1; m.rsp_instr = 0xDEADBEEF; m.rsp_err = 0;
    tick(m);
    m.rsp_valid = 0;
    // Should be back in WaitGnt with PC+4
    CHECK(m.req_valid == 1, "c3: req_valid should be 1 (back to WaitGnt)");
    CHECK(m.req_addr == 0x80000004u, "c3: req_addr=0x%08X, exp 0x80000004", m.req_addr);

    // ── Cycle 4: Another grant → WaitRsp ────────────────────
    m.req_ready = 1;
    tick(m);
    m.req_ready = 0;
    CHECK(m.rsp_ready == 1, "c4: rsp_ready in WaitRsp");

    // ── Cycle 5: Response with bus error ─────────────────────
    m.rsp_valid = 1; m.rsp_instr = 0x12345678; m.rsp_err = 1;
    tick(m);
    m.rsp_valid = 0;
    CHECK(m.req_valid == 1, "c5: back to WaitGnt");
    CHECK(m.req_addr == 0x80000008u, "c5: req_addr=0x%08X, exp 0x80000008", m.req_addr);

    // ── Cycle 6: Redirect during WaitGnt ─────────────────────
    m.redirect = 1; m.redirect_pc = 0x00001000;
    tick(m);
    m.redirect = 0;
    // Should be in Abort, then next cycle back to WaitGnt
    // In Abort: rsp_ready=1, req_valid=0
    CHECK(m.rsp_ready == 1, "c6: in Abort, rsp_ready=1");

    // ── Cycle 7: Abort → WaitGnt with redirected PC ──────────
    tick(m);
    CHECK(m.req_valid == 1, "c7: back to WaitGnt after abort");
    CHECK(m.req_addr == 0x00001000u, "c7: req_addr=0x%08X, exp 0x00001000", m.req_addr);

    // ── Cycle 8-9: Normal fetch again ────────────────────────
    m.req_ready = 1;
    tick(m);
    m.req_ready = 0;
    m.rsp_valid = 1; m.rsp_instr = 0xABCD1234; m.rsp_err = 0;
    tick(m);
    m.rsp_valid = 0;
    CHECK(m.req_valid == 1, "c9: WaitGnt again");
    CHECK(m.req_addr == 0x00001004u, "c9: req_addr=0x%08X, exp 0x00001004", m.req_addr);

    // ── Cycle 10: Redirect during WaitRsp ────────────────────
    m.req_ready = 1;
    tick(m);
    m.req_ready = 0;
    // Now in WaitRsp
    m.redirect = 1; m.redirect_pc = 0xFFFF0000;
    tick(m);
    m.redirect = 0;
    // Should be in Abort
    CHECK(m.rsp_ready == 1, "c10: Abort after redirect in WaitRsp");

    tick(m);
    CHECK(m.req_valid == 1, "c11: back to WaitGnt");
    CHECK(m.req_addr == 0xFFFF0000u, "c11: req_addr=0x%08X, exp 0xFFFF0000", m.req_addr);

    // ── Cycle 12-13: WaitGnt stall (no grant for 2 cycles) ──
    m.req_ready = 0;
    tick(m);
    CHECK(m.req_valid == 1, "c12: still WaitGnt, stalled");
    CHECK(m.req_addr == 0xFFFF0000u, "c12: PC unchanged while stalled");
    tick(m);
    CHECK(m.req_valid == 1, "c13: still stalled");

    // ── Cycle 14: Grant finally arrives ──────────────────────
    m.req_ready = 1;
    tick(m);
    m.req_ready = 0;
    CHECK(m.rsp_ready == 1, "c14: in WaitRsp");

    // ── Cycle 15: PC alignment test (bottom 2 bits = 0) ─────
    m.rsp_valid = 1; m.rsp_instr = 0x00000013; m.rsp_err = 0;
    tick(m);
    m.rsp_valid = 0;
    CHECK(m.req_valid == 1, "c15: WaitGnt");
    // 0xFFFF0000 + 4 = 0xFFFF0004, aligned → 0xFFFF0004
    CHECK(m.req_addr == 0xFFFF0004u, "c15: req_addr=0x%08X, exp 0xFFFF0004", m.req_addr);

    printf("\n=== IfuIfetch: %d tests, %d errors ===\n", test_num, errors);
    return errors ? 1 : 0;
}
