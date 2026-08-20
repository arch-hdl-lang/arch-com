// ARCH sim testbench for e203_exu_longpwbck — E203 long-pipe writeback
// collector. Tests: LSU-over-NICE arbitration, clean writeback routing with
// OITF rd/rdfpu/pc annotation, error diversion to the exception port (bus
// error, badaddr, ld/st attribution), OITF retire strobes on both the clean
// and the error path, and backpressure from the writeback sink.
//
// NOTE: this replaces a stale tb (VExuLongpWbck.h) that targeted an earlier
// revision of the fixture before the e203 fixtures were rewritten against the
// real E203 RTL, which renamed the construct to `e203_exu_longpwbck`. The old
// tb has not compiled since. Ported to the current class name
// (Ve203_exu_longpwbck).
//
// The module is purely combinational (clk/rst_n for interface compatibility):
// drive inputs, eval(), check.
//
// Run with:
//   arch sim tests/e203/e203_exu_longpwbck.arch --tb tests/e203/e203_exu_longpwbck_tb.cpp

#include "Ve203_exu_longpwbck.h"
#include <cstdio>
#include <cstdint>

static int fail_count = 0;

#define CHECK(cond, fmt, ...) do { \
    if (!(cond)) { \
        printf("  FAIL: " fmt "\n", ##__VA_ARGS__); \
        fail_count++; \
    } \
} while(0)

static Ve203_exu_longpwbck* dut;

static void clear_inputs() {
    dut->clk = 0;
    dut->rst_n = 1;
    dut->lsu_wbck_i_valid = 0;
    dut->lsu_wbck_i_wdat = 0;
    dut->lsu_wbck_i_itag = 0;
    dut->lsu_wbck_i_err = 0;
    dut->lsu_cmt_i_buserr = 0;
    dut->lsu_cmt_i_badaddr = 0;
    dut->lsu_cmt_i_ld = 0;
    dut->lsu_cmt_i_st = 0;
    dut->longp_wbck_o_ready = 1;
    dut->longp_excp_o_ready = 1;
    dut->oitf_empty = 0;
    dut->oitf_ret_ptr = 0;
    dut->oitf_ret_rdidx = 0;
    dut->oitf_ret_pc = 0;
    dut->oitf_ret_rdwen = 0;
    dut->oitf_ret_rdfpu = 0;
    dut->nice_longp_wbck_i_valid = 0;
    dut->nice_longp_wbck_i_wdat = 0;
    dut->nice_longp_wbck_i_itag = 0;
    dut->nice_longp_wbck_i_err = 0;
    dut->eval();
}

int main() {
    dut = new Ve203_exu_longpwbck;

    // ── Test 1: Idle ─────────────────────────────────────────────────
    printf("Test 1: Idle\n");
    clear_inputs();
    CHECK(dut->longp_wbck_o_valid == 0, "no wbck when idle, got %d", dut->longp_wbck_o_valid);
    CHECK(dut->longp_excp_o_valid == 0, "no excp when idle, got %d", dut->longp_excp_o_valid);
    CHECK(dut->oitf_ret_ena == 0, "no OITF retire when idle, got %d", dut->oitf_ret_ena);
    CHECK(dut->lsu_wbck_i_ready == 0, "lsu ready is grant-qualified (lsu_win=0 at idle), got %d",
          dut->lsu_wbck_i_ready);
    // NICE ready = ~lsu_win & sink-ready: legitimately high at idle (a ready
    // may be asserted without a valid).
    CHECK(dut->nice_longp_wbck_i_ready == 1, "nice ready should be high at idle with the sink free, got %d",
          dut->nice_longp_wbck_i_ready);

    // ── Test 2: Clean LSU writeback with OITF annotation ─────────────
    printf("Test 2: LSU clean writeback\n");
    clear_inputs();
    dut->lsu_wbck_i_valid = 1;
    dut->lsu_wbck_i_wdat = 0xC0FFEE00u;      // load result
    dut->oitf_ret_rdidx = 14;
    dut->oitf_ret_rdfpu = 0;
    dut->oitf_ret_pc = 0x1234;
    dut->eval();
    CHECK(dut->longp_wbck_o_valid == 1, "clean LSU wbck should be valid, got %d", dut->longp_wbck_o_valid);
    CHECK(dut->longp_wbck_o_wdat == 0xC0FFEE00u, "wbck data should be the LSU data, got 0x%08x",
          dut->longp_wbck_o_wdat);
    CHECK(dut->longp_wbck_o_rdidx == 14, "wbck rdidx comes from the OITF, got %d", dut->longp_wbck_o_rdidx);
    CHECK(dut->longp_wbck_o_rdfpu == 0, "wbck rdfpu comes from the OITF, got %d", dut->longp_wbck_o_rdfpu);
    CHECK(dut->longp_excp_o_valid == 0, "clean wbck raises no exception, got %d", dut->longp_excp_o_valid);
    CHECK(dut->lsu_wbck_i_ready == 1, "lsu should be granted, got %d", dut->lsu_wbck_i_ready);
    CHECK(dut->oitf_ret_ena == 1, "clean wbck retires the OITF entry, got %d", dut->oitf_ret_ena);
    // FPU-tagged OITF entry propagates.
    dut->oitf_ret_rdfpu = 1;
    dut->eval();
    CHECK(dut->longp_wbck_o_rdfpu == 1, "rdfpu should follow the OITF tag, got %d", dut->longp_wbck_o_rdfpu);

    // ── Test 3: LSU error diverts to the exception port ──────────────
    printf("Test 3: LSU bus-error exception\n");
    clear_inputs();
    dut->lsu_wbck_i_valid = 1;
    dut->lsu_wbck_i_wdat = 0xDDDDDDDDu;
    dut->lsu_wbck_i_err = 1;
    dut->lsu_cmt_i_buserr = 1;
    dut->lsu_cmt_i_badaddr = 0x80001234u;
    dut->lsu_cmt_i_ld = 1;
    dut->oitf_ret_pc = 0x4444;
    dut->eval();
    CHECK(dut->longp_wbck_o_valid == 0, "errored wbck must not write the RF, got %d", dut->longp_wbck_o_valid);
    CHECK(dut->longp_excp_o_valid == 1, "errored wbck raises the exception, got %d", dut->longp_excp_o_valid);
    CHECK(dut->longp_excp_o_buserr == 1, "buserr flag should pass through, got %d", dut->longp_excp_o_buserr);
    CHECK(dut->longp_excp_o_badaddr == 0x80001234u, "badaddr should pass through, got 0x%08x",
          dut->longp_excp_o_badaddr);
    CHECK(dut->longp_excp_o_ld == 1, "ld attribution should pass through, got %d", dut->longp_excp_o_ld);
    CHECK(dut->longp_excp_o_st == 0, "st attribution should be 0 for a load, got %d", dut->longp_excp_o_st);
    CHECK(dut->longp_excp_o_pc == 0x4444, "excp pc comes from the OITF, got 0x%08x", dut->longp_excp_o_pc);
    CHECK(dut->longp_excp_o_insterr == 0, "insterr is tied low, got %d", dut->longp_excp_o_insterr);
    // The error path still consumes the LSU beat and retires the OITF entry,
    // even with the wbck sink stalled.
    dut->longp_wbck_o_ready = 0;
    dut->eval();
    CHECK(dut->lsu_wbck_i_ready == 1, "errored beat is consumed regardless of wbck ready, got %d",
          dut->lsu_wbck_i_ready);
    CHECK(dut->oitf_ret_ena == 1, "errored beat retires the OITF entry, got %d", dut->oitf_ret_ena);
    // Store-error attribution.
    dut->lsu_cmt_i_ld = 0;
    dut->lsu_cmt_i_st = 1;
    dut->eval();
    CHECK(dut->longp_excp_o_st == 1, "st attribution should pass through, got %d", dut->longp_excp_o_st);

    // ── Test 4: NICE writeback when LSU is idle ──────────────────────
    printf("Test 4: NICE writeback\n");
    clear_inputs();
    dut->nice_longp_wbck_i_valid = 1;
    dut->nice_longp_wbck_i_wdat = 0x0071CE00u;   // NICE result payload
    dut->eval();
    CHECK(dut->longp_wbck_o_valid == 1, "NICE wbck should be valid, got %d", dut->longp_wbck_o_valid);
    CHECK(dut->nice_longp_wbck_i_ready == 1, "NICE should be granted, got %d", dut->nice_longp_wbck_i_ready);
    CHECK(dut->lsu_wbck_i_ready == 0, "lsu not granted while idle, got %d", dut->lsu_wbck_i_ready);
    CHECK(dut->oitf_ret_ena == 1, "NICE wbck retires the OITF entry, got %d", dut->oitf_ret_ena);

    // ── Test 5: LSU has priority over NICE ───────────────────────────
    printf("Test 5: LSU-over-NICE arbitration\n");
    clear_inputs();
    dut->lsu_wbck_i_valid = 1;
    dut->lsu_wbck_i_wdat = 0xAAAA0000u;
    dut->nice_longp_wbck_i_valid = 1;
    dut->nice_longp_wbck_i_wdat = 0xBBBB0000u;
    dut->eval();
    CHECK(dut->longp_wbck_o_wdat == 0xAAAA0000u, "LSU data must win, got 0x%08x", dut->longp_wbck_o_wdat);
    CHECK(dut->lsu_wbck_i_ready == 1, "LSU granted, got %d", dut->lsu_wbck_i_ready);
    CHECK(dut->nice_longp_wbck_i_ready == 0, "NICE must wait behind the LSU, got %d",
          dut->nice_longp_wbck_i_ready);
    // A clean NICE beat must not be masked by LSU error state: with both valid
    // and the LSU beat errored, the exception wins the cycle (sel = LSU).
    dut->lsu_wbck_i_err = 1;
    dut->eval();
    CHECK(dut->longp_excp_o_valid == 1, "LSU error still owns the cycle, got %d", dut->longp_excp_o_valid);
    CHECK(dut->longp_wbck_o_valid == 0, "no clean wbck this cycle, got %d", dut->longp_wbck_o_valid);
    CHECK(dut->nice_longp_wbck_i_ready == 0, "NICE still waits, got %d", dut->nice_longp_wbck_i_ready);

    // ── Test 6: Writeback backpressure ───────────────────────────────
    printf("Test 6: Backpressure\n");
    clear_inputs();
    dut->lsu_wbck_i_valid = 1;
    dut->lsu_wbck_i_wdat = 0x77770000u;
    dut->longp_wbck_o_ready = 0;
    dut->eval();
    CHECK(dut->longp_wbck_o_valid == 1, "wbck valid is offered even when stalled, got %d",
          dut->longp_wbck_o_valid);
    CHECK(dut->lsu_wbck_i_ready == 0, "clean beat must wait for the sink, got %d", dut->lsu_wbck_i_ready);
    CHECK(dut->oitf_ret_ena == 0, "no retire while the clean beat waits, got %d", dut->oitf_ret_ena);
    dut->longp_wbck_o_ready = 1;
    dut->eval();
    CHECK(dut->lsu_wbck_i_ready == 1, "beat completes when the sink frees, got %d", dut->lsu_wbck_i_ready);
    CHECK(dut->oitf_ret_ena == 1, "retire strobes when the beat completes, got %d", dut->oitf_ret_ena);

    printf("\n=== e203_exu_longpwbck: %d failure(s) ===\n", fail_count);
    return fail_count ? 1 : 0;
}
