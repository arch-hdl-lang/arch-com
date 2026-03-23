// ARCH sim testbench for Ift2Icb — IFU to ITCM ICB bridge
// Tests: reset state, address conversion, response pipeline, back-to-back, backpressure

#include "VIft2Icb.h"
#include <cstdio>
#include <cstdint>
#include <cstdlib>

static int fail_count = 0;

#define CHECK(cond, fmt, ...) do { \
    if (!(cond)) { \
        printf("  FAIL: " fmt "\n", ##__VA_ARGS__); \
        fail_count++; \
    } \
} while(0)

static VIft2Icb* dut;

static void tick() {
    dut->clk = 0; dut->eval();
    dut->clk = 1; dut->eval();
}

static void reset() {
    dut->rst_n = 0;
    dut->ifu_req_valid = 0;
    dut->ifu_req_pc = 0;
    dut->ifu_rsp_ready = 1;
    dut->itcm_cmd_ready = 1;
    dut->itcm_rsp_valid = 0;
    dut->itcm_rsp_data = 0;
    for (int i = 0; i < 3; i++) tick();
    dut->rst_n = 1;
    tick();
}

int main(int argc, char** argv) {
    dut = new VIft2Icb;

    // ── Test 1: Reset state ──────────────────────────────────────────
    printf("Test 1: Reset state\n");
    reset();
    CHECK(dut->ifu_rsp_valid == 0, "rsp_valid should be 0 after reset, got %d", dut->ifu_rsp_valid);
    CHECK(dut->itcm_cmd_valid == 0, "cmd_valid should be 0 after reset, got %d", dut->itcm_cmd_valid);

    // ── Test 2: Single fetch — address conversion ────────────────────
    printf("Test 2: Single fetch with address conversion\n");
    reset();
    // Send request at PC = 0x0000_1080 → word addr = 0x1080 >> 2 = 0x0420
    dut->ifu_req_valid = 1;
    dut->ifu_req_pc = 0x00001080;
    dut->itcm_cmd_ready = 1;
    dut->eval();

    CHECK(dut->itcm_cmd_valid == 1, "cmd_valid should be 1");
    CHECK(dut->itcm_cmd_addr == 0x0420, "cmd_addr should be 0x0420, got 0x%04x", dut->itcm_cmd_addr);
    CHECK(dut->ifu_req_ready == 1, "req_ready should be 1");

    // ITCM accepts cmd, respond next cycle
    tick();
    dut->ifu_req_valid = 0;
    dut->itcm_rsp_valid = 1;
    dut->itcm_rsp_data = 0xDEADBEEF;
    tick();

    // Response should appear after pipeline register
    CHECK(dut->ifu_rsp_valid == 1, "rsp_valid should be 1 after pipeline");
    CHECK(dut->ifu_rsp_instr == 0xDEADBEEF, "rsp_instr should be 0xDEADBEEF, got 0x%08x", dut->ifu_rsp_instr);

    // ── Test 3: Response pipeline latency ────────────────────────────
    printf("Test 3: Response pipeline latency\n");
    reset();
    dut->ifu_req_valid = 1;
    dut->ifu_req_pc = 0x00000100;
    dut->itcm_cmd_ready = 1;
    tick();

    // ITCM responds immediately
    dut->ifu_req_valid = 0;
    dut->itcm_rsp_valid = 1;
    dut->itcm_rsp_data = 0xCAFEBABE;
    dut->eval();

    // Before clock: rsp_valid_r still 0
    CHECK(dut->ifu_rsp_valid == 0, "rsp_valid should still be 0 before pipeline tick");

    tick();
    // After clock: registered response visible
    CHECK(dut->ifu_rsp_valid == 1, "rsp_valid should be 1 after pipeline tick");
    CHECK(dut->ifu_rsp_instr == 0xCAFEBABE, "rsp_instr should be 0xCAFEBABE, got 0x%08x", dut->ifu_rsp_instr);

    // ── Test 4: Back-to-back requests ────────────────────────────────
    printf("Test 4: Back-to-back requests\n");
    reset();
    dut->ifu_rsp_ready = 1;
    dut->itcm_cmd_ready = 1;

    // Request 1
    dut->ifu_req_valid = 1;
    dut->ifu_req_pc = 0x00000000;
    tick();
    dut->itcm_rsp_valid = 1;
    dut->itcm_rsp_data = 0x11111111;

    // Request 2 simultaneously
    dut->ifu_req_pc = 0x00000004;
    tick();
    CHECK(dut->ifu_rsp_valid == 1, "rsp from req1 should be valid");
    CHECK(dut->ifu_rsp_instr == 0x11111111, "rsp from req1 should be 0x11111111, got 0x%08x", dut->ifu_rsp_instr);

    dut->itcm_rsp_data = 0x22222222;
    tick();
    CHECK(dut->ifu_rsp_valid == 1, "rsp from req2 should be valid");
    CHECK(dut->ifu_rsp_instr == 0x22222222, "rsp from req2 should be 0x22222222, got 0x%08x", dut->ifu_rsp_instr);

    // ── Test 5: Backpressure — ifu_rsp_ready=0 stalls ────────────────
    printf("Test 5: Backpressure stalls pipeline\n");
    reset();
    dut->itcm_cmd_ready = 1;

    // Issue request
    dut->ifu_req_valid = 1;
    dut->ifu_req_pc = 0x00000200;
    tick();

    // ITCM responds
    dut->ifu_req_valid = 0;
    dut->itcm_rsp_valid = 1;
    dut->itcm_rsp_data = 0xAAAAAAAA;
    tick();

    // Response is registered
    CHECK(dut->ifu_rsp_valid == 1, "rsp should be valid");
    CHECK(dut->ifu_rsp_instr == 0xAAAAAAAA, "rsp should be 0xAAAAAAAA");

    // Now stall: ifu_rsp_ready = 0
    dut->ifu_rsp_ready = 0;
    dut->itcm_rsp_valid = 1;
    dut->itcm_rsp_data = 0xBBBBBBBB;  // new data on ITCM bus
    dut->eval();

    // Check stall propagation
    CHECK(dut->itcm_rsp_ready == 0, "itcm_rsp_ready should be 0 during stall");
    CHECK(dut->ifu_req_ready == 0, "ifu_req_ready should be 0 during stall (cmd_ready=1 but stalled)");

    // Tick while stalled — data should NOT update
    tick();
    CHECK(dut->ifu_rsp_instr == 0xAAAAAAAA, "rsp_instr should hold 0xAAAAAAAA during stall, got 0x%08x", dut->ifu_rsp_instr);

    // Release backpressure
    dut->ifu_rsp_ready = 1;
    tick();
    // Now the new data should be captured
    CHECK(dut->ifu_rsp_instr == 0xBBBBBBBB, "rsp_instr should update to 0xBBBBBBBB after stall release, got 0x%08x", dut->ifu_rsp_instr);

    // ── Summary ──────────────────────────────────────────────────────
    if (fail_count == 0) {
        printf("\nAll Ift2Icb tests PASSED.\n");
    } else {
        printf("\n%d Ift2Icb test(s) FAILED.\n", fail_count);
    }

    delete dut;
    return fail_count ? 1 : 0;
}
