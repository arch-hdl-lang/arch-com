#include "VAsyncBridge.h"
#include "verilated.h"
#include <cstdio>

int main(int argc, char** argv) {
    Verilated::commandArgs(argc, argv);
    VAsyncBridge* dut = new VAsyncBridge;

    int errors = 0;

    // Both clocks start 0; wr_clk toggles every 1 unit, rd_clk every 2
    int wr_phase = 0, rd_phase = 0;

    // Advance one wr_clk cycle (rd_clk unchanged)
    auto wr_tick = [&]() {
        dut->wr_clk = 0; dut->eval();
        dut->wr_clk = 1; dut->eval();
    };
    // Advance one rd_clk cycle (wr_clk unchanged)
    auto rd_tick = [&]() {
        dut->rd_clk = 0; dut->eval();
        dut->rd_clk = 1; dut->eval();
    };

    // Full reset
    dut->rst = 1;
    dut->push_valid = 0; dut->push_data = 0;
    dut->pop_ready  = 0;
    for (int i = 0; i < 4; i++) { wr_tick(); rd_tick(); }
    dut->rst = 0;

    // Push 8 items on wr_clk
    for (int i = 0; i < 8; i++) {
        dut->push_valid = 1;
        dut->push_data  = (uint8_t)(i + 1);
        wr_tick();
    }
    dut->push_valid = 0;
    // Let synchronizers settle (several rd_clk cycles)
    for (int i = 0; i < 8; i++) { rd_tick(); wr_tick(); }

    // Pop 8 items on rd_clk
    int pop_errors = 0;
    for (int i = 0; i < 8; i++) {
        if (!dut->pop_valid) {
            printf("FAIL: pop_valid low on item %d (FIFO should have data)\n", i);
            errors++;
        }
        int got = dut->pop_data;
        int expected = i + 1;
        if (got != expected) {
            printf("FAIL: item %d: got %d, expected %d\n", i, got, expected);
            pop_errors++;
        }
        dut->pop_ready = 1;
        rd_tick();
        dut->pop_ready = 0;
    }
    if (pop_errors == 0) {
        printf("PASS: 8 items pushed and popped correctly across clock domains\n");
    } else {
        errors += pop_errors;
    }

    // After draining, FIFO should report empty
    rd_tick(); rd_tick();
    if (dut->pop_valid) {
        printf("FAIL: pop_valid still high after draining all items\n");
        errors++;
    } else {
        printf("PASS: async FIFO empty after drain\n");
    }

    // Push-while-pop stress: 16 items
    int stress_errors = 0;
    for (int i = 0; i < 16; i++) {
        dut->push_valid = 1;
        dut->push_data  = (uint8_t)((i * 7 + 3) & 0xFF);
        wr_tick();
    }
    dut->push_valid = 0;
    for (int i = 0; i < 10; i++) { rd_tick(); wr_tick(); }

    for (int i = 0; i < 16; i++) {
        if (!dut->pop_valid) {
            printf("FAIL: stress pop_valid low at item %d\n", i);
            stress_errors++;
        }
        uint8_t expected = (uint8_t)((i * 7 + 3) & 0xFF);
        if (dut->pop_data != expected) {
            printf("FAIL: stress item %d: got %d, expected %d\n", i, (int)dut->pop_data, (int)expected);
            stress_errors++;
        }
        dut->pop_ready = 1;
        rd_tick();
        dut->pop_ready = 0;
    }
    if (stress_errors == 0) {
        printf("PASS: 16-item stress test correct\n");
    } else {
        errors += stress_errors;
    }

    dut->final();
    delete dut;

    if (errors == 0) { printf("\nALL TESTS PASSED\n"); return 0; }
    else             { printf("\n%d TESTS FAILED\n", errors); return 1; }
}
