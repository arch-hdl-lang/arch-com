// 3-stage credit_channel chain TB. Demonstrates that backpressure
// composes through multiple credit_channel hops: when the consumer
// slows, all 3 router buffers fill and producer-side can_send falls.

#define private public
#include "VNocChainTop.h"
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

static VNocChainTop dut;

static void tick() {
    dut.clk = 0; dut.eval();
    dut.clk = 1; dut.eval();
}

static void reset_and_idle() {
    dut.rst = 1;
    dut.gen_pressure = 0;
    dut.pop_pressure = 0;
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
    printf("=== NoC chain credit_channel TB (3-router pipeline) ===\n");

    reset_and_idle();

    {
        uint32_t popped = run_window(0, 0, 200);
        CHECK(popped == 0, "idle: 200 cycles produce 0 pops (got %u)", popped);
    }

    // Balanced flow: both at 128 → ~half the cycles produce/consume.
    // With 3-deep pipeline, expect O(few hundred) flits in 1000 cycles
    // minus a few cycles of fill latency.
    {
        uint32_t popped = run_window(128, 128, 1000);
        CHECK(popped >= 200,
              "balanced (gen=128, pop=128, 1000 cyc): popped %u >= 200", popped);
        CHECK(dut.in_order, "in_order true after balanced");
        printf("  info: balanced popped=%u, last_seq=%u\n", popped, dut.last_seq);
    }

    // Backpressure through 3 routers: producer fast, consumer slow.
    // All 3 router buffers + producer's credit (DEPTH=4 each = 16 total
    // in-flight slots) must drain through the consumer's slow rate.
    {
        uint32_t popped = run_window(255, 16, 4000);
        CHECK(popped > 100,
              "backpressure (gen=255, pop=16, 4000 cyc): popped %u > 100", popped);
        CHECK(popped < 800,
              "backpressure: popped %u < 800 (consumer-bounded)", popped);
        CHECK(dut.in_order, "in_order true after backpressure");
        printf("  info: backpressure popped=%u\n", popped);
    }

    // Recovery: consumer speeds back up. The pipeline should drain
    // and resume full-throughput steady state.
    {
        uint32_t popped = run_window(255, 255, 1000);
        CHECK(popped >= 300,
              "recovery (gen=255, pop=255, 1000 cyc): popped %u >= 300", popped);
        CHECK(dut.in_order, "in_order true after recovery");
        printf("  info: recovery popped=%u (cumulative %u)\n",
               popped, dut.popped_count);
    }

    // Final invariant: every flit popped exactly once, in order.
    if (dut.saw_any) {
        CHECK(dut.last_seq == dut.popped_count - 1,
              "monotonic: last_seq=%u == popped_count-1=%u",
              dut.last_seq, dut.popped_count - 1);
    }

    printf("\n=== Summary: %d pass, %d fail ===\n", pass_count, fail_count);
    return fail_count == 0 ? 0 : 1;
}
