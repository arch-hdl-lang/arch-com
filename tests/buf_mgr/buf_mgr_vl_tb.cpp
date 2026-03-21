// BufMgr Verilator testbench — verifies split SV files compile and simulate correctly
#include "VBufMgr.h"
#include "verilated.h"
#include <cstdio>
#include <cstring>
#include <memory>

static int pass_count = 0;
static int fail_count = 0;

#define CHECK(cond, msg, ...) \
  do { \
    if (cond) { printf("  PASS: " msg "\n", ##__VA_ARGS__); ++pass_count; } \
    else { printf("  FAIL: " msg "\n", ##__VA_ARGS__); ++fail_count; } \
  } while(0)

static std::unique_ptr<VBufMgr> dut;

static void tick() {
    dut->clk = 0; dut->eval();
    dut->clk = 1; dut->eval();
}

static void reset() {
    dut->rst = 1;
    dut->enqueue_valid = 0;
    dut->dequeue_valid = 0;
    tick(); tick();
    dut->rst = 0;
}

static void wait_init() {
    for (int i = 0; i < 20000; i++) {
        tick();
        if (dut->init_done) {
            printf("  init_done after %d cycles\n", i + 1);
            return;
        }
    }
    printf("  FAIL: init_done never asserted after 20000 cycles\n");
    ++fail_count;
}

static void set_data(uint32_t lo) {
    // enqueue_data is 128 bits — set low word, zero rest
    dut->enqueue_data[0] = lo;
    dut->enqueue_data[1] = 0;
    dut->enqueue_data[2] = 0;
    dut->enqueue_data[3] = 0;
}

static uint32_t get_deq_data() {
    return dut->dequeue_data[0];
}

static void enqueue(uint8_t qn, uint32_t data_lo) {
    dut->enqueue_valid = 1;
    dut->enqueue_queue_number = qn;
    set_data(data_lo);
    tick();
    dut->enqueue_valid = 0;
}

static uint32_t dequeue(uint8_t qn) {
    dut->dequeue_valid = 1;
    dut->dequeue_queue_number = qn;
    tick();  // DQ0
    dut->dequeue_valid = 0;
    tick();  // DQ1->DQ2
    if (!dut->dequeue_resp_valid) {
        printf("  FAIL: dequeue_resp_valid not set for queue %u\n", qn);
        ++fail_count;
        return 0xFFFFFFFF;
    }
    return get_deq_data();
}

static void idle(int n) {
    dut->enqueue_valid = 0;
    dut->dequeue_valid = 0;
    for (int i = 0; i < n; i++) tick();
}

int main(int argc, char** argv) {
    Verilated::commandArgs(argc, argv);
    dut = std::make_unique<VBufMgr>();

    // ── Test 1: Reset + Init ──
    printf("[Test 1] Reset + Init\n");
    reset();
    CHECK(dut->init_done == 0, "init_done==%u expected 0 after reset", (unsigned)dut->init_done);

    wait_init();
    CHECK(dut->init_done == 1, "init_done==%u expected 1", (unsigned)dut->init_done);
    CHECK(dut->free_count_out == 16384, "free_count==%u expected 16384", (unsigned)dut->free_count_out);

    // ── Test 2: Single enqueue + dequeue (queue 0) ──
    printf("[Test 2] Single enqueue/dequeue queue 0\n");
    enqueue(0, 0xDEADBEEF);
    idle(4);

    uint32_t v = dequeue(0);
    CHECK(v == 0xDEADBEEF, "dequeue(q=0): got 0x%08X expected 0xDEADBEEF", v);
    idle(4);

    // ── Test 3: Multi-queue ──
    printf("[Test 3] Multi-queue: enqueue to q0,q1,q2 then dequeue each\n");
    enqueue(0, 0x100);
    enqueue(1, 0x200);
    enqueue(2, 0x300);
    idle(6);

    v = dequeue(0);
    CHECK(v == 0x100, "dequeue(q=0): got 0x%08X expected 0x00000100", v);
    idle(2);
    v = dequeue(1);
    CHECK(v == 0x200, "dequeue(q=1): got 0x%08X expected 0x00000200", v);
    idle(2);
    v = dequeue(2);
    CHECK(v == 0x300, "dequeue(q=2): got 0x%08X expected 0x00000300", v);
    idle(4);

    // ── Test 4: FIFO order: 4 items to queue 5 ──
    printf("[Test 4] FIFO order: 4 items to queue 5\n");
    enqueue(5, 0xA0);
    enqueue(5, 0xA1);
    enqueue(5, 0xA2);
    enqueue(5, 0xA3);
    idle(6);

    for (uint32_t i = 0; i < 4; i++) {
        v = dequeue(5);
        CHECK(v == 0xA0 + i, "dequeue(q=5)[%u]: got 0x%08X expected 0x%08X", i, v, 0xA0 + i);
        idle(2);
    }

    // ── Test 5: Back-to-back enqueue q10 ──
    printf("[Test 5] Back-to-back enqueue q10\n");
    dut->enqueue_valid = 1;
    dut->enqueue_queue_number = 10;
    set_data(0xB0); tick();
    set_data(0xB1); tick();
    set_data(0xB2); tick();
    dut->enqueue_valid = 0;
    idle(6);

    v = dequeue(10);
    CHECK(v == 0xB0, "dequeue(q=10)[0]: got 0x%08X expected 0x000000B0", v);
    idle(2);
    v = dequeue(10);
    CHECK(v == 0xB1, "dequeue(q=10)[1]: got 0x%08X expected 0x000000B1", v);
    idle(2);
    v = dequeue(10);
    CHECK(v == 0xB2, "dequeue(q=10)[2]: got 0x%08X expected 0x000000B2", v);
    idle(4);

    // ── Test 6: Simultaneous enqueue + dequeue ──
    printf("[Test 6] Simultaneous enqueue(q=21) + dequeue(q=20)\n");
    enqueue(20, 0xC0);
    idle(6);

    dut->enqueue_valid = 1;
    dut->enqueue_queue_number = 21;
    set_data(0xC1);
    dut->dequeue_valid = 1;
    dut->dequeue_queue_number = 20;
    tick();
    dut->enqueue_valid = 0;
    dut->dequeue_valid = 0;
    tick();
    CHECK(dut->dequeue_resp_valid == 1, "simul deq resp_valid==%u expected 1", (unsigned)dut->dequeue_resp_valid);
    CHECK(get_deq_data() == 0xC0, "simul deq(q=20): got 0x%08X expected 0x000000C0", get_deq_data());
    idle(6);

    v = dequeue(21);
    CHECK(v == 0xC1, "dequeue(q=21) after simul enq: got 0x%08X expected 0x000000C1", v);

    // ── Test 7: High queue numbers ──
    printf("[Test 7] High queue numbers: q=255, q=128\n");
    enqueue(255, 0xFF00);
    enqueue(128, 0x8000);
    idle(6);

    v = dequeue(255);
    CHECK(v == 0xFF00, "dequeue(q=255): got 0x%08X expected 0x0000FF00", v);
    idle(2);
    v = dequeue(128);
    CHECK(v == 0x8000, "dequeue(q=128): got 0x%08X expected 0x00008000", v);

    // ── Summary ──
    printf("\nBufMgr Verilator: %d/%d tests passed\n", pass_count, pass_count + fail_count);

    dut->final();
    return fail_count != 0 ? 1 : 0;
}
