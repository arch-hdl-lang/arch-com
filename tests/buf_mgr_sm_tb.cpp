// BufMgrSm testbench — small variant (16 entries x 32b, 4 queues)

#include "VBufMgrSm.h"
#include <cstdio>
#include <cassert>
#include <cstring>

static int pass_count = 0;
static int fail_count = 0;

#define CHECK(cond, msg, ...) \
  do { \
    if (cond) { printf("  PASS: " msg "\n", ##__VA_ARGS__); ++pass_count; } \
    else { printf("  FAIL: " msg "\n", ##__VA_ARGS__); ++fail_count; } \
  } while(0)

static VBufMgrSm dut;

static void tick() {
    dut.clk = 0; dut.eval();
    dut.clk = 1; dut.eval();
}

static void reset() {
    dut.rst = 1;
    dut.enqueue_valid = 0;
    dut.dequeue_valid = 0;
    tick(); tick();
    dut.rst = 0;
}

static void wait_init() {
    for (int i = 0; i < 100; i++) {
        tick();
        if (dut.init_done) return;
    }
    printf("  FAIL: init_done never asserted\n");
    ++fail_count;
}

static void enqueue(uint8_t qn, uint32_t data) {
    dut.enqueue_valid = 1;
    dut.enqueue_queue_number = qn;
    dut.enqueue_data = data;
    tick();
    dut.enqueue_valid = 0;
}

// Issue dequeue, wait for pipeline (1 more cycle for sync SRAM), return data
static uint32_t dequeue(uint8_t qn) {
    dut.dequeue_valid = 1;
    dut.dequeue_queue_number = qn;
    tick();  // DQ0: issue SRAM reads, set dq1_valid
    dut.dequeue_valid = 0;
    tick();  // DQ1: dq2_valid set, SRAM data available (1-cycle sync)
    if (!dut.dequeue_resp_valid) {
        printf("  FAIL: dequeue_resp_valid not set for queue %u\n", qn);
        ++fail_count;
        return 0xFFFFFFFF;
    }
    return dut.dequeue_data;
}

static void idle(int n) {
    dut.enqueue_valid = 0;
    dut.dequeue_valid = 0;
    for (int i = 0; i < n; i++) tick();
}

int main() {
    memset(&dut, 0, sizeof(dut));

    // ── Test 1: Reset + Init ──
    printf("[Test 1] Reset + Init\n");
    reset();
    CHECK(dut.init_done == 0, "init_done==%u expected 0 after reset", dut.init_done);

    wait_init();
    CHECK(dut.init_done == 1, "init_done==%u expected 1 after init", dut.init_done);
    CHECK(dut.free_count_out == 16, "free_count==%u expected 16 after init", dut.free_count_out);

    // ── Test 2: Single enqueue + dequeue (queue 0) ──
    printf("[Test 2] Single enqueue/dequeue queue 0\n");
    printf("  enqueue(q=0, data=0xDEAD)\n");
    enqueue(0, 0xDEAD);
    printf("  free_count=%u after enqueue (expected 15)\n", dut.free_count_out);
    idle(4);

    uint32_t v = dequeue(0);
    CHECK(v == 0xDEAD, "dequeue(q=0): got 0x%08X expected 0x0000DEAD", v);
    printf("  free_count=%u after dequeue (expected 16)\n", dut.free_count_out);
    idle(4);

    // ── Test 3: Multi-queue ──
    printf("[Test 3] Multi-queue: enqueue to q0,q1,q2 then dequeue each\n");
    printf("  enqueue(q=0, 0x100), enqueue(q=1, 0x200), enqueue(q=2, 0x300)\n");
    enqueue(0, 0x100);
    enqueue(1, 0x200);
    enqueue(2, 0x300);
    printf("  free_count=%u after 3 enqueues (expected 13)\n", dut.free_count_out);
    idle(6);

    v = dequeue(0);
    CHECK(v == 0x100, "dequeue(q=0): got 0x%08X expected 0x00000100", v);
    idle(2);
    v = dequeue(1);
    CHECK(v == 0x200, "dequeue(q=1): got 0x%08X expected 0x00000200", v);
    idle(2);
    v = dequeue(2);
    CHECK(v == 0x300, "dequeue(q=2): got 0x%08X expected 0x00000300", v);
    printf("  free_count=%u after 3 dequeues (expected 16)\n", dut.free_count_out);
    idle(4);

    // ── Test 4: FIFO order: 4 items to queue 3 ──
    printf("[Test 4] FIFO order: 4 items to queue 3\n");
    printf("  enqueue(q=3, 0xA0..0xA3)\n");
    enqueue(3, 0xA0);
    enqueue(3, 0xA1);
    enqueue(3, 0xA2);
    enqueue(3, 0xA3);
    printf("  free_count=%u after 4 enqueues (expected 12)\n", dut.free_count_out);
    idle(6);

    for (uint32_t i = 0; i < 4; i++) {
        v = dequeue(3);
        CHECK(v == 0xA0 + i, "dequeue(q=3)[%u]: got 0x%08X expected 0x%08X", i, v, 0xA0 + i);
        idle(2);
    }
    printf("  free_count=%u after 4 dequeues (expected 16)\n", dut.free_count_out);

    // ── Test 5: Back-to-back enqueue same queue (enqueue_valid held high) ──
    printf("[Test 5] Back-to-back enqueue q1 (valid held high 3 cycles)\n");
    dut.enqueue_valid = 1;
    dut.enqueue_queue_number = 1;
    dut.enqueue_data = 0xB0;
    printf("  cycle 1: data=0xB0\n");
    tick();
    dut.enqueue_data = 0xB1;
    printf("  cycle 2: data=0xB1\n");
    tick();
    dut.enqueue_data = 0xB2;
    printf("  cycle 3: data=0xB2\n");
    tick();
    dut.enqueue_valid = 0;
    printf("  free_count=%u after 3 b2b enqueues (expected 13)\n", dut.free_count_out);
    idle(6);

    v = dequeue(1);
    CHECK(v == 0xB0, "dequeue(q=1)[0]: got 0x%08X expected 0x000000B0", v);
    idle(2);
    v = dequeue(1);
    CHECK(v == 0xB1, "dequeue(q=1)[1]: got 0x%08X expected 0x000000B1", v);
    idle(2);
    v = dequeue(1);
    CHECK(v == 0xB2, "dequeue(q=1)[2]: got 0x%08X expected 0x000000B2", v);
    printf("  free_count=%u after 3 dequeues (expected 16)\n", dut.free_count_out);
    idle(4);

    // ── Test 6: Simultaneous enqueue + dequeue (different queues) ──
    printf("[Test 6] Simultaneous enqueue(q=3) + dequeue(q=2)\n");
    printf("  setup: enqueue(q=2, 0xC0)\n");
    enqueue(2, 0xC0);
    idle(6);

    printf("  simultaneous: enqueue(q=3, 0xC1) + dequeue(q=2)\n");
    dut.enqueue_valid = 1;
    dut.enqueue_queue_number = 3;
    dut.enqueue_data = 0xC1;
    dut.dequeue_valid = 1;
    dut.dequeue_queue_number = 2;
    tick();
    dut.enqueue_valid = 0;
    dut.dequeue_valid = 0;
    tick(); // DQ1: resp should be valid now (1-cycle sync SRAM)
    CHECK(dut.dequeue_resp_valid == 1, "simul deq resp_valid==%u expected 1", dut.dequeue_resp_valid);
    CHECK(dut.dequeue_data == 0xC0, "simul deq(q=2): got 0x%08X expected 0x000000C0", dut.dequeue_data);
    idle(6);

    v = dequeue(3);
    CHECK(v == 0xC1, "dequeue(q=3) after simul enq: got 0x%08X expected 0x000000C1", v);
    printf("  free_count=%u after simul test (expected 16)\n", dut.free_count_out);

    // ── Summary ──
    printf("\nBufMgrSm: %d/%d tests passed\n", pass_count, pass_count + fail_count);
    return fail_count != 0 ? 1 : 0;
}
