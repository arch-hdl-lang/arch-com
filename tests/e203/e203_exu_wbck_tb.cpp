// ARCH sim testbench for e203_exu_wbck — E203 write-back arbiter.
// Tests: ALU-only passthrough, long-pipe unconditional priority (wins the RF
// port and stalls the ALU channel), FPU-destination suppression of the RF
// write enable, and idle behavior.
//
// NOTE: this replaces a stale tb (VExuWbck.h) that targeted an earlier
// revision of the fixture before the e203 fixtures were rewritten against the
// real E203 RTL, which renamed the construct to `e203_exu_wbck`. The old tb
// has not compiled since. Ported to the current class name (Ve203_exu_wbck).
// The current-generation e203_exu_wbck_vltor_tb.cpp covers the Verilator
// flavor and is untouched.
//
// The module is purely combinational (clk/rst_n ports exist for interface
// compatibility only): drive inputs, eval(), check.
//
// Run with:
//   arch sim tests/e203/e203_exu_wbck.arch --tb tests/e203/e203_exu_wbck_tb.cpp

#include "Ve203_exu_wbck.h"
#include <cstdio>
#include <cstdint>

static int fail_count = 0;

#define CHECK(cond, fmt, ...) do { \
    if (!(cond)) { \
        printf("  FAIL: " fmt "\n", ##__VA_ARGS__); \
        fail_count++; \
    } \
} while(0)

static Ve203_exu_wbck* dut;

static void clear_inputs() {
    dut->clk = 0;
    dut->rst_n = 1;
    dut->alu_wbck_i_valid = 0;
    dut->alu_wbck_i_wdat = 0;
    dut->alu_wbck_i_rdidx = 0;
    dut->longp_wbck_i_valid = 0;
    dut->longp_wbck_i_wdat = 0;
    dut->longp_wbck_i_flags = 0;
    dut->longp_wbck_i_rdidx = 0;
    dut->longp_wbck_i_rdfpu = 0;
    dut->eval();
}

int main() {
    dut = new Ve203_exu_wbck;

    // ── Test 1: Idle ─────────────────────────────────────────────────
    printf("Test 1: Idle\n");
    clear_inputs();
    CHECK(dut->rf_wbck_o_ena == 0, "no RF write when idle, got %d", dut->rf_wbck_o_ena);
    CHECK(dut->longp_wbck_i_ready == 1, "longp always ready, got %d", dut->longp_wbck_i_ready);
    CHECK(dut->alu_wbck_i_ready == 1, "alu ready when longp idle, got %d", dut->alu_wbck_i_ready);

    // ── Test 2: ALU-only passthrough ─────────────────────────────────
    printf("Test 2: ALU passthrough\n");
    clear_inputs();
    dut->alu_wbck_i_valid = 1;
    dut->alu_wbck_i_wdat = 0x12345678u;
    dut->alu_wbck_i_rdidx = 11;
    dut->eval();
    CHECK(dut->rf_wbck_o_ena == 1, "ALU wbck should enable the RF write, got %d", dut->rf_wbck_o_ena);
    CHECK(dut->rf_wbck_o_wdat == 0x12345678u, "RF wdat should be the ALU data, got 0x%08x", dut->rf_wbck_o_wdat);
    CHECK(dut->rf_wbck_o_rdidx == 11, "RF rdidx should be the ALU rd, got %d", dut->rf_wbck_o_rdidx);
    CHECK(dut->alu_wbck_i_ready == 1, "alu stays ready with no longp, got %d", dut->alu_wbck_i_ready);

    // ── Test 3: Long-pipe priority over ALU ──────────────────────────
    printf("Test 3: Long-pipe priority\n");
    clear_inputs();
    dut->alu_wbck_i_valid = 1;
    dut->alu_wbck_i_wdat = 0x11111111u;
    dut->alu_wbck_i_rdidx = 4;
    dut->longp_wbck_i_valid = 1;
    dut->longp_wbck_i_wdat = 0x22222222u;
    dut->longp_wbck_i_rdidx = 21;
    dut->eval();
    CHECK(dut->rf_wbck_o_ena == 1, "longp wbck should enable the RF write, got %d", dut->rf_wbck_o_ena);
    CHECK(dut->rf_wbck_o_wdat == 0x22222222u, "longp data must win the mux, got 0x%08x", dut->rf_wbck_o_wdat);
    CHECK(dut->rf_wbck_o_rdidx == 21, "longp rdidx must win the mux, got %d", dut->rf_wbck_o_rdidx);
    CHECK(dut->alu_wbck_i_ready == 0, "alu must stall while longp writes, got %d", dut->alu_wbck_i_ready);
    CHECK(dut->longp_wbck_i_ready == 1, "longp always ready, got %d", dut->longp_wbck_i_ready);
    // ALU channel drains once longp deasserts.
    dut->longp_wbck_i_valid = 0;
    dut->eval();
    CHECK(dut->alu_wbck_i_ready == 1, "alu recovers when longp drops, got %d", dut->alu_wbck_i_ready);
    CHECK(dut->rf_wbck_o_wdat == 0x11111111u, "mux falls back to ALU data, got 0x%08x", dut->rf_wbck_o_wdat);
    CHECK(dut->rf_wbck_o_rdidx == 4, "mux falls back to ALU rdidx, got %d", dut->rf_wbck_o_rdidx);

    // ── Test 4: FPU destination suppresses the integer RF write ──────
    printf("Test 4: FPU rd suppression\n");
    clear_inputs();
    dut->longp_wbck_i_valid = 1;
    dut->longp_wbck_i_wdat = 0x33333333u;
    dut->longp_wbck_i_rdidx = 8;
    dut->longp_wbck_i_rdfpu = 1;
    dut->eval();
    CHECK(dut->rf_wbck_o_ena == 0, "FPU-destination longp wbck must not write the int RF, got %d",
          dut->rf_wbck_o_ena);
    CHECK(dut->longp_wbck_i_ready == 1, "longp still ready (write is consumed), got %d",
          dut->longp_wbck_i_ready);
    // An FPU longp write still blocks the ALU channel (it holds the port).
    dut->alu_wbck_i_valid = 1;
    dut->eval();
    CHECK(dut->alu_wbck_i_ready == 0, "alu still stalls behind an FPU longp wbck, got %d",
          dut->alu_wbck_i_ready);
    CHECK(dut->rf_wbck_o_ena == 0, "no RF write while FPU longp holds the port, got %d",
          dut->rf_wbck_o_ena);
    // Integer longp write re-enables.
    dut->longp_wbck_i_rdfpu = 0;
    dut->eval();
    CHECK(dut->rf_wbck_o_ena == 1, "integer longp wbck writes the RF, got %d", dut->rf_wbck_o_ena);

    printf("\n=== e203_exu_wbck: %d failure(s) ===\n", fail_count);
    return fail_count ? 1 : 0;
}
