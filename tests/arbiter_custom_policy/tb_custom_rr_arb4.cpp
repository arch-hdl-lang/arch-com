// Testbench for CustomRrArb4 (`policy CustomRrGrant` + `hook grant_select`).
//
// Runs unchanged under BOTH backends — `arch sim --tb` and Verilator on
// the emitted SV — so the two must agree or one of them fails. That is
// the point: arch-hdl-lang/arch-com#912 was a silent divergence, where
// `arch sim` ignored the hook entirely and substituted a fixed
// lowest-index-wins priority scan.
//
// Pre-fix arch-sim behavior: requester 0 wins every cycle in phase A
// (and phase B never grants requester 3, because the requester count
// fell back to 2 for this `param N` arbiter).
//
// Prints "PASS custom_rr_arb4" to stdout on success; non-zero exit on
// any mismatch.

#include "VCustomRrArb4.h"
#include <cstdio>

int main() {
    VCustomRrArb4 dut;

    // Reset. Drive nothing during reset.
    dut.clk = 0;
    dut.rst = 1;
    dut.prio = 0;
    dut.request_valid = 0;
    for (int i = 0; i < 4; i++) {
        dut.clk = 0; dut.eval();
        dut.clk = 1; dut.eval();
    }
    dut.rst = 0;

    // ── Phase A: all four requesting, no priority override ───────────
    // The hook rotates strictly: consecutive grants walk 0,1,2,3,0,...
    // The starting index depends on where reset left `last_grant`, so
    // derive it from the first sample (same convention as tb_rr_arb3).
    dut.request_valid = 0xF;
    dut.prio = 0;
    int grants[24];
    for (int cyc = 0; cyc < 24; cyc++) {
        dut.clk = 0; dut.eval();
        dut.clk = 1; dut.eval();
        if (!dut.grant_valid) {
            fprintf(stdout, "FAIL: phase A grant_valid low at cycle %d\n", cyc);
            return 1;
        }
        if ((unsigned)dut.request_ready != (1u << (unsigned)dut.grant_requester)) {
            fprintf(stdout,
                "FAIL: phase A cycle %d ready=0x%x not one-hot for idx %u\n",
                cyc, (unsigned)dut.request_ready, (unsigned)dut.grant_requester);
            return 1;
        }
        grants[cyc] = (int)dut.grant_requester;
    }
    int start = grants[0];
    if (start < 0 || start > 3) {
        fprintf(stdout, "FAIL: invalid grant idx at cycle 0: %d\n", start);
        return 1;
    }
    for (int i = 0; i < 24; i++) {
        int expected = (start + i) % 4;
        if (grants[i] != expected) {
            fprintf(stdout,
                "FAIL: phase A cycle %d granted %d, expected %d (strict RR)\n",
                i, grants[i], expected);
            fprintf(stdout, "grant sequence: ");
            for (int j = 0; j < 24; j++) fprintf(stdout, "%d ", grants[j]);
            fprintf(stdout, "\n");
            return 1;
        }
    }
    int count[4] = {0, 0, 0, 0};
    for (int i = 0; i < 24; i++) count[grants[i]]++;
    for (int i = 0; i < 4; i++) {
        if (count[i] != 6) {
            fprintf(stdout, "FAIL: unfair distribution: 0=%d 1=%d 2=%d 3=%d\n",
                    count[0], count[1], count[2], count[3]);
            return 1;
        }
    }

    // ── Phase B: only requesters 1 and 3 ─────────────────────────────
    // Rotation must skip the silent requesters and alternate 1,3,1,3.
    // Requester 3 is only reachable if the requester count came from
    // `ports[N]` — a count of 2 can never grant it.
    dut.request_valid = 0b1010;
    int prev = -1;
    for (int cyc = 0; cyc < 8; cyc++) {
        dut.clk = 0; dut.eval();
        dut.clk = 1; dut.eval();
        int g = (int)dut.grant_requester;
        if (!dut.grant_valid || (g != 1 && g != 3)) {
            fprintf(stdout,
                "FAIL: phase B cycle %d granted idx %d (valid=%d), expected 1 or 3\n",
                cyc, g, (int)dut.grant_valid);
            return 1;
        }
        if (prev >= 0 && g == prev) {
            fprintf(stdout,
                "FAIL: phase B cycle %d repeated idx %d — not rotating\n", cyc, g);
            return 1;
        }
        prev = g;
    }

    // ── Phase C: priority override ───────────────────────────────────
    // `prio` is an ordinary arbiter port wired into the hook's third
    // argument; flagging requester 2 must pin every grant to 2,
    // regardless of rotation state.
    dut.request_valid = 0xF;
    dut.prio = 0b0100;
    for (int cyc = 0; cyc < 6; cyc++) {
        dut.clk = 0; dut.eval();
        dut.clk = 1; dut.eval();
        if (!dut.grant_valid || (int)dut.grant_requester != 2) {
            fprintf(stdout,
                "FAIL: phase C cycle %d granted idx %d (valid=%d), expected 2 — "
                "`prio` port not reaching the hook\n",
                cyc, (int)dut.grant_requester, (int)dut.grant_valid);
            return 1;
        }
    }

    // ── Phase D: no requesters ───────────────────────────────────────
    dut.request_valid = 0;
    dut.prio = 0;
    for (int cyc = 0; cyc < 4; cyc++) {
        dut.clk = 0; dut.eval();
        dut.clk = 1; dut.eval();
        if (dut.grant_valid || dut.request_ready != 0) {
            fprintf(stdout,
                "FAIL: phase D cycle %d granted with no requesters "
                "(valid=%d ready=0x%x)\n",
                cyc, (int)dut.grant_valid, (unsigned)dut.request_ready);
            return 1;
        }
    }

    fprintf(stdout, "PASS custom_rr_arb4 (hook-driven rotation, prio override)\n");
    return 0;
}
