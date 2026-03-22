// Testbench for E203 ExuCommit — arch sim
#include "VExuCommit.h"
#include <cstdio>
#include <cstdlib>

static int errors = 0;
static int test_num = 0;

#define CHECK(cond, ...) do { \
    test_num++; \
    if (!(cond)) { errors++; printf("FAIL test %d: ", test_num); printf(__VA_ARGS__); printf("\n"); } \
    else { printf("PASS test %d\n", test_num); } \
} while(0)

int main() {
    VExuCommit m;
    // Reset defaults
    m.clk = 0; m.rst = 1;
    m.alu_valid = 0; m.alu_wdat = 0; m.alu_rd_idx = 0; m.alu_rd_en = 0;
    m.long_valid = 0; m.long_wdat = 0; m.long_rd_idx = 0; m.long_rd_en = 0;
    m.wbck_ready = 0;
    m.eval();

    // ── Test 1: No inputs → no output ──────────────────────────
    m.rst = 0;
    m.eval();
    CHECK(m.wbck_valid == 0, "idle: wbck_valid should be 0");
    CHECK(m.commit_valid == 0, "idle: commit_valid should be 0");
    CHECK(m.commit_src == 0, "idle: commit_src should be 0");
    CHECK(m.alu_ready == 0, "idle: alu_ready should be 0");
    CHECK(m.long_ready == 0, "idle: long_ready should be 0");

    // ── Test 2: ALU only, wbck not ready ───────────────────────
    m.alu_valid = 1; m.alu_wdat = 0xDEAD; m.alu_rd_idx = 5; m.alu_rd_en = 1;
    m.wbck_ready = 0;
    m.eval();
    CHECK(m.wbck_valid == 1, "alu only: wbck_valid should be 1");
    CHECK(m.wbck_wdat == 0xDEAD, "alu only: wbck_wdat should be 0xDEAD, got 0x%X", m.wbck_wdat);
    CHECK(m.wbck_rd_idx == 5, "alu only: wbck_rd_idx should be 5, got %d", m.wbck_rd_idx);
    CHECK(m.wbck_rd_en == 1, "alu only: wbck_rd_en should be 1");
    CHECK(m.alu_ready == 0, "alu only, not ready: alu_ready should be 0");
    CHECK(m.commit_valid == 0, "alu only, not ready: commit_valid should be 0");
    CHECK(m.commit_src == 1, "alu only: commit_src should be 1 (alu), got %d", m.commit_src);

    // ── Test 3: ALU only, wbck ready ──────────────────────────
    m.wbck_ready = 1;
    m.eval();
    CHECK(m.alu_ready == 1, "alu ready: alu_ready should be 1");
    CHECK(m.long_ready == 0, "alu ready: long_ready should be 0");
    CHECK(m.commit_valid == 1, "alu ready: commit_valid should be 1");
    CHECK(m.commit_src == 1, "alu ready: commit_src should be 1");

    // ── Test 4: Long-pipe only ────────────────────────────────
    m.alu_valid = 0;
    m.long_valid = 1; m.long_wdat = 0xBEEF; m.long_rd_idx = 10; m.long_rd_en = 1;
    m.wbck_ready = 1;
    m.eval();
    CHECK(m.wbck_valid == 1, "long only: wbck_valid should be 1");
    CHECK(m.wbck_wdat == 0xBEEF, "long only: wbck_wdat should be 0xBEEF, got 0x%X", m.wbck_wdat);
    CHECK(m.wbck_rd_idx == 10, "long only: wbck_rd_idx should be 10, got %d", m.wbck_rd_idx);
    CHECK(m.wbck_rd_en == 1, "long only: wbck_rd_en should be 1");
    CHECK(m.long_ready == 1, "long only: long_ready should be 1");
    CHECK(m.alu_ready == 0, "long only: alu_ready should be 0");
    CHECK(m.commit_src == 2, "long only: commit_src should be 2 (long), got %d", m.commit_src);

    // ── Test 5: Both valid — ALU wins ─────────────────────────
    m.alu_valid = 1; m.alu_wdat = 0x1111; m.alu_rd_idx = 3; m.alu_rd_en = 1;
    m.long_valid = 1; m.long_wdat = 0x2222; m.long_rd_idx = 7; m.long_rd_en = 1;
    m.wbck_ready = 1;
    m.eval();
    CHECK(m.wbck_wdat == 0x1111, "both: ALU wins, wdat=0x1111, got 0x%X", m.wbck_wdat);
    CHECK(m.wbck_rd_idx == 3, "both: ALU wins, rd_idx=3, got %d", m.wbck_rd_idx);
    CHECK(m.alu_ready == 1, "both: alu_ready should be 1 (winner)");
    CHECK(m.long_ready == 0, "both: long_ready should be 0 (loser)");
    CHECK(m.commit_src == 1, "both: commit_src should be 1 (alu), got %d", m.commit_src);

    // ── Test 6: Both valid, wbck not ready ────────────────────
    m.wbck_ready = 0;
    m.eval();
    CHECK(m.alu_ready == 0, "both not ready: alu_ready should be 0");
    CHECK(m.long_ready == 0, "both not ready: long_ready should be 0");
    CHECK(m.commit_valid == 0, "both not ready: commit_valid should be 0");
    CHECK(m.wbck_valid == 1, "both not ready: wbck_valid still 1");

    // ── Test 7: Long pipe, rd_en=0 (no writeback) ────────────
    m.alu_valid = 0;
    m.long_valid = 1; m.long_wdat = 0xCAFE; m.long_rd_idx = 0; m.long_rd_en = 0;
    m.wbck_ready = 1;
    m.eval();
    CHECK(m.wbck_rd_en == 0, "long no-wb: wbck_rd_en should be 0");
    CHECK(m.wbck_wdat == 0xCAFE, "long no-wb: wbck_wdat=0xCAFE, got 0x%X", m.wbck_wdat);
    CHECK(m.commit_valid == 1, "long no-wb: commit_valid should be 1");

    // ── Test 8: ALU with rd_en=0 ─────────────────────────────
    m.alu_valid = 1; m.alu_wdat = 0xFACE; m.alu_rd_idx = 15; m.alu_rd_en = 0;
    m.long_valid = 0;
    m.wbck_ready = 1;
    m.eval();
    CHECK(m.wbck_rd_en == 0, "alu no-wb: wbck_rd_en should be 0");
    CHECK(m.wbck_wdat == 0xFACE, "alu no-wb: wbck_wdat=0xFACE, got 0x%X", m.wbck_wdat);
    CHECK(m.wbck_rd_idx == 15, "alu no-wb: wbck_rd_idx=15, got %d", m.wbck_rd_idx);

    // ── Summary ───────────────────────────────────────────────
    printf("\n=== ExuCommit: %d tests, %d errors ===\n", test_num, errors);
    return errors ? 1 : 0;
}
