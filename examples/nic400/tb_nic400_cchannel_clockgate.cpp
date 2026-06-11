// Behavioral testbench for Nic400CChannelClockGate (AMBA Low-Power Interface
// "C-channel" clock-gating handshake, NIC-400 TRM §2.2.3).
//
// Mirrors the harc-side Nic400CChannelClockGate_test.harc scenario so the same
// gate/wake contract is exercised through arch-com's own sim + Verilator
// backends (the harc test runs only through the harc harness):
//   reset            → ACTIVE  (csysack=1, clk_en=1, cactive=0 when idle)
//   csysreq=0 idle   → GATED   (csysack=0, clk_en=0)
//   busy while gated  → WAKE    (back to ACTIVE, cactive=1)
//   csysreq=0 + busy  → stays ACTIVE (a gate request can't gate active work)
//   csysreq=1        → ACTIVE (run again)
//
// FSM outputs are combinational on the state register, so one clk edge then a
// read observes the post-edge state. Runs identically under arch-sim (--tb)
// and Verilator; prints "PASS cchannel_clockgate" on success, non-zero exit on
// any mismatch.

#include "VNic400CChannelClockGate.h"
#include <cstdio>
#include <cstdlib>

static VNic400CChannelClockGate dut;

static void tick() {
    dut.clk = 0;
    dut.eval();
    dut.clk = 1;
    dut.eval();
}

static int fail(const char *msg) {
    fprintf(stdout, "FAIL cchannel_clockgate: %s\n", msg);
    return 1;
}

int main() {
    // ── Reset → ACTIVE, running ──────────────────────────────────────────
    dut.rst = 1;
    dut.cc_csysreq = 1; // controller wants the clock running
    dut.busy = 0;
    tick();
    tick();
    dut.rst = 0;
    tick();
    if (dut.cc_csysack != 1) return fail("post-reset csysack != 1");
    if (dut.clk_en != 1)     return fail("post-reset clk_en != 1");
    if (dut.cc_cactive != 0) return fail("idle cactive != 0");

    // ── 1. Request quiesce while idle → GATED ────────────────────────────
    dut.cc_csysreq = 0;
    dut.busy = 0;
    tick();
    if (dut.cc_csysack != 0) return fail("gate: csysack != 0");
    if (dut.clk_en != 0)     return fail("gate: clk_en != 0 (clock not gated)");

    // ── 2. Activity while gated → WAKE back to ACTIVE ────────────────────
    dut.busy = 1; // csysreq still 0, but pending work forces a wake
    tick();
    if (dut.clk_en != 1)     return fail("wake: clk_en != 1 (clock stayed gated)");
    if (dut.cc_csysack != 1) return fail("wake: csysack != 1");
    if (dut.cc_cactive != 1) return fail("wake: cactive != 1 (busy not reflected)");

    // ── 3. csysreq=0 but busy → cannot gate active work, stays ACTIVE ────
    dut.cc_csysreq = 0;
    dut.busy = 1;
    tick();
    if (dut.clk_en != 1)     return fail("busy-hold: clk_en != 1 (gated despite busy)");
    if (dut.cc_cactive != 1) return fail("busy-hold: cactive != 1");

    // ── 4. Drop busy with csysreq=0 → now safe to GATE ───────────────────
    dut.busy = 0;
    tick();
    if (dut.clk_en != 0)     return fail("regate: clk_en != 0 (idle gate request ignored)");
    if (dut.cc_csysack != 0) return fail("regate: csysack != 0");

    // ── 5. csysreq=1 → ACTIVE again ──────────────────────────────────────
    dut.cc_csysreq = 1;
    dut.busy = 0;
    tick();
    if (dut.cc_csysack != 1) return fail("rerun: csysack != 1");
    if (dut.clk_en != 1)     return fail("rerun: clk_en != 1");
    if (dut.cc_cactive != 0) return fail("rerun: cactive != 0");

    fprintf(stdout, "PASS cchannel_clockgate\n");
    return 0;
}
