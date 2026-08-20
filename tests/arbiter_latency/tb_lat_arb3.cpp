// Testbench for LatArb3 (`policy round_robin; latency 3;`).
//
// Runs unchanged under BOTH backends — `arch sim --tb` and Verilator on
// the emitted SV — so the two must agree or one of them fails. That is
// the point: arch-hdl-lang/arch-com#917 was a silent divergence, where
// `arch sim` ignored `latency` entirely and presented the grant
// `latency - 1` cycles earlier than the SV every downstream tool sees.
//
// Sampling point: the outputs are read *before* the rising edge, with
// the cycle's request already applied. That is where the latency is
// observable — a `latency 1` arbiter drives the grant combinationally,
// so it would answer in the same cycle, while `latency 3` answers with
// the previous cycle's grant. (Sampling after the edge cannot tell the
// two apart: the edge that registers stage 1 also samples the comb that
// the current request just produced.)
//
// The latency-3 sibling of tb_lat_arb2.cpp: it exercises the *chained*
// case (two register stages, one of them an intermediate `_p1` reg),
// which a fix handling only a single stage would fail.
//
// Pre-fix arch-sim behavior: phase A grants at cycle 0 instead of cycle
// 2, and every phase-B grant names the requester asserting in the
// current cycle rather than two cycles earlier.
//
// Prints "PASS lat_arb3" to stdout on success; non-zero exit on any
// mismatch.

#include "VLatArb3.h"
#include <cstdio>

// `latency N` ⇒ the grant is registered through N-1 stages.
static const int LAT = 3;

int main() {
    VLatArb3 dut;

    // Reset. The pipeline registers are reset-cleared, so the grant must
    // stay low until the chain refills after release.
    dut.clk = 0;
    dut.rst = 1;
    dut.request_valid = 0;
    for (int i = 0; i < 4; i++) {
        dut.clk = 0; dut.eval();
        dut.clk = 1; dut.eval();
    }
    dut.rst = 0;

    // ── Phase A: one steady requester ────────────────────────────────
    // The combinational grant is valid from cycle 0 on, so the output
    // grant is valid from cycle LAT-1 on and low before that.
    dut.request_valid = 0b0001;
    for (int cyc = 0; cyc < 6; cyc++) {
        dut.clk = 0; dut.eval();
        int want = (cyc >= LAT - 1) ? 1 : 0;
        if ((int)dut.grant_valid != want) {
            fprintf(stdout,
                "FAIL: phase A cycle %d grant_valid=%d, expected %d "
                "(latency %d => %d register stage(s))\n",
                cyc, (int)dut.grant_valid, want, LAT, LAT - 1);
            return 1;
        }
        unsigned want_rdy = want ? 0b0001u : 0u;
        if ((unsigned)dut.request_ready != want_rdy) {
            fprintf(stdout,
                "FAIL: phase A cycle %d request_ready=0x%x, expected 0x%x "
                "(ready must be pipelined with the grant)\n",
                cyc, (unsigned)dut.request_ready, want_rdy);
            return 1;
        }
        dut.clk = 1; dut.eval();
    }

    // ── Phase B: a walking single requester ──────────────────────────
    // With exactly one requester asserting each cycle, the grant index
    // is that requester — delayed by LAT-1 cycles. This is the check
    // that fails outright on an off-by-one pipeline.
    for (int cyc = 0; cyc < 16; cyc++) {
        dut.request_valid = 1u << (cyc % 4);
        dut.clk = 0; dut.eval();
        int src = cyc - (LAT - 1);
        if (src >= 0) {
            int want_idx = src % 4;
            if (!dut.grant_valid || (int)dut.grant_requester != want_idx) {
                fprintf(stdout,
                    "FAIL: phase B cycle %d granted idx %d (valid=%d), "
                    "expected %d — the request asserted %d cycle(s) earlier\n",
                    cyc, (int)dut.grant_requester, (int)dut.grant_valid,
                    want_idx, LAT - 1);
                return 1;
            }
            if ((unsigned)dut.request_ready != (1u << want_idx)) {
                fprintf(stdout,
                    "FAIL: phase B cycle %d request_ready=0x%x, expected 0x%x\n",
                    cyc, (unsigned)dut.request_ready, 1u << want_idx);
                return 1;
            }
        }
        dut.clk = 1; dut.eval();
    }

    // ── Phase C: drain ───────────────────────────────────────────────
    // After every requester deasserts, the already-pipelined grants keep
    // draining for LAT-1 more cycles, then the output goes idle.
    dut.request_valid = 0;
    for (int cyc = 0; cyc < 4; cyc++) {
        dut.clk = 0; dut.eval();
        int want = (cyc < LAT - 1) ? 1 : 0;
        if ((int)dut.grant_valid != want) {
            fprintf(stdout,
                "FAIL: phase C cycle %d grant_valid=%d, expected %d "
                "(drain of %d stage(s))\n",
                cyc, (int)dut.grant_valid, want, LAT - 1);
            return 1;
        }
        dut.clk = 1; dut.eval();
    }

    // ── Phase D: reset clears the whole pipeline ─────────────────────
    dut.request_valid = 0b1111;
    for (int i = 0; i < 3; i++) {
        dut.clk = 0; dut.eval();
        dut.clk = 1; dut.eval();
    }
    dut.rst = 1;
    dut.clk = 0; dut.eval();
    dut.clk = 1; dut.eval();
    dut.clk = 0; dut.eval();
    if (dut.grant_valid || dut.request_ready != 0) {
        fprintf(stdout,
            "FAIL: phase D reset did not clear the grant pipeline "
            "(valid=%d ready=0x%x)\n",
            (int)dut.grant_valid, (unsigned)dut.request_ready);
        return 1;
    }

    fprintf(stdout, "PASS lat_arb3 (grant pipelined by latency-1 stages)\n");
    return 0;
}
