#include "VFsmS2mm.h"
#include <cstdio>
#include <cstdlib>

static VFsmS2mm dut;
static int cycle_count = 0;

static void tick() {
    dut.clk = 0; dut.eval();
    dut.clk = 1; dut.eval();
    cycle_count++;
}

static void reset() {
    dut.rst = 1;
    dut.start = 0;
    dut.dst_addr = 0;
    dut.num_beats = 0;
    dut.recv_count = 0;
    dut.pop_valid = 0;
    dut.pop_data = 0;
    dut.axi_wr_aw_ready = 0;
    dut.axi_wr_w_ready = 0;
    dut.axi_wr_b_valid = 0;
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

// Test 1: Basic S2MM transfer — 4 beats to address 0x2000
static void test_basic_transfer() {
    reset();

    ASSERT_EQ(dut.halted, 1, "halted at idle");
    ASSERT_EQ(dut.done, 0, "done at idle");

    // Start transfer
    dut.start = 1;
    dut.dst_addr = 0x2000;
    dut.num_beats = 4;
    tick();
    dut.start = 0;

    // Now in WaitRecv — recv_count < num_beats, waiting for stream data
    dut.eval();
    ASSERT_EQ(dut.halted, 0, "not halted in WaitRecv");
    ASSERT_EQ(dut.axi_wr_aw_valid, 0, "aw_valid=0 in WaitRecv");

    // Simulate: all 4 beats received into FIFO
    dut.recv_count = 4;
    tick(); // transition to SendAW

    // Now in SendAW
    dut.eval();
    ASSERT_EQ(dut.axi_wr_aw_valid, 1, "aw_valid in SendAW");
    ASSERT_EQ(dut.axi_wr_aw_addr, 0x2000u, "aw_addr");
    ASSERT_EQ(dut.axi_wr_aw_len, 3u, "aw_len (4-1=3)");
    ASSERT_EQ(dut.axi_wr_aw_size, 2u, "aw_size");
    ASSERT_EQ(dut.axi_wr_aw_burst, 1u, "aw_burst (INCR)");

    // Accept AW
    dut.axi_wr_aw_ready = 1;
    tick();
    dut.axi_wr_aw_ready = 0;

    // Now in SendW — drive 4 beats from FIFO pop
    uint32_t data[4] = {0xBEEF0000, 0xBEEF0001, 0xBEEF0002, 0xBEEF0003};
    for (int i = 0; i < 4; i++) {
        dut.pop_valid = 1;
        dut.pop_data = data[i];
        dut.axi_wr_w_ready = 1;
        dut.eval();

        ASSERT_EQ(dut.axi_wr_w_valid, 1, "w_valid in SendW");
        ASSERT_EQ(dut.axi_wr_w_data, data[i], "w_data matches pop_data");
        ASSERT_EQ(dut.axi_wr_w_strb, 0xFu, "w_strb all bytes");
        ASSERT_EQ(dut.pop_ready, 1, "pop_ready = w_ready");

        if (i == 3) {
            ASSERT_EQ(dut.axi_wr_w_last, 1, "w_last on beat 3");
        } else {
            ASSERT_EQ(dut.axi_wr_w_last, 0, "w_last=0 before last beat");
        }
        tick();
    }
    dut.pop_valid = 0;
    dut.axi_wr_w_ready = 0;

    // Now in WaitB
    dut.eval();
    ASSERT_EQ(dut.axi_wr_b_ready, 1, "b_ready in WaitB");

    // Send B response
    dut.axi_wr_b_valid = 1;
    tick();
    dut.axi_wr_b_valid = 0;

    // Done
    dut.eval();
    ASSERT_EQ(dut.done, 1, "done pulses");
    tick();

    // Back to Idle
    dut.eval();
    ASSERT_EQ(dut.halted, 1, "halted after done");
    ASSERT_EQ(dut.done, 0, "done clears");

    printf("Test 1 PASS: basic 4-beat S2MM transfer\n");
}

// Test 2: Status signals
static void test_status_signals() {
    reset();

    ASSERT_EQ(dut.idle_out, 1, "idle at reset");
    ASSERT_EQ(dut.halted, 1, "halted at reset");

    dut.start = 1;
    dut.dst_addr = 0;
    dut.num_beats = 1;
    tick();
    dut.start = 0;
    dut.eval();

    ASSERT_EQ(dut.idle_out, 0, "not idle during transfer");
    ASSERT_EQ(dut.halted, 0, "not halted during transfer");

    // Complete transfer quickly
    dut.recv_count = 1;
    tick();
    dut.axi_wr_aw_ready = 1;
    tick();
    dut.axi_wr_aw_ready = 0;
    dut.pop_valid = 1;
    dut.pop_data = 0x42;
    dut.axi_wr_w_ready = 1;
    tick();
    dut.pop_valid = 0;
    dut.axi_wr_w_ready = 0;
    dut.axi_wr_b_valid = 1;
    tick();
    dut.axi_wr_b_valid = 0;
    // Done
    tick();
    dut.eval();
    ASSERT_EQ(dut.idle_out, 1, "idle after return");

    printf("Test 2 PASS: status signals\n");
}

int main() {
    test_basic_transfer();
    test_status_signals();
    printf("PASS\n");
    return 0;
}
