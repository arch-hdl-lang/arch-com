// Verilator testbench for TaskQueue (linklist_basic)
// Tests: alloc, free, insert_tail, delete_head, read_data, write_data
//
// Build:
//   verilator --cc --exe --build tests/linklist_basic_tb.cpp tests/linklist_basic.sv \
//             --top-module TaskQueue -o linklist_basic_tb \
//             --Mdir linklist_basic_build
//   ./linklist_basic_build/linklist_basic_tb
//
// Makefile target: make -C tests linklist_basic

#include "VTaskQueue.h"
#include "verilated.h"
#include <cassert>
#include <cstdint>
#include <cstdio>
#include <vector>

static int  g_tests  = 0;
static int  g_passed = 0;

#define CHECK(cond, msg) do { \
    g_tests++; \
    if (cond) { g_passed++; } \
    else { fprintf(stderr, "FAIL [%s:%d] %s\n", __FILE__, __LINE__, msg); } \
} while(0)

// ── DUT wrapper ─────────────────────────────────────────────────────────────

struct DUT {
    VTaskQueue dut;
    uint64_t   time = 0;

    void reset(int cycles = 2) {
        dut.rst = 1;
        dut.clk = 0;
        // clear all inputs
        dut.alloc_req_valid      = 0;
        dut.free_req_valid       = 0;
        dut.free_req_handle      = 0;
        dut.insert_tail_req_valid = 0;
        dut.insert_tail_req_data  = 0;
        dut.delete_head_req_valid = 0;
        dut.read_data_req_valid  = 0;
        dut.read_data_req_handle = 0;
        dut.write_data_req_valid = 0;
        dut.write_data_req_handle = 0;
        dut.write_data_req_data  = 0;
        for (int i = 0; i < cycles * 2; i++) tick_half();
        dut.rst = 0;
    }

    void tick_half() {
        dut.clk ^= 1;
        dut.eval();
        time++;
    }

    // Advance one full clock cycle; sample outputs on rising edge.
    void tick() {
        tick_half(); // negedge
        tick_half(); // posedge  ← sample here
    }

    // Pulse req for one cycle; return handle on resp (or 0 if no resp).
    uint32_t alloc_one() {
        dut.alloc_req_valid = 1;
        tick();
        dut.alloc_req_valid = 0;
        uint32_t h = dut.alloc_resp_valid ? (uint32_t)dut.alloc_resp_handle : 0xFF;
        tick(); // allow resp to propagate
        return h;
    }

    void free_one(uint32_t handle) {
        dut.free_req_valid  = 1;
        dut.free_req_handle = handle;
        tick();
        dut.free_req_valid  = 0;
        dut.free_req_handle = 0;
        tick();
    }

    // insert_tail: latency 2 — issue cycle 1, commit cycle 2
    uint32_t insert_tail_one(uint32_t data) {
        dut.insert_tail_req_valid = 1;
        dut.insert_tail_req_data  = data;
        tick(); // cycle 1: FSM grabs slot
        dut.insert_tail_req_valid = 0;
        dut.insert_tail_req_data  = 0;
        tick(); // cycle 2: link update, resp fires
        uint32_t h = dut.insert_tail_resp_valid ? (uint32_t)dut.insert_tail_resp_handle : 0xFF;
        tick(); // let resp clear
        return h;
    }

    // delete_head: latency 2
    uint32_t delete_head_one() {
        dut.delete_head_req_valid = 1;
        tick(); // cycle 1: latch head data + slot
        dut.delete_head_req_valid = 0;
        tick(); // cycle 2: free slot, advance head, resp fires
        uint32_t d = dut.delete_head_resp_valid ? (uint32_t)dut.delete_head_resp_data : 0xDEAD;
        tick(); // let resp clear
        return d;
    }

    uint32_t read_data_one(uint32_t handle) {
        dut.read_data_req_valid  = 1;
        dut.read_data_req_handle = handle;
        tick(); // rising edge: resp latched into output register
        // Sample immediately after first tick — resp is valid now
        uint32_t data = dut.read_data_resp_valid ? (uint32_t)dut.read_data_resp_data : 0xDEAD;
        dut.read_data_req_valid  = 0;
        dut.read_data_req_handle = 0;
        tick(); // clear cycle
        return data;
    }

    void write_data_one(uint32_t handle, uint32_t data) {
        dut.write_data_req_valid   = 1;
        dut.write_data_req_handle  = handle;
        dut.write_data_req_data    = data;
        tick();
        dut.write_data_req_valid  = 0;
        dut.write_data_req_handle = 0;
        dut.write_data_req_data   = 0;
        tick();
    }
};

// ── Tests ────────────────────────────────────────────────────────────────────

static void test_initial_state(DUT& d) {
    d.reset();
    CHECK(d.dut.empty == 1, "empty after reset");
    CHECK(d.dut.full  == 0, "not full after reset");
    CHECK(d.dut.length == 0, "length == 0 after reset");
    CHECK(d.dut.alloc_req_ready == 1, "alloc_req_ready after reset (free slots)");
    printf("  test_initial_state: %d tests\n", g_tests);
}

static void test_alloc_and_free(DUT& d) {
    d.reset();
    // Alloc one slot — alloc consumes a free slot so length/empty reflect it
    uint32_t h = d.alloc_one();
    CHECK(h != 0xFF, "alloc returned a valid handle");
    CHECK(d.dut.length == 1, "length == 1 after alloc (slot consumed from free list)");
    CHECK(d.dut.empty == 0, "not empty after alloc (slot allocated)");

    // Free it back
    d.free_one(h);
    CHECK(d.dut.empty == 1, "still empty after free (no data was inserted)");
    printf("  test_alloc_and_free ok\n");
}

static void test_insert_and_delete_single(DUT& d) {
    d.reset();
    uint32_t h = d.insert_tail_one(0xABCD1234);
    CHECK(h != 0xFF, "insert_tail got a handle");
    CHECK(d.dut.length == 1, "length == 1 after insert");
    CHECK(d.dut.empty == 0, "not empty after insert");

    uint32_t data = d.delete_head_one();
    CHECK(data == 0xABCD1234, "delete_head returned correct data");
    CHECK(d.dut.length == 0, "length == 0 after delete");
    CHECK(d.dut.empty == 1, "empty after delete");
    printf("  test_insert_and_delete_single ok\n");
}

static void test_fifo_order(DUT& d) {
    d.reset();
    // Insert 4 values
    const uint32_t vals[4] = {10, 20, 30, 40};
    for (int i = 0; i < 4; i++) d.insert_tail_one(vals[i]);
    CHECK(d.dut.length == 4, "length == 4 after 4 inserts");

    // Delete all and verify FIFO order
    for (int i = 0; i < 4; i++) {
        uint32_t got = d.delete_head_one();
        CHECK(got == vals[i], "FIFO order preserved");
    }
    CHECK(d.dut.empty == 1, "empty after draining");
    printf("  test_fifo_order ok\n");
}

static void test_fill_to_capacity(DUT& d) {
    d.reset();
    const int DEPTH = 8;
    // Fill all 8 slots
    for (int i = 0; i < DEPTH; i++) d.insert_tail_one(0x100 + i);
    CHECK(d.dut.full == 1, "full after filling DEPTH slots");
    CHECK(d.dut.length == DEPTH, "length == DEPTH");
    CHECK(d.dut.insert_tail_req_ready == 0, "insert_tail blocked when full");

    // Drain all
    for (int i = 0; i < DEPTH; i++) {
        uint32_t got = d.delete_head_one();
        CHECK(got == (uint32_t)(0x100 + i), "drain order correct");
    }
    CHECK(d.dut.empty == 1, "empty after full drain");
    printf("  test_fill_to_capacity ok\n");
}

static void test_read_write_data(DUT& d) {
    d.reset();
    uint32_t h = d.insert_tail_one(0xDEADBEEF);

    uint32_t rd = d.read_data_one(h);
    CHECK(rd == 0xDEADBEEF, "read_data returns inserted value");

    d.write_data_one(h, 0xCAFEBABE);
    rd = d.read_data_one(h);
    CHECK(rd == 0xCAFEBABE, "read_data returns written value");

    // Cleanup
    d.delete_head_one();
    printf("  test_read_write_data ok\n");
}

static void test_alloc_free_cycle(DUT& d) {
    // Alloc/free without inserting; should never affect length
    d.reset();
    std::vector<uint32_t> handles;
    for (int i = 0; i < 8; i++) handles.push_back(d.alloc_one());
    CHECK(d.dut.alloc_req_ready == 0, "no free slots left");

    CHECK(d.dut.length == 8, "length == DEPTH when all slots allocated");
    for (auto h : handles) d.free_one(h);
    CHECK(d.dut.alloc_req_ready == 1, "slots restored after free");
    printf("  test_alloc_free_cycle ok\n");
}

static void test_interleaved_insert_delete(DUT& d) {
    d.reset();
    // Insert 3, delete 1, insert 2 — verify FIFO across the gap
    d.insert_tail_one(1);
    d.insert_tail_one(2);
    d.insert_tail_one(3);
    uint32_t x = d.delete_head_one();
    CHECK(x == 1, "first dequeue == 1");
    d.insert_tail_one(4);
    d.insert_tail_one(5);
    uint32_t y = d.delete_head_one(); CHECK(y == 2, "second dequeue == 2");
    uint32_t z = d.delete_head_one(); CHECK(z == 3, "third dequeue == 3");
    uint32_t w = d.delete_head_one(); CHECK(w == 4, "fourth dequeue == 4");
    uint32_t v = d.delete_head_one(); CHECK(v == 5, "fifth dequeue == 5");
    CHECK(d.dut.empty == 1, "empty at end");
    printf("  test_interleaved_insert_delete ok\n");
}

// ── main ─────────────────────────────────────────────────────────────────────

int main(int argc, char** argv) {
    Verilated::commandArgs(argc, argv);
    DUT d;

    printf("=== TaskQueue (linklist_basic) simulation tests ===\n");
    test_initial_state(d);
    test_alloc_and_free(d);
    test_insert_and_delete_single(d);
    test_fifo_order(d);
    test_fill_to_capacity(d);
    test_read_write_data(d);
    test_alloc_free_cycle(d);
    test_interleaved_insert_delete(d);

    printf("\n%d / %d tests passed\n", g_passed, g_tests);
    return (g_passed == g_tests) ? 0 : 1;
}
