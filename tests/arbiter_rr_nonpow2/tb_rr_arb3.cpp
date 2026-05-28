// Verilator testbench for RRArb3 (round-robin, NUM_REQ=3).
//
// Drives all three requesters always-asserted and captures the
// grant_requester sequence for 24 cycles. Expects strict round-robin
// (each idx wins exactly 8/24 = 1/3 of cycles) — the documented
// "fair distribution" guarantee for `policy round_robin`.
//
// Pre-fix (rr_ptr_r <= rr_ptr_r + 1, no explicit %): NUM_REQ=3 produced
// the unfair sequence 0,1,2,0,0,1,2,0,... with idx 0 winning 50%.
//
// Prints "PASS rr_arb3" to stdout on success; non-zero exit on any
// mismatch.

#include "VRRArb3.h"
#include <cstdio>
#include <cstdlib>

int main() {
    VRRArb3 dut;

    // Reset the design for a few cycles. Drive nothing during reset.
    dut.clk = 0;
    dut.rst = 1;
    dut.request_valid = 0;
    for (int i = 0; i < 4; i++) {
        dut.clk = 0; dut.eval();
        dut.clk = 1; dut.eval();
    }

    // Release reset; assert all three requesters every cycle.
    dut.rst = 0;
    dut.request_valid = 0b111;

    // Capture 24 grant_requester values, one per posedge.
    int grants[24];
    for (int cyc = 0; cyc < 24; cyc++) {
        dut.clk = 0; dut.eval();
        dut.clk = 1; dut.eval();
        if (!dut.grant_valid) {
            fprintf(stdout, "FAIL: grant_valid de-asserted at cycle %d\n", cyc);
            return 1;
        }
        grants[cyc] = (int)dut.grant_requester;
    }

    // Strict round-robin: the per-cycle sequence is (start + i) % 3 for
    // some start in {0, 1, 2}. Determine start from cycle 0 and verify.
    int start = grants[0];
    if (start < 0 || start > 2) {
        fprintf(stdout, "FAIL: invalid grant idx at cycle 0: %d\n", start);
        return 1;
    }
    for (int i = 0; i < 24; i++) {
        int expected = (start + i) % 3;
        if (grants[i] != expected) {
            fprintf(stdout,
                "FAIL: cycle %d granted %d, expected %d (strict RR seq)\n",
                i, grants[i], expected);
            fprintf(stdout, "grant sequence: ");
            for (int j = 0; j < 24; j++) fprintf(stdout, "%d ", grants[j]);
            fprintf(stdout, "\n");
            return 1;
        }
    }

    // Each idx must win exactly 8 of 24 cycles. The pre-fix pattern is
    // not strict RR (caught above) AND is unfair on counts — this check
    // is redundant but documents intent.
    int count[3] = {0, 0, 0};
    for (int i = 0; i < 24; i++) count[grants[i]]++;
    if (count[0] != 8 || count[1] != 8 || count[2] != 8) {
        fprintf(stdout, "FAIL: unfair distribution: 0=%d 1=%d 2=%d\n",
                count[0], count[1], count[2]);
        return 1;
    }

    fprintf(stdout, "PASS rr_arb3 (strict round-robin, idx-fair)\n");
    return 0;
}
