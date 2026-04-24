// 4×4 mesh NoC testbench. Producer at (0,0).local, Consumer at
// (3,3).local. The TB sets dst_x=3, dst_y=3 so every flit traverses
// the full mesh diagonal: 3 hops east through (1,0), (2,0), (3,0)
// then 3 hops north through (3,1), (3,2), (3,3). XY routing turns
// once at (3,0).

#define private public
#include "VMesh4x4.h"
#undef private
#include <cstdio>
#include <cstdint>

static int pass_count = 0;
static int fail_count = 0;

#define CHECK(cond, msg, ...) \
  do { \
    if (cond) { printf("  PASS: " msg "\n", ##__VA_ARGS__); ++pass_count; } \
    else { printf("  FAIL: " msg "\n", ##__VA_ARGS__); ++fail_count; } \
  } while (0)

static VMesh4x4 dut;

static void tick() {
    dut.clk = 0; dut.eval();
    dut.clk = 1; dut.eval();
}

static void reset_and_idle() {
    dut.rst = 1;
    dut.gen_pressure = 0;
    dut.pop_pressure = 0;
    dut.dst_x = 3;
    dut.dst_y = 3;
    tick(); tick(); tick();
    dut.rst = 0;
    tick();
}

static uint32_t run_window(uint8_t gen, uint8_t pop, int cycles) {
    dut.gen_pressure = gen;
    dut.pop_pressure = pop;
    uint32_t before = dut.popped_count;
    for (int i = 0; i < cycles; ++i) tick();
    return dut.popped_count - before;
}

int main() {
    printf("=== 4x4 mesh NoC TB ===\n");

    reset_and_idle();

    {
        uint32_t popped = run_window(0, 0, 200);
        CHECK(popped == 0, "idle: 200 cycles produce 0 pops (got %u)", popped);
    }

    // Balanced — 6-hop diagonal traversal incurs latency before the
    // first flit lands at (3,3); allow ample cycles.
    {
        uint32_t popped = run_window(128, 128, 4000);
        CHECK(popped >= 200,
              "balanced (gen=128, pop=128, 4000 cyc): popped %u >= 200", popped);
        printf("  info: balanced popped=%u, last_payload=%u\n",
               popped, dut.last_payload);
    }

    // Backpressure: producer fast, consumer slow. Per-router buffers
    // (DEPTH=4) along the path absorb in-flight flits, then producer
    // back-pressures via can_send.
    {
        uint32_t popped = run_window(255, 32, 4000);
        CHECK(popped > 50,
              "backpressure (gen=255, pop=32, 4000 cyc): popped %u > 50", popped);
        printf("  info: backpressure popped=%u\n", popped);
    }

    // Recovery: pop pressure back to max, dst still (3,3). Pipeline
    // drains and steady-state throughput resumes.
    {
        uint32_t popped = run_window(255, 255, 2000);
        CHECK(popped >= 500,
              "recovery (gen=255, pop=255, 2000 cyc): popped %u >= 500", popped);
        printf("  info: recovery popped=%u (cumulative %u)\n",
               popped, dut.popped_count);
    }

    // Routing topology check: reset, then send to (3,3) cleanly. Verify
    // first-flit latency reflects the 6-hop diagonal (3 east + 3 north).
    reset_and_idle();
    dut.dst_x = 3; dut.dst_y = 3;
    dut.gen_pressure = 255;
    dut.pop_pressure = 255;
    int latency = -1;
    for (int i = 0; i < 200; ++i) {
        tick();
        if (dut.popped_count > 0) { latency = i + 1; break; }
    }
    CHECK(latency >= 6 && latency < 30,
          "first-flit latency for 6-hop diagonal: %d cycles (expected 6..30)",
          latency);

    printf("\n=== Summary: %d pass, %d fail ===\n", pass_count, fail_count);
    return fail_count == 0 ? 0 : 1;
}
