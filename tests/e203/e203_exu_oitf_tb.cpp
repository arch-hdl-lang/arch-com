// Testbench for E203 ExuOitf — arch sim
#include "VExuOitf.h"
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

static void tick(VExuOitf &m) {
    m.clk = 0; m.eval();
    m.clk = 1; m.eval();
}

static void reset(VExuOitf &m) {
    m.clk = 0; m.rst_n = 0;
    m.dis_ena = 0; m.dis_rd_idx = 0; m.dis_rd_en = 0;
    m.ret_ena = 0;
    m.chk_rs1_idx = 0; m.chk_rs1_en = 0;
    m.chk_rs2_idx = 0; m.chk_rs2_en = 0;
    m.chk_rd_idx = 0;  m.chk_rd_en = 0;
    m.eval();
    tick(m); tick(m);
    m.rst_n = 1;
    tick(m);
}

int main() {
    VExuOitf m;
    reset(m);

    // ── Test 1: After reset, FIFO empty, ready ──────────────────────
    m.eval();
    CHECK(BOOL(m.oitf_empty), "reset: empty=%d", m.oitf_empty);
    CHECK(BOOL(m.dis_ready),  "reset: ready=%d", m.dis_ready);
    CHECK(!BOOL(m.raw_dep),   "reset: raw_dep=%d", m.raw_dep);
    CHECK(!BOOL(m.waw_dep),   "reset: waw_dep=%d", m.waw_dep);
    CHECK(!BOOL(m.dep_stall), "reset: dep_stall=%d", m.dep_stall);

    // ── Test 6: Allocate entry 0 (rd=x5, rd_en=1) ──────────────────
    m.dis_ena = 1; m.dis_rd_idx = 5; m.dis_rd_en = 1;
    tick(m);
    m.dis_ena = 0;
    m.eval();
    CHECK(!BOOL(m.oitf_empty), "alloc0: empty=%d", m.oitf_empty);
    CHECK(BOOL(m.dis_ready),   "alloc0: ready=%d (1 slot used)", m.dis_ready);

    // ── Test 8: RAW check — rs1=x5 should hit ──────────────────────
    m.chk_rs1_en = 1; m.chk_rs1_idx = 5;
    m.chk_rs2_en = 0; m.chk_rd_en = 0;
    m.eval();
    CHECK(BOOL(m.raw_dep),    "raw rs1=x5: raw_dep=%d", m.raw_dep);
    CHECK(BOOL(m.dep_stall),  "raw rs1=x5: stall=%d", m.dep_stall);

    // ── Test 10: RAW check — rs1=x3 should NOT hit ─────────────────
    m.chk_rs1_idx = 3;
    m.eval();
    CHECK(!BOOL(m.raw_dep),   "raw rs1=x3: raw_dep=%d", m.raw_dep);

    // ── Test 11: RAW check — rs2=x5 should hit ─────────────────────
    m.chk_rs1_en = 0; m.chk_rs2_en = 1; m.chk_rs2_idx = 5;
    m.eval();
    CHECK(BOOL(m.raw_dep),    "raw rs2=x5: raw_dep=%d", m.raw_dep);

    // ── Test 12: WAW check — rd=x5 should hit ──────────────────────
    m.chk_rs2_en = 0; m.chk_rd_en = 1; m.chk_rd_idx = 5;
    m.eval();
    CHECK(BOOL(m.waw_dep),    "waw rd=x5: waw_dep=%d", m.waw_dep);

    // ── Test 13: WAW check — rd=x7 should NOT hit ──────────────────
    m.chk_rd_idx = 7;
    m.eval();
    CHECK(!BOOL(m.waw_dep),   "waw rd=x7: waw_dep=%d", m.waw_dep);

    // ── Test 14: Allocate entry 1 (rd=x10, rd_en=1) — FIFO full ────
    m.chk_rs1_en = 0; m.chk_rs2_en = 0; m.chk_rd_en = 0;
    m.dis_ena = 1; m.dis_rd_idx = 10; m.dis_rd_en = 1;
    tick(m);
    m.dis_ena = 0;
    m.eval();
    CHECK(!BOOL(m.dis_ready), "alloc1: ready=%d (full)", m.dis_ready);
    CHECK(!BOOL(m.oitf_empty),"alloc1: empty=%d", m.oitf_empty);

    // ── Test 16: RAW hits both entries — rs1=x5, rs2=x10 ───────────
    m.chk_rs1_en = 1; m.chk_rs1_idx = 5;
    m.chk_rs2_en = 1; m.chk_rs2_idx = 10;
    m.chk_rd_en = 0;
    m.eval();
    CHECK(BOOL(m.raw_dep),    "raw both: raw_dep=%d", m.raw_dep);

    // ── Test 17: Deallocate entry 0 (oldest) ────────────────────────
    m.chk_rs1_en = 0; m.chk_rs2_en = 0;
    CHECK(m.ret_rd_idx == 5,  "ret0: rd_idx=%d (exp 5)", m.ret_rd_idx);
    CHECK(BOOL(m.ret_rd_en),  "ret0: rd_en=%d", m.ret_rd_en);
    m.ret_ena = 1;
    tick(m);
    m.ret_ena = 0;
    m.eval();
    CHECK(BOOL(m.dis_ready),  "dealloc0: ready=%d (1 free)", m.dis_ready);

    // ── Test 20: rs1=x5 should no longer hit (entry 0 freed) ───────
    m.chk_rs1_en = 1; m.chk_rs1_idx = 5;
    m.eval();
    CHECK(!BOOL(m.raw_dep),   "post-dealloc: rs1=x5 raw_dep=%d", m.raw_dep);

    // ── Test 21: rs1=x10 still hits (entry 1 valid) ────────────────
    m.chk_rs1_idx = 10;
    m.eval();
    CHECK(BOOL(m.raw_dep),    "post-dealloc: rs1=x10 raw_dep=%d", m.raw_dep);

    // ── Test 22: Deallocate entry 1 — FIFO empty again ──────────────
    m.chk_rs1_en = 0;
    m.ret_ena = 1;
    tick(m);
    m.ret_ena = 0;
    m.eval();
    CHECK(BOOL(m.oitf_empty), "empty again: empty=%d", m.oitf_empty);
    CHECK(BOOL(m.dis_ready),  "empty again: ready=%d", m.dis_ready);

    // ── Test 24: Allocate with rd_en=0 — no hazards ─────────────────
    m.dis_ena = 1; m.dis_rd_idx = 15; m.dis_rd_en = 0;
    tick(m);
    m.dis_ena = 0;
    m.chk_rs1_en = 1; m.chk_rs1_idx = 15;
    m.chk_rd_en = 1; m.chk_rd_idx = 15;
    m.eval();
    CHECK(!BOOL(m.raw_dep),   "rd_en=0: raw_dep=%d", m.raw_dep);
    CHECK(!BOOL(m.waw_dep),   "rd_en=0: waw_dep=%d", m.waw_dep);

    // ── Test 26: Clean up, deallocate ───────────────────────────────
    m.chk_rs1_en = 0; m.chk_rd_en = 0;
    m.ret_ena = 1;
    tick(m);
    m.ret_ena = 0;
    m.eval();
    CHECK(BOOL(m.oitf_empty), "final empty: empty=%d", m.oitf_empty);

    // ── Test 27: Back-to-back alloc+dealloc same cycle ──────────────
    m.dis_ena = 1; m.dis_rd_idx = 20; m.dis_rd_en = 1;
    m.ret_ena = 0;
    tick(m);
    // Now alloc second and dealloc first simultaneously
    m.dis_ena = 1; m.dis_rd_idx = 21; m.dis_rd_en = 1;
    m.ret_ena = 1;
    tick(m);
    m.dis_ena = 0; m.ret_ena = 0;
    m.eval();
    // Should have 1 entry (alloc'd 2, dealloc'd 1)
    CHECK(!BOOL(m.oitf_empty), "b2b: not empty=%d", m.oitf_empty);
    CHECK(BOOL(m.dis_ready),   "b2b: ready=%d", m.dis_ready);

    // Check the remaining entry is rd=21
    m.chk_rs1_en = 1; m.chk_rs1_idx = 21;
    m.eval();
    CHECK(BOOL(m.raw_dep), "b2b: rs1=x21 raw=%d", m.raw_dep);

    // Clean up
    m.chk_rs1_en = 0;
    m.ret_ena = 1;
    tick(m);
    m.ret_ena = 0;
    m.eval();
    CHECK(BOOL(m.oitf_empty), "b2b cleanup: empty=%d", m.oitf_empty);

    printf("\n=== ExuOitf arch sim: %d tests, %d errors ===\n", test_num, errors);
    return errors ? 1 : 0;
}
