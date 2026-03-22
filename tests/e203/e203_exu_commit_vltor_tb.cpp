// Verilator testbench for E203 ExuCommit — cross-check against arch sim
#include "VExuCommit.h"
#include "verilated.h"
#include <cstdio>
#include <cstdlib>

static int errors = 0;
static int test_num = 0;

#define CHECK(cond, ...) do { \
    test_num++; \
    if (!(cond)) { errors++; printf("FAIL test %d: ", test_num); printf(__VA_ARGS__); printf("\n"); } \
    else { printf("PASS test %d\n", test_num); } \
} while(0)

int main(int argc, char** argv) {
    Verilated::commandArgs(argc, argv);
    VExuCommit* m = new VExuCommit;

    // Defaults
    m->clk = 0; m->rst = 1;
    m->alu_valid = 0; m->alu_wdat = 0; m->alu_rd_idx = 0; m->alu_rd_en = 0;
    m->long_valid = 0; m->long_wdat = 0; m->long_rd_idx = 0; m->long_rd_en = 0;
    m->wbck_ready = 0;
    m->eval();

    // Release reset
    m->rst = 0;
    m->eval();

    // ── Idle ──────────────────────────────────────────────────
    CHECK(m->wbck_valid == 0, "idle: wbck_valid");
    CHECK(m->commit_valid == 0, "idle: commit_valid");
    CHECK(m->commit_src == 0, "idle: commit_src");

    // ── ALU only, ready ───────────────────────────────────────
    m->alu_valid = 1; m->alu_wdat = 0xAABB; m->alu_rd_idx = 12; m->alu_rd_en = 1;
    m->wbck_ready = 1;
    m->eval();
    CHECK(m->wbck_valid == 1, "alu: wbck_valid");
    CHECK(m->wbck_wdat == 0xAABB, "alu: wdat=0x%X", m->wbck_wdat);
    CHECK(m->wbck_rd_idx == 12, "alu: rd_idx=%d", m->wbck_rd_idx);
    CHECK(m->alu_ready == 1, "alu: alu_ready");
    CHECK(m->long_ready == 0, "alu: long_ready");
    CHECK(m->commit_src == 1, "alu: src=%d", m->commit_src);

    // ── Long only ─────────────────────────────────────────────
    m->alu_valid = 0;
    m->long_valid = 1; m->long_wdat = 0xCCDD; m->long_rd_idx = 20; m->long_rd_en = 1;
    m->eval();
    CHECK(m->wbck_wdat == 0xCCDD, "long: wdat=0x%X", m->wbck_wdat);
    CHECK(m->wbck_rd_idx == 20, "long: rd_idx=%d", m->wbck_rd_idx);
    CHECK(m->long_ready == 1, "long: long_ready");
    CHECK(m->commit_src == 2, "long: src=%d", m->commit_src);

    // ── Both — ALU wins ───────────────────────────────────────
    m->alu_valid = 1; m->alu_wdat = 0x1234; m->alu_rd_idx = 1;
    m->long_valid = 1; m->long_wdat = 0x5678; m->long_rd_idx = 2;
    m->eval();
    CHECK(m->wbck_wdat == 0x1234, "both: ALU wins, wdat=0x%X", m->wbck_wdat);
    CHECK(m->wbck_rd_idx == 1, "both: rd_idx=%d", m->wbck_rd_idx);
    CHECK(m->alu_ready == 1, "both: alu_ready");
    CHECK(m->long_ready == 0, "both: long_ready blocked");

    // ── Both, wbck not ready ──────────────────────────────────
    m->wbck_ready = 0;
    m->eval();
    CHECK(m->alu_ready == 0, "stall: alu_ready");
    CHECK(m->long_ready == 0, "stall: long_ready");
    CHECK(m->commit_valid == 0, "stall: commit_valid");

    // ── Summary ───────────────────────────────────────────────
    printf("\n=== ExuCommit Verilator: %d tests, %d errors ===\n", test_num, errors);
    delete m;
    return errors ? 1 : 0;
}
