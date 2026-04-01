#include "VFsmMm2s.h"
#include <cstdio>
#include <cstdlib>

static VFsmMm2s dut;
static int cycle_count = 0;

static void tick() {
    dut.clk = 0; dut.eval();
    dut.clk = 1; dut.eval();
    cycle_count++;
}

static void reset() {
    dut.rst = 1;
    dut.start = 0;
    dut.src_addr = 0;
    dut.num_beats = 0;
    dut.axi_rd_ar_ready = 0;
    dut.axi_rd_r_valid = 0;
    dut.axi_rd_r_data = 0;
    dut.axi_rd_r_last = 0;
    dut.push_ready = 1;
    tick(); tick();
    dut.rst = 0;
    tick();
    cycle_count = 0;
}

#define ASSERT_EQ(a, b, msg) do { \
    if ((a) != (b)) { \
        printf("FAIL %s: got=0x%x exp=0x%x at cycle %d\n", msg, (unsigned)(a), (unsigned)(b), cycle_count); \
        exit(1); \
    } \
} while(0)

// Test 1: Basic MM2S transfer — 4 beats from address 0x1000
static void test_basic_transfer() {
    reset();

    // Initially halted
    ASSERT_EQ(dut.halted, 1, "halted at idle");
    ASSERT_EQ(dut.done, 0, "done at idle");
    ASSERT_EQ(dut.axi_rd_ar_valid, 0, "ar_valid at idle");

    // Start transfer: 4 beats from 0x1000
    dut.start = 1;
    dut.src_addr = 0x1000;
    dut.num_beats = 4;
    tick();
    dut.start = 0;

    // Should now be in SendAR — check after eval
    dut.eval();
    ASSERT_EQ(dut.axi_rd_ar_valid, 1, "ar_valid in SendAR");
    ASSERT_EQ(dut.axi_rd_ar_addr, 0x1000u, "ar_addr");
    ASSERT_EQ(dut.axi_rd_ar_len, 3u, "ar_len (4-1=3)");
    ASSERT_EQ(dut.axi_rd_ar_size, 2u, "ar_size (4 bytes)");
    ASSERT_EQ(dut.axi_rd_ar_burst, 1u, "ar_burst (INCR)");
    ASSERT_EQ(dut.halted, 0, "not halted in SendAR");

    // Accept AR
    dut.axi_rd_ar_ready = 1;
    tick();
    dut.axi_rd_ar_ready = 0;

    // Should now be in WaitR — send 4 R beats
    uint32_t expected[4] = {0xDEAD0000, 0xDEAD0001, 0xDEAD0002, 0xDEAD0003};
    for (int i = 0; i < 4; i++) {
        dut.axi_rd_r_valid = 1;
        dut.axi_rd_r_data = expected[i];
        dut.axi_rd_r_last = (i == 3) ? 1 : 0;
        dut.push_ready = 1;
        dut.eval(); // propagate comb: r_ready = push_ready, push_valid = r_valid

        ASSERT_EQ(dut.axi_rd_r_ready, 1, "r_ready when push_ready");
        ASSERT_EQ(dut.push_valid, 1, "push_valid when r_valid");
        ASSERT_EQ(dut.push_data, expected[i], "push_data matches r_data");
        tick();
    }
    dut.axi_rd_r_valid = 0;
    dut.axi_rd_r_last = 0;

    // Should now be in Done
    dut.eval();
    ASSERT_EQ(dut.done, 1, "done pulses");
    tick();

    // Back to Idle
    dut.eval();
    ASSERT_EQ(dut.done, 0, "done clears");
    ASSERT_EQ(dut.halted, 1, "halted after done");

    printf("Test 1 PASS: basic 4-beat MM2S transfer\n");
}

// Test 2: Back-pressure — push_ready goes low mid-transfer
static void test_backpressure() {
    reset();

    dut.start = 1;
    dut.src_addr = 0x2000;
    dut.num_beats = 2;
    tick();
    dut.start = 0;

    // Accept AR
    dut.axi_rd_ar_ready = 1;
    tick();
    dut.axi_rd_ar_ready = 0;

    // Beat 0: normal
    dut.axi_rd_r_valid = 1;
    dut.axi_rd_r_data = 0xAAAA;
    dut.axi_rd_r_last = 0;
    dut.push_ready = 1;
    dut.eval();
    ASSERT_EQ(dut.axi_rd_r_ready, 1, "r_ready when push_ready=1");
    tick();

    // Beat 1: push_ready goes low — should stall
    dut.axi_rd_r_data = 0xBBBB;
    dut.axi_rd_r_last = 1;
    dut.push_ready = 0;
    dut.eval();
    ASSERT_EQ(dut.axi_rd_r_ready, 0, "r_ready drops when push_ready=0");
    tick();

    // Still in WaitR, not Done yet (last beat not pushed)
    dut.eval();
    ASSERT_EQ(dut.done, 0, "done=0 while stalled");

    // Resume push_ready
    dut.push_ready = 1;
    dut.eval();
    ASSERT_EQ(dut.axi_rd_r_ready, 1, "r_ready resumes");
    ASSERT_EQ(dut.push_valid, 1, "push_valid when r_valid");
    ASSERT_EQ(dut.push_data, 0xBBBBu, "push_data correct after stall");
    tick();

    // Now Done
    dut.eval();
    ASSERT_EQ(dut.done, 1, "done after stall resolved");
    tick();
    dut.eval();
    ASSERT_EQ(dut.halted, 1, "halted after done");

    printf("Test 2 PASS: back-pressure stalls transfer\n");
}

// Test 3: idle_out signal
static void test_idle_signal() {
    reset();

    ASSERT_EQ(dut.idle_out, 1, "idle at reset");

    dut.start = 1;
    dut.src_addr = 0;
    dut.num_beats = 1;
    tick();
    dut.start = 0;
    dut.eval();

    ASSERT_EQ(dut.idle_out, 0, "not idle during transfer");

    // Complete the transfer quickly
    dut.axi_rd_ar_ready = 1;
    tick();
    dut.axi_rd_ar_ready = 0;

    dut.axi_rd_r_valid = 1;
    dut.axi_rd_r_data = 0x12;
    dut.axi_rd_r_last = 1;
    dut.push_ready = 1;
    tick();
    dut.axi_rd_r_valid = 0;

    // Done state
    dut.eval();
    ASSERT_EQ(dut.done, 1, "done asserted");
    tick();

    // Back to Idle
    dut.eval();
    ASSERT_EQ(dut.idle_out, 1, "idle after return");

    printf("Test 3 PASS: idle_out signal\n");
}

int main() {
    test_basic_transfer();
    test_backpressure();
    test_idle_signal();
    printf("PASS\n");
    return 0;
}
