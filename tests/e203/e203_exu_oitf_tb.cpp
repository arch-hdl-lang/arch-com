// ARCH sim testbench for e203_exu_oitf — E203 outstanding-instruction FIFO.
// Tests: empty/ready reset state, allocate/retire pointer march through the
// 2-entry circular FIFO, full backpressure (dis_ready deasserts), FIFO-order
// retire info (rdidx/rdwen/rdfpu/pc of the OLDEST entry), RAW/WAW hazard
// match outputs against in-flight rd, FPU-tag disambiguation (integer x5 vs
// FPU f5 do not collide), rdwen gating (entries that don't write rd never
// raise a hazard), and simultaneous allocate+retire.
//
// NOTE: this replaces a stale tb (VExuOitf.h) that targeted an earlier
// revision of the fixture before the e203 fixtures were rewritten against the
// real E203 RTL, which renamed the construct to `e203_exu_oitf`. The old tb
// has not compiled since. Ported to the current class name (Ve203_exu_oitf).
//
// Run with:
//   arch sim tests/e203/e203_exu_oitf.arch --tb tests/e203/e203_exu_oitf_tb.cpp

#include "Ve203_exu_oitf.h"
#include <cstdio>
#include <cstdint>

static int fail_count = 0;

#define CHECK(cond, fmt, ...) do { \
    if (!(cond)) { \
        printf("  FAIL: " fmt "\n", ##__VA_ARGS__); \
        fail_count++; \
    } \
} while(0)

static Ve203_exu_oitf* dut;

static void tick() {
    dut->clk = 0; dut->eval();
    dut->clk = 1; dut->eval();
}

static void reset() {
    dut->rst_n = 0;
    dut->dis_ena = 0;
    dut->ret_ena = 0;
    dut->disp_i_rs1en = 0; dut->disp_i_rs2en = 0; dut->disp_i_rs3en = 0;
    dut->disp_i_rdwen = 0;
    dut->disp_i_rs1fpu = 0; dut->disp_i_rs2fpu = 0; dut->disp_i_rs3fpu = 0;
    dut->disp_i_rdfpu = 0;
    dut->disp_i_rs1idx = 0; dut->disp_i_rs2idx = 0; dut->disp_i_rs3idx = 0;
    dut->disp_i_rdidx = 0;
    dut->disp_i_pc = 0;
    for (int i = 0; i < 3; i++) tick();
    dut->rst_n = 1;
    dut->eval();
}

// Allocate one entry (assumes dis_ready).
static void alloc(uint8_t rdidx, uint8_t rdwen, uint8_t rdfpu, uint32_t pc) {
    dut->dis_ena = 1;
    dut->disp_i_rdidx = rdidx;
    dut->disp_i_rdwen = rdwen;
    dut->disp_i_rdfpu = rdfpu;
    dut->disp_i_pc = pc;
    dut->eval();
    tick();
    dut->dis_ena = 0;
    dut->disp_i_rdwen = 0;
    dut->eval();
}

// Retire the oldest entry.
static void retire() {
    dut->ret_ena = 1;
    dut->eval();
    tick();
    dut->ret_ena = 0;
    dut->eval();
}

int main() {
    dut = new Ve203_exu_oitf;

    // ── Test 1: Reset state ──────────────────────────────────────────
    printf("Test 1: Reset state\n");
    reset();
    CHECK(dut->oitf_empty == 1, "OITF should be empty after reset, got %d", dut->oitf_empty);
    CHECK(dut->dis_ready == 1, "dis_ready should be 1 after reset, got %d", dut->dis_ready);
    CHECK(dut->dis_ptr == 0, "dis_ptr should be 0 after reset, got %d", dut->dis_ptr);
    CHECK(dut->ret_ptr == 0, "ret_ptr should be 0 after reset, got %d", dut->ret_ptr);
    CHECK(dut->oitfrd_match_disprs1 == 0, "no hazard when empty, got %d", dut->oitfrd_match_disprs1);

    // ── Test 2: Allocate to full, retire to empty ────────────────────
    printf("Test 2: Fill and drain\n");
    reset();
    alloc(5, 1, 0, 0x100);           // entry 0: long-op writing x5
    CHECK(dut->oitf_empty == 0, "not empty after one alloc, got %d", dut->oitf_empty);
    CHECK(dut->dis_ready == 1, "one free slot remains, got %d", dut->dis_ready);
    CHECK(dut->dis_ptr == 1, "dis_ptr should advance to 1, got %d", dut->dis_ptr);
    alloc(9, 1, 0, 0x104);           // entry 1: long-op writing x9
    CHECK(dut->dis_ready == 0, "FIFO full: dis_ready should drop, got %d", dut->dis_ready);
    CHECK(dut->oitf_empty == 0, "full is not empty, got %d", dut->oitf_empty);
    CHECK(dut->dis_ptr == 0, "dis_ptr should wrap to 0, got %d", dut->dis_ptr);
    // Oldest entry (x5, pc 0x100) is presented for retire.
    CHECK(dut->ret_ptr == 0, "ret_ptr should be 0, got %d", dut->ret_ptr);
    CHECK(dut->ret_rdidx == 5, "oldest rdidx should be 5, got %d", dut->ret_rdidx);
    CHECK(dut->ret_rdwen == 1, "oldest rdwen should be 1, got %d", dut->ret_rdwen);
    CHECK(dut->ret_pc == 0x100, "oldest pc should be 0x100, got 0x%08x", dut->ret_pc);
    retire();
    CHECK(dut->dis_ready == 1, "one slot frees after retire, got %d", dut->dis_ready);
    CHECK(dut->ret_ptr == 1, "ret_ptr should advance to 1, got %d", dut->ret_ptr);
    CHECK(dut->ret_rdidx == 9, "next-oldest rdidx should be 9, got %d", dut->ret_rdidx);
    CHECK(dut->ret_pc == 0x104, "next-oldest pc should be 0x104, got 0x%08x", dut->ret_pc);
    retire();
    CHECK(dut->oitf_empty == 1, "empty after draining both, got %d", dut->oitf_empty);
    CHECK(dut->dis_ready == 1, "dis_ready restored, got %d", dut->dis_ready);

    // ── Test 3: RAW/WAW hazard matches ───────────────────────────────
    printf("Test 3: Hazard matches\n");
    reset();
    alloc(7, 1, 0, 0x200);           // in-flight op writes x7
    // RAW on rs1 = x7.
    dut->disp_i_rs1en = 1; dut->disp_i_rs1idx = 7;
    dut->eval();
    CHECK(dut->oitfrd_match_disprs1 == 1, "rs1=x7 should match in-flight rd x7, got %d",
          dut->oitfrd_match_disprs1);
    // Different index: no match.
    dut->disp_i_rs1idx = 8;
    dut->eval();
    CHECK(dut->oitfrd_match_disprs1 == 0, "rs1=x8 must not match rd x7, got %d",
          dut->oitfrd_match_disprs1);
    // rs1en gate.
    dut->disp_i_rs1idx = 7; dut->disp_i_rs1en = 0;
    dut->eval();
    CHECK(dut->oitfrd_match_disprs1 == 0, "rs1en=0 must suppress the match, got %d",
          dut->oitfrd_match_disprs1);
    dut->disp_i_rs1en = 1;
    // rs2 and rs3 channels.
    dut->disp_i_rs2en = 1; dut->disp_i_rs2idx = 7;
    dut->disp_i_rs3en = 1; dut->disp_i_rs3idx = 6;
    dut->eval();
    CHECK(dut->oitfrd_match_disprs2 == 1, "rs2=x7 should match, got %d", dut->oitfrd_match_disprs2);
    CHECK(dut->oitfrd_match_disprs3 == 0, "rs3=x6 must not match, got %d", dut->oitfrd_match_disprs3);
    // WAW: new op also writes x7.
    dut->disp_i_rdwen = 1; dut->disp_i_rdidx = 7;
    dut->eval();
    CHECK(dut->oitfrd_match_disprd == 1, "rd=x7 WAW should match, got %d", dut->oitfrd_match_disprd);
    dut->disp_i_rdidx = 3;
    dut->eval();
    CHECK(dut->oitfrd_match_disprd == 0, "rd=x3 must not match, got %d", dut->oitfrd_match_disprd);
    // Hazard clears when the in-flight op retires.
    dut->disp_i_rs1idx = 7;
    retire();
    dut->eval();
    CHECK(dut->oitfrd_match_disprs1 == 0, "hazard must clear after retire, got %d",
          dut->oitfrd_match_disprs1);

    // ── Test 4: FPU tag disambiguation ───────────────────────────────
    printf("Test 4: FPU tag disambiguation\n");
    reset();
    alloc(5, 1, 1, 0x300);           // in-flight op writes FPU f5
    dut->disp_i_rs1en = 1; dut->disp_i_rs1idx = 5; dut->disp_i_rs1fpu = 0;
    dut->eval();
    CHECK(dut->oitfrd_match_disprs1 == 0, "integer x5 must not match FPU f5, got %d",
          dut->oitfrd_match_disprs1);
    dut->disp_i_rs1fpu = 1;
    dut->eval();
    CHECK(dut->oitfrd_match_disprs1 == 1, "FPU f5 should match FPU f5, got %d",
          dut->oitfrd_match_disprs1);
    CHECK(dut->ret_rdfpu == 1, "ret_rdfpu should report the FPU tag, got %d", dut->ret_rdfpu);

    // ── Test 5: rdwen gating of hazards ──────────────────────────────
    printf("Test 5: rdwen gating\n");
    reset();
    alloc(4, 0, 0, 0x400);           // in-flight op does NOT write rd (e.g. store)
    dut->disp_i_rs1en = 1; dut->disp_i_rs1idx = 4; dut->disp_i_rs1fpu = 0;
    dut->eval();
    CHECK(dut->oitfrd_match_disprs1 == 0, "non-writing entry must never raise a hazard, got %d",
          dut->oitfrd_match_disprs1);
    CHECK(dut->ret_rdwen == 0, "ret_rdwen should be 0 for the non-writing entry, got %d", dut->ret_rdwen);

    // ── Test 6: Simultaneous allocate + retire at full ───────────────
    printf("Test 6: Concurrent alloc+retire\n");
    reset();
    alloc(1, 1, 0, 0x500);
    alloc(2, 1, 0, 0x504);
    CHECK(dut->dis_ready == 0, "full before concurrent op, got %d", dut->dis_ready);
    // Retire the oldest while... dis_ready is low, so dis_ena&dis_ready won't
    // allocate. Verify retire-only takes effect and frees a slot.
    dut->ret_ena = 1;
    dut->dis_ena = 1;                // blocked by dis_ready==0
    dut->disp_i_rdidx = 3; dut->disp_i_rdwen = 1; dut->disp_i_pc = 0x508;
    dut->eval();
    tick();
    dut->ret_ena = 0;
    dut->eval();
    CHECK(dut->dis_ready == 1, "slot frees after retire, got %d", dut->dis_ready);
    CHECK(dut->ret_rdidx == 2, "oldest is now x2, got %d", dut->ret_rdidx);
    // Now dis_ready is high: the pending alloc goes through together with a retire.
    dut->ret_ena = 1;
    dut->eval();
    tick();
    dut->dis_ena = 0;
    dut->ret_ena = 0;
    dut->eval();
    CHECK(dut->oitf_empty == 0, "alloc+retire keeps one entry in flight, got %d", dut->oitf_empty);
    CHECK(dut->ret_rdidx == 3, "remaining entry is x3, got %d", dut->ret_rdidx);
    CHECK(dut->ret_pc == 0x508, "remaining pc is 0x508, got 0x%08x", dut->ret_pc);
    retire();
    CHECK(dut->oitf_empty == 1, "empty after final retire, got %d", dut->oitf_empty);

    printf("\n=== e203_exu_oitf: %d failure(s) ===\n", fail_count);
    return fail_count ? 1 : 0;
}
