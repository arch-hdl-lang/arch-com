#include "VMm2sFifo.h"
#include <cstdio>
#include <cstdlib>

static VMm2sFifo dut;
static int cycle_count = 0;

static void tick() {
    dut.clk = 0; dut.eval();
    dut.clk = 1; dut.eval();
    cycle_count++;
}

static void reset() {
    dut.rst = 1;
    dut.push_valid = 0;
    dut.push_data = 0;
    dut.pop_ready = 0;
    tick(); tick();
    dut.rst = 0;
    tick();
}

#define ASSERT_EQ(a, b, msg) do { \
    if ((a) != (b)) { \
        printf("FAIL %s: got=%u exp=%u at cycle %d\n", msg, (unsigned)(a), (unsigned)(b), cycle_count); \
        exit(1); \
    } \
} while(0)

// Test 1: push 4 items, pop 4, verify FIFO order
static void test_push_pop_order() {
    reset();

    // Push 4 items
    for (int i = 0; i < 4; i++) {
        dut.push_valid = 1;
        dut.push_data = 0xA0 + i;
        dut.pop_ready = 0;
        ASSERT_EQ(dut.push_ready, 1, "push_ready during push");
        tick();
    }
    dut.push_valid = 0;
    tick(); // let state settle

    // Pop 4 items and verify order
    for (int i = 0; i < 4; i++) {
        dut.pop_ready = 1;
        ASSERT_EQ(dut.pop_valid, 1, "pop_valid during pop");
        ASSERT_EQ(dut.pop_data, (uint32_t)(0xA0 + i), "pop_data order");
        tick();
    }
    dut.pop_ready = 0;
    tick();

    // FIFO should be empty now
    ASSERT_EQ(dut.empty, 1, "empty after drain");
    ASSERT_EQ(dut.pop_valid, 0, "pop_valid when empty");

    printf("Test 1 PASS: push/pop FIFO order correct\n");
}

// Test 2: push until full
static void test_push_until_full() {
    reset();

    dut.pop_ready = 0;
    for (int i = 0; i < 16; i++) {
        dut.push_valid = 1;
        dut.push_data = i;
        ASSERT_EQ(dut.push_ready, 1, "push_ready before full");
        tick();
    }
    // Now FIFO should be full (depth=16)
    ASSERT_EQ(dut.full, 1, "full after 16 pushes");
    ASSERT_EQ(dut.push_ready, 0, "push_ready when full");

    dut.push_valid = 0;
    tick();

    printf("Test 2 PASS: push until full\n");
}

// Test 3: empty detection
static void test_empty_detection() {
    reset();

    ASSERT_EQ(dut.empty, 1, "empty after reset");
    ASSERT_EQ(dut.pop_valid, 0, "pop_valid when empty");

    // Push one item
    dut.push_valid = 1;
    dut.push_data = 0x42;
    tick();
    dut.push_valid = 0;
    tick();

    ASSERT_EQ(dut.empty, 0, "not empty after push");
    ASSERT_EQ(dut.pop_valid, 1, "pop_valid after push");

    // Pop it
    dut.pop_ready = 1;
    ASSERT_EQ(dut.pop_data, 0x42u, "pop_data");
    tick();
    dut.pop_ready = 0;
    tick();

    ASSERT_EQ(dut.empty, 1, "empty after pop");

    printf("Test 3 PASS: empty detection\n");
}

int main() {
    test_push_pop_order();
    test_push_until_full();
    test_empty_detection();
    printf("PASS\n");
    return 0;
}
