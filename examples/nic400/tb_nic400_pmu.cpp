// Nic400Pmu smoke test — drive synthetic AR/AW/R/W/B event pulses on each
// master's input lanes and check that the corresponding counters integrate
// the pulses correctly. NUM_MASTERS=3, COUNTER_W=32.
//
// Test pattern:
//   master 0:  3 AR, 8 R (e.g. a 1+7 burst), 2 AW, 2 W, 2 B (single-beat ×2)
//   master 1:  1 AR, 4 R (burst-of-4), 0 writes
//   master 2:  0 reads, 5 AW, 5 W, 5 B (5 single-beat writes)
//
// Each pulse is a one-cycle assertion of the corresponding event_in[i] bit.

#include "VNic400Pmu.h"
#include <cstdint>
#include <cstdio>

static VNic400Pmu dut;
static uint64_t cycle = 0;

static void tick() { dut.clk = 0; dut.eval(); dut.clk = 1; dut.eval(); cycle++; }

static void clear_events() {
    for (int i = 0; i < 3; ++i) {
        dut.ar_event[i] = 0;
        dut.r_event[i]  = 0;
        dut.aw_event[i] = 0;
        dut.w_event[i]  = 0;
        dut.b_event[i]  = 0;
    }
}

// Drive a one-cycle pulse on event_in[master_idx] and advance the clock.
// All other event lines are 0 for that cycle.
static void pulse_ar(unsigned m) { clear_events(); dut.ar_event[m] = 1; tick(); clear_events(); }
static void pulse_r(unsigned m)  { clear_events(); dut.r_event[m]  = 1; tick(); clear_events(); }
static void pulse_aw(unsigned m) { clear_events(); dut.aw_event[m] = 1; tick(); clear_events(); }
static void pulse_w(unsigned m)  { clear_events(); dut.w_event[m]  = 1; tick(); clear_events(); }
static void pulse_b(unsigned m)  { clear_events(); dut.b_event[m]  = 1; tick(); clear_events(); }

static int check(const char* label, uint32_t got, uint32_t expect) {
    if (got != expect) {
        std::printf("FAIL %s: got %u, expected %u\n", label, got, expect);
        return 1;
    }
    return 0;
}

int main() {
    dut.rst = 0;
    clear_events();
    for (int i = 0; i < 4; ++i) tick();
    dut.rst = 1;
    for (int i = 0; i < 3; ++i) tick();

    // Master 0: 3 AR, 8 R beats, 2 AW, 2 W, 2 B.
    for (int i = 0; i < 3; ++i) pulse_ar(0);
    for (int i = 0; i < 8; ++i) pulse_r(0);
    for (int i = 0; i < 2; ++i) pulse_aw(0);
    for (int i = 0; i < 2; ++i) pulse_w(0);
    for (int i = 0; i < 2; ++i) pulse_b(0);

    // Master 1: 1 AR, 4 R beats.
    pulse_ar(1);
    for (int i = 0; i < 4; ++i) pulse_r(1);

    // Master 2: 5 AW, 5 W, 5 B.
    for (int i = 0; i < 5; ++i) pulse_aw(2);
    for (int i = 0; i < 5; ++i) pulse_w(2);
    for (int i = 0; i < 5; ++i) pulse_b(2);

    // Settle one more tick so combinational outputs reflect the latest reg state.
    tick();

    int err = 0;
    err |= check("m0.ar", dut.ar_count[0], 3);
    err |= check("m0.r",  dut.r_count[0],  8);
    err |= check("m0.aw", dut.aw_count[0], 2);
    err |= check("m0.w",  dut.w_count[0],  2);
    err |= check("m0.b",  dut.b_count[0],  2);

    err |= check("m1.ar", dut.ar_count[1], 1);
    err |= check("m1.r",  dut.r_count[1],  4);
    err |= check("m1.aw", dut.aw_count[1], 0);
    err |= check("m1.w",  dut.w_count[1],  0);
    err |= check("m1.b",  dut.b_count[1],  0);

    err |= check("m2.ar", dut.ar_count[2], 0);
    err |= check("m2.r",  dut.r_count[2],  0);
    err |= check("m2.aw", dut.aw_count[2], 5);
    err |= check("m2.w",  dut.w_count[2],  5);
    err |= check("m2.b",  dut.b_count[2],  5);

    if (err) return 1;
    std::printf("PASS Nic400Pmu: per-master AR/AW/R/W/B counters integrate event pulses correctly\n");
    return 0;
}
