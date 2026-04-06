#include "VTxQueue.h"
#include "verilated.h"
#include <cstdio>
#include <queue>

int main(int argc, char** argv) {
    Verilated::commandArgs(argc, argv);
    VTxQueue* dut = new VTxQueue;

    int errors = 0;
    std::queue<int> model; // software model

    auto tick = [&]() {
        dut->clk = 0; dut->eval();
        dut->clk = 1; dut->eval();
    };

    // Reset
    dut->rst = 1; dut->push_valid = 0; dut->pop_ready = 0; dut->push_data = 0;
    tick(); tick();

    if (!dut->empty || dut->full) {
        printf("FAIL: after reset, empty=%d full=%d (expected empty=1 full=0)\n",
               dut->empty, dut->full);
        errors++;
    } else {
        printf("PASS: reset → empty FIFO\n");
    }

    dut->rst = 0;

    // Push 16 items (fill to capacity)
    for (int i = 0; i < 16; i++) {
        dut->push_valid = 1;
        dut->push_data  = i + 1;
        dut->pop_ready  = 0;
        if (!dut->push_ready) {
            printf("FAIL: FIFO unexpectedly full at push %d\n", i);
            errors++;
        }
        tick();
        model.push(i + 1);
    }
    dut->push_valid = 0;
    tick(); // settle

    if (!dut->full) {
        printf("FAIL: FIFO should be full after 16 pushes\n");
        errors++;
    } else {
        printf("PASS: FIFO full after 16 pushes\n");
    }
    if (dut->push_ready) {
        printf("FAIL: push_ready should be 0 when full\n");
        errors++;
    } else {
        printf("PASS: push_ready deasserted when full\n");
    }

    // Pop all 16 items and verify order
    int pop_errors = 0;
    for (int i = 0; i < 16; i++) {
        dut->pop_ready = 1;
        if (!dut->pop_valid) {
            printf("FAIL: pop_valid unexpectedly low at pop %d\n", i);
            errors++;
        }
        int expected = model.front(); model.pop();
        if (dut->pop_data != expected) {
            printf("FAIL: pop %d: got %d, expected %d\n", i, (int)dut->pop_data, expected);
            pop_errors++;
        }
        tick();
    }
    dut->pop_ready = 0;
    tick();
    if (pop_errors == 0) {
        printf("PASS: all 16 items popped in correct order\n");
    } else {
        errors += pop_errors;
    }

    if (!dut->empty) {
        printf("FAIL: FIFO should be empty after popping all items\n");
        errors++;
    } else {
        printf("PASS: FIFO empty after all pops\n");
    }

    // Push one more item and pop it
    dut->push_valid = 1; dut->push_data = 0xAA; dut->pop_ready = 0;
    tick(); // push 0xAA; wr_ptr++, rd_ptr unchanged
    // Sample pop_data BEFORE the rising edge that would advance rd_ptr
    dut->push_valid = 0; dut->pop_ready = 1;
    dut->clk = 0; dut->eval(); // combinational update
    int got_aa = dut->pop_data;
    dut->clk = 1; dut->eval(); // commit (rd_ptr advances here)
    if (got_aa != 0xAA) {
        printf("FAIL: push/pop: got 0x%X, expected 0xAA\n", got_aa);
        errors++;
    } else {
        printf("PASS: push/pop: 0xAA read correctly\n");
    }

    dut->final();
    delete dut;

    if (errors == 0) { printf("\nALL TESTS PASSED\n"); return 0; }
    else             { printf("\n%d TESTS FAILED\n", errors); return 1; }
}
