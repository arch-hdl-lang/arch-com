// ARCH sim testbench for e203_ifu_litebpu — E203 static branch predictor.
// Tests: reset state, JAL/JALR always-taken, Bxx backward-taken/forward-
// not-taken, PC-adder operand selection (pc / 0 / x1 / rs1), JALR x1
// dependency stall, JALR xN dependency stall + IR-clear override, and the
// two-phase xN regfile read (issue cycle asserts bpu2rf_rs1_ena + bpu_wait,
// next cycle the pending flag suppresses re-issue).
//
// NOTE: this replaces a stale tb (VLiteBpu.h) that targeted an earlier,
// simplified revision of this fixture. The fixture was rewritten against the
// real E203 RTL and renamed to `e203_ifu_litebpu`; the old tb has not
// compiled since. The `_vltor_tb.cpp` Verilator twin was deleted at the same
// time (no harness path runs vltor TBs) and its coverage folds in here.
//
// Run with:
//   arch sim tests/e203/e203_ifu_litebpu.arch --tb tests/e203/e203_ifu_litebpu_tb.cpp

#include "Ve203_ifu_litebpu.h"
#include <cstdio>
#include <cstdint>

static int fail_count = 0;

#define CHECK(cond, fmt, ...) do { \
    if (!(cond)) { \
        printf("  FAIL: " fmt "\n", ##__VA_ARGS__); \
        fail_count++; \
    } \
} while(0)

static Ve203_ifu_litebpu* dut;

static void tick() {
    dut->clk = 0; dut->eval();
    dut->clk = 1; dut->eval();
}

// Drive every input to a benign idle value and pulse reset.
static void reset() {
    dut->rst_n = 0;
    dut->pc = 0x1000;
    dut->dec_jal = 0;
    dut->dec_jalr = 0;
    dut->dec_bxx = 0;
    dut->dec_bjp_imm = 0;
    dut->dec_jalr_rs1idx = 0;
    dut->oitf_empty = 1;
    dut->ir_empty = 1;
    dut->ir_rs1en = 0;
    dut->jalr_rs1idx_cam_irrdidx = 0;
    dut->dec_i_valid = 0;
    dut->ir_valid_clr = 0;
    dut->rf2bpu_x1 = 0;
    dut->rf2bpu_rs1 = 0;
    for (int i = 0; i < 3; i++) tick();
    dut->rst_n = 1;
    dut->eval();
}

int main() {
    dut = new Ve203_ifu_litebpu;

    // ── Test 1: Reset / idle state ───────────────────────────────────
    printf("Test 1: Reset state\n");
    reset();
    CHECK(dut->prdt_taken == 0, "prdt_taken should be 0 with no branch, got %d", dut->prdt_taken);
    CHECK(dut->bpu_wait == 0, "bpu_wait should be 0 at idle, got %d", dut->bpu_wait);
    CHECK(dut->bpu2rf_rs1_ena == 0, "bpu2rf_rs1_ena should be 0 at idle, got %d", dut->bpu2rf_rs1_ena);
    CHECK(dut->prdt_pc_add_op1 == 0x1000, "op1 should be pc (0x1000) with no jalr, got 0x%08x",
          dut->prdt_pc_add_op1);
    CHECK(dut->prdt_pc_add_op2 == 0, "op2 should mirror dec_bjp_imm (0), got 0x%08x",
          dut->prdt_pc_add_op2);

    // ── Test 2: JAL is always predicted taken, op1 = pc ──────────────
    printf("Test 2: JAL taken\n");
    reset();
    dut->dec_i_valid = 1;
    dut->dec_jal = 1;
    dut->dec_bjp_imm = 0x100;           // jal pc+0x100 (forward)
    dut->eval();
    CHECK(dut->prdt_taken == 1, "JAL should be predicted taken, got %d", dut->prdt_taken);
    CHECK(dut->prdt_pc_add_op1 == 0x1000, "JAL op1 should be pc, got 0x%08x", dut->prdt_pc_add_op1);
    CHECK(dut->prdt_pc_add_op2 == 0x100, "JAL op2 should be the imm, got 0x%08x", dut->prdt_pc_add_op2);
    CHECK(dut->bpu_wait == 0, "JAL never waits, got %d", dut->bpu_wait);

    // ── Test 3: Bxx backward taken, forward not taken ────────────────
    printf("Test 3: Bxx static prediction\n");
    reset();
    dut->dec_i_valid = 1;
    dut->dec_bxx = 1;
    dut->dec_bjp_imm = 0xFFFFFF00u;     // -256: backward branch
    dut->eval();
    CHECK(dut->prdt_taken == 1, "backward Bxx should be taken, got %d", dut->prdt_taken);
    CHECK(dut->prdt_pc_add_op1 == 0x1000, "Bxx op1 should be pc, got 0x%08x", dut->prdt_pc_add_op1);
    CHECK(dut->prdt_pc_add_op2 == 0xFFFFFF00u, "Bxx op2 should be the imm, got 0x%08x",
          dut->prdt_pc_add_op2);
    dut->dec_bjp_imm = 0x40;            // +64: forward branch
    dut->eval();
    CHECK(dut->prdt_taken == 0, "forward Bxx should not be taken, got %d", dut->prdt_taken);
    CHECK(dut->bpu_wait == 0, "Bxx never waits, got %d", dut->bpu_wait);

    // ── Test 4: JALR rs1=x0 — taken, op1 = 0, no wait ────────────────
    printf("Test 4: JALR x0\n");
    reset();
    dut->dec_i_valid = 1;
    dut->dec_jalr = 1;
    dut->dec_jalr_rs1idx = 0;
    dut->dec_bjp_imm = 0x8;
    dut->oitf_empty = 0;                // even with OITF busy, x0 has no dep
    dut->eval();
    CHECK(dut->prdt_taken == 1, "JALR should be predicted taken, got %d", dut->prdt_taken);
    CHECK(dut->prdt_pc_add_op1 == 0, "JALR x0 op1 should be 0, got 0x%08x", dut->prdt_pc_add_op1);
    CHECK(dut->bpu_wait == 0, "JALR x0 never waits, got %d", dut->bpu_wait);
    CHECK(dut->bpu2rf_rs1_ena == 0, "JALR x0 needs no regfile read, got %d", dut->bpu2rf_rs1_ena);

    // ── Test 5: JALR rs1=x1 — op1 = rf2bpu_x1 when no dependency ─────
    printf("Test 5: JALR x1 clean\n");
    reset();
    dut->dec_i_valid = 1;
    dut->dec_jalr = 1;
    dut->dec_jalr_rs1idx = 1;
    dut->rf2bpu_x1 = 0xCAFE0000u;
    dut->eval();                        // oitf_empty=1, cam=0 -> no dep
    CHECK(dut->prdt_taken == 1, "JALR x1 should be taken, got %d", dut->prdt_taken);
    CHECK(dut->prdt_pc_add_op1 == 0xCAFE0000u, "JALR x1 op1 should be rf2bpu_x1, got 0x%08x",
          dut->prdt_pc_add_op1);
    CHECK(dut->bpu_wait == 0, "clean JALR x1 should not wait, got %d", dut->bpu_wait);

    // ── Test 6: JALR x1 dependency stalls ────────────────────────────
    printf("Test 6: JALR x1 dependency\n");
    // (a) OITF not empty -> x1 may be written by an outstanding long-pipe op.
    dut->oitf_empty = 0;
    dut->eval();
    CHECK(dut->bpu_wait == 1, "JALR x1 with OITF busy should wait, got %d", dut->bpu_wait);
    dut->oitf_empty = 1;
    // (b) IR destination CAM hit on x1.
    dut->jalr_rs1idx_cam_irrdidx = 1;
    dut->eval();
    CHECK(dut->bpu_wait == 1, "JALR x1 with IR rd==x1 should wait, got %d", dut->bpu_wait);
    // (c) dec_i_valid low masks the dependency.
    dut->dec_i_valid = 0;
    dut->eval();
    CHECK(dut->bpu_wait == 0, "dep should be masked when dec_i_valid=0, got %d", dut->bpu_wait);

    // ── Test 7: JALR rs1=xN two-phase regfile read ───────────────────
    printf("Test 7: JALR xN read issue\n");
    reset();
    dut->dec_i_valid = 1;
    dut->dec_jalr = 1;
    dut->dec_jalr_rs1idx = 5;
    dut->rf2bpu_rs1 = 0xBEEF0000u;
    dut->eval();                        // oitf_empty=1, ir_empty=1 -> no dep
    CHECK(dut->prdt_pc_add_op1 == 0xBEEF0000u, "JALR xN op1 should be rf2bpu_rs1, got 0x%08x",
          dut->prdt_pc_add_op1);
    // Issue cycle: read enable + wait both assert.
    CHECK(dut->bpu2rf_rs1_ena == 1, "xN read should issue (rs1_ena), got %d", dut->bpu2rf_rs1_ena);
    CHECK(dut->bpu_wait == 1, "issue cycle should assert bpu_wait, got %d", dut->bpu_wait);
    tick();                             // rs1xn_rdrf_r loads 1
    dut->eval();
    // Pending flag suppresses re-issue; no dep remains, so wait drops.
    CHECK(dut->bpu2rf_rs1_ena == 0, "read must not re-issue while pending, got %d", dut->bpu2rf_rs1_ena);
    CHECK(dut->bpu_wait == 0, "wait should clear once the read is pending, got %d", dut->bpu_wait);
    tick();                             // rs1xn_rdrf_set=0 -> flag self-clears
    dut->eval();
    CHECK(dut->bpu2rf_rs1_ena == 1, "flag self-clears after one cycle, read re-issues, got %d",
          dut->bpu2rf_rs1_ena);

    // ── Test 8: JALR xN dependency blocks the read ───────────────────
    printf("Test 8: JALR xN dependency\n");
    reset();
    dut->dec_i_valid = 1;
    dut->dec_jalr = 1;
    dut->dec_jalr_rs1idx = 7;
    dut->oitf_empty = 0;                // OITF busy -> dep, and no IR-clear override
    dut->eval();
    CHECK(dut->bpu_wait == 1, "xN dep (OITF busy) should wait, got %d", dut->bpu_wait);
    CHECK(dut->bpu2rf_rs1_ena == 0, "xN dep must block the regfile read, got %d", dut->bpu2rf_rs1_ena);

    // IR occupied and using rs1, not clearing: still blocked.
    dut->oitf_empty = 1;
    dut->ir_empty = 0;
    dut->ir_rs1en = 1;
    dut->ir_valid_clr = 0;
    dut->eval();
    CHECK(dut->bpu_wait == 1, "xN dep (IR busy, rs1 in use) should wait, got %d", dut->bpu_wait);
    CHECK(dut->bpu2rf_rs1_ena == 0, "read must stay blocked, got %d", dut->bpu2rf_rs1_ena);

    // ── Test 9: IR-clear override lets the read issue under an IR dep ─
    printf("Test 9: xN IR-clear override\n");
    // Same dep, but IR is clearing this cycle -> override fires, read issues.
    dut->ir_valid_clr = 1;
    dut->eval();
    CHECK(dut->bpu2rf_rs1_ena == 1, "read should issue when IR is clearing, got %d",
          dut->bpu2rf_rs1_ena);
    CHECK(dut->bpu_wait == 1, "issue cycle still waits, got %d", dut->bpu_wait);
    // Override also fires when the IR entry does not read rs1 at all.
    dut->ir_valid_clr = 0;
    dut->ir_rs1en = 0;
    dut->eval();
    CHECK(dut->bpu2rf_rs1_ena == 1, "read should issue when IR has no rs1 use, got %d",
          dut->bpu2rf_rs1_ena);

    printf("\n=== e203_ifu_litebpu: %d failure(s) ===\n", fail_count);
    return fail_count ? 1 : 0;
}
