// NoC credit_channel testbench — see doc/plan_credit_channel.md §"Validation
// plan — NoC flit credit". Exercises the producer/consumer pair under
// balanced, producer-fast, and recovery scenarios. Self-checks for
// in-order delivery + monotonic seq_no progress.

#define private public
#include "VNocCreditTop.h"
#undef private
#include <cstdio>
#include <cstdint>
#include <cassert>

static int pass_count = 0;
static int fail_count = 0;

#define CHECK(cond, msg, ...) \
  do { \
    if (cond) { printf("  PASS: " msg "\n", ##__VA_ARGS__); ++pass_count; } \
    else { printf("  FAIL: " msg "\n", ##__VA_ARGS__); ++fail_count; } \
  } while (0)

static VNocCreditTop dut;

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

static uint64_t run_window(uint8_t gen, uint8_t pop, int cycles) {
    dut.gen_pressure = gen;
    dut.pop_pressure = pop;
    uint64_t before = dut.popped_count;
    for (int i = 0; i < cycles; ++i) tick();
    return dut.popped_count - before;
}

int main() {
    printf("=== NoC credit_channel TB ===\n");

    reset_and_idle();

    // Sanity 1: idle (gen=0, pop=0) → no progress over 200 cycles.
    {
        uint64_t popped = run_window(0, 0, 200);
        CHECK(popped == 0, "idle: 200 cycles produce 0 pops (got %llu)",
              (unsigned long long)popped);
        CHECK(dut.in_order, "in_order still true after idle");
    }

    // Scenario 1: balanced 50/50 — both pressures at 128, 1000 cycles.
    // LFSR < 128 ≈ half the time; producer + consumer both fire ~half-cycles.
    // Expected throughput is bounded by the slower side; with both at 128
    // we should see a few hundred flits.
    {
        uint64_t popped = run_window(128, 128, 1000);
        CHECK(popped >= 200,
              "balanced (gen=128, pop=128, 1000 cyc): popped %llu >= 200",
              (unsigned long long)popped);
        CHECK(dut.in_order, "in_order still true after balanced run");
        printf("  info: balanced popped=%llu, last_seq=%llu\n",
               (unsigned long long)popped,
               (unsigned long long)dut.last_seq);
    }

    // Scenario 2: producer fast / consumer slow — credit drains, sender
    // backpressures naturally via can_send. Throughput is bounded by the
    // consumer's pop rate.
    {
        uint64_t popped = run_window(255, 32, 2000);
        CHECK(popped > 100,
              "backpressure (gen=255, pop=32, 2000 cyc): popped %llu > 100",
              (unsigned long long)popped);
        // Consumer-bounded: ~32/255 of 2000 ≈ 250 pops max. Real number is
        // smaller because LFSR<32 ≈ 12.5% of the time. Loose upper bound:
        CHECK(popped < 600,
              "backpressure: popped %llu < 600 (consumer-bounded)",
              (unsigned long long)popped);
        CHECK(dut.in_order, "in_order still true after backpressure run");
        printf("  info: backpressure popped=%llu\n",
               (unsigned long long)popped);
    }

    // Scenario 3: recovery — speed consumer back up to 255, producer
    // already at 255. Drained credits should refill and throughput rises.
    uint64_t recovery_baseline = dut.popped_count;
    {
        uint64_t popped = run_window(255, 255, 1000);
        CHECK(popped >= 300,
              "recovery (gen=255, pop=255, 1000 cyc): popped %llu >= 300",
              (unsigned long long)popped);
        CHECK(dut.in_order, "in_order still true after recovery run");
        printf("  info: recovery popped=%llu (cumulative %llu)\n",
               (unsigned long long)popped,
               (unsigned long long)(dut.popped_count - 0));
    }
    (void)recovery_baseline;

    // Final invariant: every popped flit's seq_no incremented by 1, so
    // last_seq == popped_count - 1 (when saw_any is set).
    if (dut.saw_any) {
        CHECK(dut.last_seq == dut.popped_count - 1,
              "monotonic: last_seq=%llu == popped_count-1=%llu",
              (unsigned long long)dut.last_seq,
              (unsigned long long)(dut.popped_count - 1));
    }

    printf("\n=== Summary: %d pass, %d fail ===\n", pass_count, fail_count);
    return fail_count == 0 ? 0 : 1;
}
