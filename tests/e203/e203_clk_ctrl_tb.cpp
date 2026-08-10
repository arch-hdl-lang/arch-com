// ARCH sim testbench for e203_clk_ctrl — per-subsystem clock gating built
// from latch-based ICG cells (e203_clkgate). Tests: gated-off clocks staying
// low, enabled clocks following clk in both phases, the always-on clk_aon,
// per-subsystem independence of the six gates, core_cgstop force-off,
// latch-based glitch-free behavior (enable changes during the high phase do
// not propagate until after a low phase), test_mode bypass, and the
// itcm_ls/dtcm_ls light-sleep equations.
//
// NOTE: this replaces a stale tb (VClkCtrl.h, ifu_gate_en/clk_ifu port names)
// that targeted the pre-2026-04 PascalCase fixture generation. The current
// construct is `e203_clk_ctrl` with core_*_active inputs, clk_core_* outputs
// and an e203_clkgate ICG submodule, so the sim class is Ve203_clk_ctrl.
//
// Run with:
//   arch sim tests/e203/e203_clk_ctrl.arch tests/e203/e203_clkgate.arch \
//            --tb tests/e203/e203_clk_ctrl_tb.cpp

#include "Ve203_clk_ctrl.h"
#include <cstdio>
#include <cstdint>

static int fail_count = 0;

#define CHECK(cond, fmt, ...) do { \
    if (!(cond)) { \
        printf("  FAIL: " fmt "\n", ##__VA_ARGS__); \
        fail_count++; \
    } \
} while(0)

static Ve203_clk_ctrl* dut;

static void reset() {
    dut->clk = 0;
    dut->rst_n = 0;
    dut->test_mode = 0;
    dut->core_cgstop = 0;
    dut->core_ifu_active = 0;
    dut->core_exu_active = 0;
    dut->core_lsu_active = 0;
    dut->core_biu_active = 0;
    dut->itcm_active = 0;
    dut->dtcm_active = 0;
    dut->core_wfi = 0;
    for (int i = 0; i < 3; i++) {
        dut->clk = 0; dut->eval();
        dut->clk = 1; dut->eval();
    }
    dut->clk = 0;
    dut->rst_n = 1;
    dut->eval();
}

// One full clock cycle: low phase (latches ICG enables), then high phase.
static void cycle() {
    dut->clk = 0; dut->eval();
    dut->clk = 1; dut->eval();
}

int main() {
    dut = new Ve203_clk_ctrl;

    // ── Test 1: All inactive — every gated clock stays low ───────────
    printf("Test 1: All gated off\n");
    reset();
    cycle();                    // latch the all-zero enables, clk high
    CHECK(dut->clk_core_ifu == 0, "clk_core_ifu should be gated low, got %d", dut->clk_core_ifu);
    CHECK(dut->clk_core_exu == 0, "clk_core_exu should be gated low, got %d", dut->clk_core_exu);
    CHECK(dut->clk_core_lsu == 0, "clk_core_lsu should be gated low, got %d", dut->clk_core_lsu);
    CHECK(dut->clk_core_biu == 0, "clk_core_biu should be gated low, got %d", dut->clk_core_biu);
    CHECK(dut->clk_itcm == 0, "clk_itcm should be gated low, got %d", dut->clk_itcm);
    CHECK(dut->clk_dtcm == 0, "clk_dtcm should be gated low, got %d", dut->clk_dtcm);
    CHECK(dut->clk_aon == 1, "clk_aon is ungated and must follow clk=1, got %d", dut->clk_aon);
    dut->clk = 0; dut->eval();
    CHECK(dut->clk_aon == 0, "clk_aon must follow clk=0, got %d", dut->clk_aon);

    // ── Test 2: Enabled clock follows clk in both phases ─────────────
    printf("Test 2: IFU clock enabled\n");
    reset();
    dut->core_ifu_active = 1;
    cycle();                    // low phase latches enable, then clk=1
    CHECK(dut->clk_core_ifu == 1, "clk_core_ifu should follow clk high when active, got %d",
          dut->clk_core_ifu);
    CHECK(dut->clk_core_exu == 0, "clk_core_exu must stay gated (independent), got %d",
          dut->clk_core_exu);
    dut->clk = 0; dut->eval();
    CHECK(dut->clk_core_ifu == 0, "clk_core_ifu should follow clk low, got %d", dut->clk_core_ifu);

    // ── Test 3: Each gate is driven by its own active flag ───────────
    printf("Test 3: Per-subsystem independence\n");
    reset();
    dut->core_exu_active = 1;
    dut->itcm_active = 1;
    cycle();
    CHECK(dut->clk_core_exu == 1, "clk_core_exu should be running, got %d", dut->clk_core_exu);
    CHECK(dut->clk_itcm == 1, "clk_itcm should be running, got %d", dut->clk_itcm);
    CHECK(dut->clk_core_ifu == 0 && dut->clk_core_lsu == 0 && dut->clk_core_biu == 0 &&
          dut->clk_dtcm == 0, "unenabled clocks must stay low");

    // ── Test 4: core_cgstop forces all gated clocks off ──────────────
    printf("Test 4: core_cgstop\n");
    reset();
    dut->core_ifu_active = 1;
    dut->core_exu_active = 1;
    dut->core_lsu_active = 1;
    dut->core_biu_active = 1;
    dut->itcm_active = 1;
    dut->dtcm_active = 1;
    dut->core_cgstop = 1;
    cycle();
    CHECK(dut->clk_core_ifu == 0 && dut->clk_core_exu == 0 && dut->clk_core_lsu == 0 &&
          dut->clk_core_biu == 0 && dut->clk_itcm == 0 && dut->clk_dtcm == 0,
          "cgstop must gate every subsystem clock off");
    CHECK(dut->clk_aon == 1, "clk_aon is not affected by cgstop, got %d", dut->clk_aon);
    dut->core_cgstop = 0;
    cycle();
    CHECK(dut->clk_core_ifu == 1, "clocks should resume once cgstop clears, got %d",
          dut->clk_core_ifu);

    // ── Test 5: Latch-based ICG is glitch-free ───────────────────────
    printf("Test 5: Glitch-free latching\n");
    reset();
    cycle();                    // clk high, ifu enable latched at 0
    dut->core_ifu_active = 1;   // raise enable DURING the high phase
    dut->eval();
    CHECK(dut->clk_core_ifu == 0, "enable raised mid-high must not propagate this phase, got %d",
          dut->clk_core_ifu);
    cycle();                    // next low phase latches 1, then clk high
    CHECK(dut->clk_core_ifu == 1, "enable should propagate after the low phase, got %d",
          dut->clk_core_ifu);
    // Symmetrically: dropping the enable mid-high keeps the clock on.
    dut->core_ifu_active = 0;
    dut->eval();
    CHECK(dut->clk_core_ifu == 1, "enable dropped mid-high must hold through this phase, got %d",
          dut->clk_core_ifu);
    cycle();
    CHECK(dut->clk_core_ifu == 0, "clock should stop after the low phase latches 0, got %d",
          dut->clk_core_ifu);

    // ── Test 6: test_mode bypasses the gating ────────────────────────
    printf("Test 6: test_mode bypass\n");
    reset();
    dut->test_mode = 1;         // all active flags are 0
    cycle();
    CHECK(dut->clk_core_ifu == 1 && dut->clk_core_exu == 1 && dut->clk_core_lsu == 1 &&
          dut->clk_core_biu == 1 && dut->clk_itcm == 1 && dut->clk_dtcm == 1,
          "test_mode must force every gated clock to follow clk");
    dut->test_mode = 0;
    cycle();
    CHECK(dut->clk_core_ifu == 0, "clocks should gate again once test_mode drops, got %d",
          dut->clk_core_ifu);

    // ── Test 7: TCM light-sleep equations ────────────────────────────
    printf("Test 7: Light-sleep\n");
    reset();
    dut->core_wfi = 0;
    dut->eval();
    CHECK(dut->itcm_ls == 0 && dut->dtcm_ls == 0, "no light-sleep without WFI");
    dut->core_wfi = 1;
    dut->eval();
    CHECK(dut->itcm_ls == 1, "itcm_ls should assert when inactive and WFI, got %d", dut->itcm_ls);
    CHECK(dut->dtcm_ls == 1, "dtcm_ls should assert when inactive and WFI, got %d", dut->dtcm_ls);
    dut->itcm_active = 1;
    dut->eval();
    CHECK(dut->itcm_ls == 0, "itcm_ls must clear while the ITCM is active, got %d", dut->itcm_ls);
    CHECK(dut->dtcm_ls == 1, "dtcm_ls should stay asserted (DTCM inactive), got %d", dut->dtcm_ls);

    printf("\n=== e203_clk_ctrl: %d failure(s) ===\n", fail_count);
    return fail_count ? 1 : 0;
}
