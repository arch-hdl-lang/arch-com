#include "VWaitPipe.h"
#include "verilated.h"
#include <cstdio>
#include <cstdlib>

int main(int argc, char** argv) {
    Verilated::commandArgs(argc, argv);
    auto* dut = new VWaitPipe;

    int errors = 0;
    int cycle = 0;

    auto tick = [&]() {
        dut->clk = 0; dut->eval();
        dut->clk = 1; dut->eval();
        cycle++;
    };

    // Reset
    dut->rst = 1;
    dut->addr_in = 0;
    dut->mem_valid = 0;
    dut->mem_data = 0;
    tick(); tick();
    dut->rst = 0;

    // ── Test 1: Fast path — mem_valid already high ──────────────────
    printf("Test 1: Fast path (mem_valid=1 on entry)\n");
    dut->addr_in = 0xAAAA;
    dut->mem_valid = 1;
    dut->mem_data = 0xDEAD;
    tick(); // Fetch captures addr
    tick(); // DataAccess: upstream valid, mem_valid=1 → fast path, captures 0xDEAD
    tick(); // Writeback captures DataAccess.data
    // After 3 pipeline cycles from input, data_out should have the value
    // But pipeline latency depends on valid propagation. Let's run a few more.
    tick();

    printf("  cycle=%d data_out=0x%08X (expect 0x0000DEAD)\n", cycle, dut->data_out);
    if (dut->data_out != 0xDEAD) {
        printf("  FAIL: expected 0xDEAD\n");
        errors++;
    }

    // ── Test 2: Slow path — mem_valid delayed ──────────────────────
    printf("Test 2: Slow path (mem_valid delayed by 3 cycles)\n");
    // Reset to clean state
    dut->rst = 1;
    tick(); tick();
    dut->rst = 0;
    cycle = 0;

    dut->addr_in = 0xBBBB;
    dut->mem_valid = 0;
    dut->mem_data = 0;
    tick(); // cycle 1: Fetch captures addr
    tick(); // cycle 2: DataAccess sees upstream valid, mem_valid=0 → enters wait state 1
    tick(); // cycle 3: still waiting
    tick(); // cycle 4: still waiting

    // Now assert mem_valid with data
    dut->mem_valid = 1;
    dut->mem_data = 0xBEEF;
    tick(); // cycle 5: DataAccess FSM in state 1, mem_valid=1 → captures, returns to idle

    dut->mem_valid = 0;
    tick(); // cycle 6: Writeback captures
    tick(); // cycle 7: data_out should have 0xBEEF

    printf("  cycle=%d data_out=0x%08X (expect 0x0000BEEF)\n", cycle, dut->data_out);
    if (dut->data_out != 0xBEEF) {
        printf("  FAIL: expected 0xBEEF\n");
        errors++;
    }

    // ── Test 3: Verify stall — upstream shouldn't advance while DataAccess is busy ──
    printf("Test 3: Stall propagation\n");
    dut->rst = 1;
    tick(); tick();
    dut->rst = 0;
    cycle = 0;

    dut->addr_in = 0x1111;
    dut->mem_valid = 0;
    dut->mem_data = 0;
    tick(); // cycle 1: Fetch captures 0x1111

    // Change addr_in — if stall works, Fetch should NOT capture this
    dut->addr_in = 0x2222;
    tick(); // cycle 2: DataAccess enters wait (mem_valid=0), stalls upstream

    // Fetch should be stalled — fetch_addr should still be 0x1111 (not 0x2222)
    // We can't directly observe fetch_addr from outside, but we can verify
    // that a second value doesn't corrupt the pipeline.

    dut->addr_in = 0x3333;
    tick(); // cycle 3: still stalled
    tick(); // cycle 4: still stalled

    dut->mem_valid = 1;
    dut->mem_data = 0xCAFE;
    tick(); // cycle 5: DataAccess completes, unstalls

    dut->mem_valid = 0;
    dut->addr_in = 0;
    tick(); tick(); tick(); // Let pipeline drain

    printf("  cycle=%d data_out=0x%08X (expect 0x0000CAFE)\n", cycle, dut->data_out);
    if (dut->data_out != 0xCAFE) {
        printf("  FAIL: expected 0xCAFE\n");
        errors++;
    }

    // Summary
    if (errors == 0) {
        printf("\nPASS: All tests passed\n");
    } else {
        printf("\nFAIL: %d error(s)\n", errors);
    }

    delete dut;
    return errors ? 1 : 0;
}
