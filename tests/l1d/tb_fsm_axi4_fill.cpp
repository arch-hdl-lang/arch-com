#include "VFsmAxi4Fill.h"
#include "verilated.h"
#include <cstdio>
#include <cstdlib>
#include <cstdint>

static VFsmAxi4Fill* dut;

static void tick(int n = 1) {
    for (int i = 0; i < n; i++) {
        dut->clk = 0; dut->eval();
        dut->clk = 1; dut->eval();
    }
}

static void fail(const char* msg) { printf("FAIL: %s\n", msg); exit(1); }

// Drive inactive inputs to safe defaults
static void idle_inputs() {
    dut->fill_start = 0; dut->fill_addr  = 0;
    dut->ar_ready   = 0;
    dut->r_valid    = 0; dut->r_data = 0;
    dut->r_id = 0; dut->r_resp = 0; dut->r_last = 0;
}

int main(int argc, char** argv) {
    VerilatedContext* ctx = new VerilatedContext;
    ctx->commandArgs(argc, argv);
    dut = new VFsmAxi4Fill(ctx);

    // Reset
    idle_inputs();
    dut->rst = 1; dut->clk = 0; dut->eval();
    tick(2);
    dut->rst = 0; tick(1);

    // ── Test: issue fill for address 0x80000040 (not line-aligned) ─────────
    uint64_t req_addr     = 0x80000040ULL;
    uint64_t line_addr    = req_addr & ~63ULL; // 0x80000000

    dut->fill_start = 1;
    dut->fill_addr  = req_addr;
    tick(1);
    dut->fill_start = 0;

    // Wait for AR channel to become valid (FSM: SendAR)
    int timeout = 20;
    while (!dut->ar_valid && --timeout) tick(1);
    if (!dut->ar_valid) fail("ar_valid never asserted");

    // Check AR channel values
    if ((uint64_t)dut->ar_addr != line_addr)
        fail("ar_addr not line-aligned");
    if (dut->ar_len  != 7) fail("ar_len != 7");
    if (dut->ar_size != 3) fail("ar_size != 3 (8B/beat)");
    if (dut->ar_burst!= 1) fail("ar_burst != 1 (INCR)");
    if (dut->ar_id   != 0) fail("ar_id != 0");

    // Handshake: accept AR
    dut->ar_ready = 1; tick(1); dut->ar_ready = 0;

    // FSM now in WaitR: r_ready should be asserted
    tick(1); // give FSM time to reach WaitR
    if (!dut->r_ready) fail("r_ready not asserted in WaitR");

    // Send 8 R beats
    uint64_t expected_words[8];
    for (int i = 0; i < 8; i++) {
        expected_words[i] = 0xCAFE000000000000ULL | ((uint64_t)i << 32) | 0xDEAD0000U + i;
        dut->r_valid = 1;
        dut->r_data  = expected_words[i];
        dut->r_id    = 0;
        dut->r_resp  = 0;
        dut->r_last  = (i == 7) ? 1 : 0;
        tick(1);
    }
    dut->r_valid = 0; dut->r_last = 0;

    // FSM entered Done on the last beat's rising edge — fill_done asserted now
    dut->eval();
    if (!dut->fill_done) fail("fill_done not asserted after 8 beats");

    // Verify fill words
    uint64_t words[8] = {
        dut->fill_word_0, dut->fill_word_1, dut->fill_word_2, dut->fill_word_3,
        dut->fill_word_4, dut->fill_word_5, dut->fill_word_6, dut->fill_word_7
    };
    for (int i = 0; i < 8; i++) {
        if (words[i] != expected_words[i]) {
            printf("FAIL: fill_word_%d = 0x%016llx, expected 0x%016llx\n",
                   i, (unsigned long long)words[i], (unsigned long long)expected_words[i]);
            exit(1);
        }
    }

    // fill_done should clear on next cycle (FSM returns to Idle)
    tick(1);
    if (dut->fill_done) fail("fill_done should clear after Done->Idle");

    // ── Test 2: back-to-back fill ──────────────────────────────────────────
    dut->fill_start = 1; dut->fill_addr = 0x90000000ULL; tick(1); dut->fill_start = 0;

    timeout = 20;
    while (!dut->ar_valid && --timeout) tick(1);
    if (!dut->ar_valid) fail("second fill: ar_valid never asserted");
    if ((uint64_t)dut->ar_addr != 0x90000000ULL) fail("second fill: ar_addr wrong");

    dut->ar_ready = 1; tick(1); dut->ar_ready = 0;
    tick(1);

    for (int i = 0; i < 8; i++) {
        dut->r_valid = 1; dut->r_data = 0xBEEFULL + i;
        dut->r_last = (i == 7) ? 1 : 0;
        tick(1);
    }
    dut->r_valid = 0; dut->r_last = 0; dut->eval();
    if (!dut->fill_done) fail("second fill: fill_done not asserted");

    printf("PASS\n");
    delete dut; delete ctx;
    return 0;
}
