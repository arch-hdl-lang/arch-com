// ARCH sim testbench for e203_irq_ctrl — combinational machine-mode interrupt
// controller (MEI/MTI/MSI). Tests: idle state, mip_* mirroring of the raw
// sources independent of enables, per-source mie gating, global mstatus_mie
// gating, pipeline gating (pipe_flush_ack | commit_valid), the
// MEI > MSI > MTI priority order, and mcause encodings 0x8000000B / 0x80000003
// / 0x80000007.
//
// NOTE: this replaces a stale tb (VIrqCtrl.h) that targeted the pre-2026-04
// PascalCase fixture naming. The construct is now `e203_irq_ctrl`, so the sim
// class is Ve203_irq_ctrl.
//
// Run with:
//   arch sim tests/e203/e203_irq_ctrl.arch --tb tests/e203/e203_irq_ctrl_tb.cpp

#include "Ve203_irq_ctrl.h"
#include <cstdio>
#include <cstdint>

static int fail_count = 0;

#define CHECK(cond, fmt, ...) do { \
    if (!(cond)) { \
        printf("  FAIL: " fmt "\n", ##__VA_ARGS__); \
        fail_count++; \
    } \
} while(0)

static Ve203_irq_ctrl* dut;

static void clear_inputs() {
    dut->clk = 0;
    dut->rst_n = 1;
    dut->ext_irq_i = 0;
    dut->sw_irq_i = 0;
    dut->tmr_irq_i = 0;
    dut->mstatus_mie = 1;
    dut->mie_meie = 1;
    dut->mie_mtie = 1;
    dut->mie_msie = 1;
    dut->pipe_flush_ack = 1;
    dut->commit_valid = 0;
    dut->eval();
}

int main() {
    dut = new Ve203_irq_ctrl;

    // ── Test 1: Idle — no sources, no request ────────────────────────
    printf("Test 1: Idle\n");
    clear_inputs();
    CHECK(dut->irq_req == 0, "irq_req should be 0 with no sources, got %d", dut->irq_req);
    CHECK(dut->irq_cause == 0, "irq_cause should be 0 with no sources, got 0x%08x", dut->irq_cause);
    CHECK(dut->mip_meip == 0 && dut->mip_mtip == 0 && dut->mip_msip == 0,
          "all mip bits should be 0 with no sources");

    // ── Test 2: mip mirrors raw sources regardless of enables ────────
    printf("Test 2: mip mirroring\n");
    clear_inputs();
    dut->mie_meie = 0; dut->mie_mtie = 0; dut->mie_msie = 0;
    dut->mstatus_mie = 0;
    dut->ext_irq_i = 1; dut->tmr_irq_i = 1; dut->sw_irq_i = 1;
    dut->eval();
    CHECK(dut->mip_meip == 1, "mip_meip should mirror ext_irq_i even when disabled, got %d",
          dut->mip_meip);
    CHECK(dut->mip_mtip == 1, "mip_mtip should mirror tmr_irq_i even when disabled, got %d",
          dut->mip_mtip);
    CHECK(dut->mip_msip == 1, "mip_msip should mirror sw_irq_i even when disabled, got %d",
          dut->mip_msip);
    CHECK(dut->irq_req == 0, "irq_req must stay 0 with all enables off, got %d", dut->irq_req);

    // ── Test 3: External interrupt with full enables ─────────────────
    printf("Test 3: MEI request\n");
    clear_inputs();
    dut->ext_irq_i = 1;
    dut->eval();
    CHECK(dut->irq_req == 1, "irq_req should assert for enabled MEI, got %d", dut->irq_req);
    CHECK(dut->irq_cause == 0x8000000Bu, "MEI cause should be 0x8000000B, got 0x%08x",
          dut->irq_cause);
    // Per-source enable gating.
    dut->mie_meie = 0;
    dut->eval();
    CHECK(dut->irq_req == 0, "irq_req should drop when mie_meie=0, got %d", dut->irq_req);

    // ── Test 4: Global mstatus_mie gating ────────────────────────────
    printf("Test 4: Global enable\n");
    clear_inputs();
    dut->tmr_irq_i = 1;
    dut->eval();
    CHECK(dut->irq_req == 1, "irq_req should assert for enabled MTI, got %d", dut->irq_req);
    dut->mstatus_mie = 0;
    dut->eval();
    CHECK(dut->irq_req == 0, "irq_req should drop when mstatus_mie=0, got %d", dut->irq_req);

    // ── Test 5: Pipeline gating ──────────────────────────────────────
    printf("Test 5: Pipeline gating\n");
    clear_inputs();
    dut->sw_irq_i = 1;
    dut->pipe_flush_ack = 0;
    dut->commit_valid = 0;
    dut->eval();
    CHECK(dut->irq_req == 0, "irq_req must wait for flush_ack or commit_valid, got %d",
          dut->irq_req);
    dut->commit_valid = 1;
    dut->eval();
    CHECK(dut->irq_req == 1, "irq_req should assert on commit_valid, got %d", dut->irq_req);
    dut->commit_valid = 0;
    dut->pipe_flush_ack = 1;
    dut->eval();
    CHECK(dut->irq_req == 1, "irq_req should assert on pipe_flush_ack, got %d", dut->irq_req);

    // ── Test 6: Priority MEI > MSI > MTI and cause encodings ─────────
    printf("Test 6: Priority and mcause\n");
    clear_inputs();
    // All three pending: MEI wins.
    dut->ext_irq_i = 1; dut->sw_irq_i = 1; dut->tmr_irq_i = 1;
    dut->eval();
    CHECK(dut->irq_cause == 0x8000000Bu, "MEI should win with all pending, got 0x%08x",
          dut->irq_cause);
    // MSI + MTI pending: MSI wins.
    dut->ext_irq_i = 0;
    dut->eval();
    CHECK(dut->irq_cause == 0x80000003u, "MSI should beat MTI, got 0x%08x", dut->irq_cause);
    // MTI only.
    dut->sw_irq_i = 0;
    dut->eval();
    CHECK(dut->irq_cause == 0x80000007u, "MTI cause should be 0x80000007, got 0x%08x",
          dut->irq_cause);
    CHECK(dut->irq_req == 1, "irq_req should still assert for MTI alone, got %d", dut->irq_req);
    // Priority selection must respect enables, not just raw sources: with MEI
    // pending but disabled, the enabled MSI is selected.
    dut->ext_irq_i = 1; dut->sw_irq_i = 1;
    dut->mie_meie = 0;
    dut->eval();
    CHECK(dut->irq_cause == 0x80000003u, "disabled MEI must not mask MSI's cause, got 0x%08x",
          dut->irq_cause);

    printf("\n=== e203_irq_ctrl: %d failure(s) ===\n", fail_count);
    return fail_count ? 1 : 0;
}
