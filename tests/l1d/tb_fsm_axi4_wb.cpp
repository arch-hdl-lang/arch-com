#include "VFsmAxi4Wb.h"
#include "verilated.h"
#include <cstdio>
#include <cstdlib>
#include <cstdint>

static VFsmAxi4Wb* dut;

static void tick(int n = 1) {
    for (int i = 0; i < n; i++) {
        dut->clk = 0; dut->eval();
        dut->clk = 1; dut->eval();
    }
}

static void fail(const char* msg) { printf("FAIL: %s\n", msg); exit(1); }

int main(int argc, char** argv) {
    VerilatedContext* ctx = new VerilatedContext;
    ctx->commandArgs(argc, argv);
    dut = new VFsmAxi4Wb(ctx);

    // Reset
    dut->rst = 1; dut->clk = 0; dut->eval();
    dut->wb_start  = 0; dut->wb_addr = 0;
    dut->wb_word_0 = 0; dut->wb_word_1 = 0; dut->wb_word_2 = 0; dut->wb_word_3 = 0;
    dut->wb_word_4 = 0; dut->wb_word_5 = 0; dut->wb_word_6 = 0; dut->wb_word_7 = 0;
    dut->aw_ready = 0; dut->w_ready = 0; dut->b_valid = 0; dut->b_id = 0; dut->b_resp = 0;
    tick(2); dut->rst = 0; tick(1);

    // ── Test: writeback line at addr 0xA0001098 (unaligned → aligns to 0xA0001080) ─
    uint64_t wb_addr    = 0xA0001098ULL;
    uint64_t line_addr  = wb_addr & ~63ULL; // 0xA0001098 & ~63 = 0xA0001080

    uint64_t wb_words[8];
    for (int i = 0; i < 8; i++) wb_words[i] = 0xDEADBEEF00000000ULL | (uint64_t)i;

    dut->wb_start  = 1; dut->wb_addr = wb_addr;
    dut->wb_word_0 = wb_words[0]; dut->wb_word_1 = wb_words[1];
    dut->wb_word_2 = wb_words[2]; dut->wb_word_3 = wb_words[3];
    dut->wb_word_4 = wb_words[4]; dut->wb_word_5 = wb_words[5];
    dut->wb_word_6 = wb_words[6]; dut->wb_word_7 = wb_words[7];
    tick(1); dut->wb_start = 0;

    // Wait for AW channel
    int timeout = 20;
    while (!dut->aw_valid && --timeout) tick(1);
    if (!dut->aw_valid) fail("aw_valid never asserted");

    if ((uint64_t)dut->aw_addr != line_addr)
        fail("aw_addr not line-aligned");
    if (dut->aw_len  != 7) fail("aw_len != 7");
    if (dut->aw_size != 3) fail("aw_size != 3");
    if (dut->aw_burst!= 1) fail("aw_burst != 1 (INCR)");
    if (dut->aw_id   != 1) fail("aw_id != 1");

    // Accept AW
    dut->aw_ready = 1; tick(1); dut->aw_ready = 0;
    tick(1); // FSM transitions to SendW

    // Verify each W beat
    for (int i = 0; i < 8; i++) {
        timeout = 5;
        while (!dut->w_valid && --timeout) tick(1);
        if (!dut->w_valid) { printf("FAIL: w_valid not asserted for beat %d\n", i); exit(1); }

        uint8_t exp_last = (i == 7) ? 1 : 0;
        if ((uint64_t)dut->w_data != wb_words[i]) {
            printf("FAIL: beat %d: w_data=0x%016llx expected=0x%016llx\n",
                   i, (unsigned long long)(uint64_t)dut->w_data,
                   (unsigned long long)wb_words[i]);
            exit(1);
        }
        if (dut->w_strb != 0xFF) { printf("FAIL: beat %d: w_strb=0x%02x\n", i, (uint8_t)dut->w_strb); exit(1); }
        if (dut->w_last != exp_last) { printf("FAIL: beat %d: w_last=%d expected=%d\n", i, dut->w_last, exp_last); exit(1); }

        dut->w_ready = 1; tick(1); dut->w_ready = 0;
    }

    // FSM in WaitB: drive b_valid
    tick(1);
    if (dut->wb_done) fail("wb_done prematurely asserted");

    dut->b_valid = 1; dut->b_id = 1; dut->b_resp = 0;
    dut->eval();
    if (!dut->b_ready) fail("b_ready not asserted in WaitB");
    if (!dut->wb_done) fail("wb_done not asserted when b_valid");

    tick(1);
    dut->b_valid = 0;

    // wb_done should clear (FSM returned to Idle)
    if (dut->wb_done) fail("wb_done still asserted after b_valid deasserted");

    // ── Test 2: back-to-back with w_ready deasserted stall ────────────────
    for (int i = 0; i < 8; i++) wb_words[i] = 0x1111111100000000ULL | (uint64_t)(i * 0x10);
    dut->wb_start  = 1; dut->wb_addr = 0xB0000000ULL;
    dut->wb_word_0 = wb_words[0]; dut->wb_word_1 = wb_words[1];
    dut->wb_word_2 = wb_words[2]; dut->wb_word_3 = wb_words[3];
    dut->wb_word_4 = wb_words[4]; dut->wb_word_5 = wb_words[5];
    dut->wb_word_6 = wb_words[6]; dut->wb_word_7 = wb_words[7];
    tick(1); dut->wb_start = 0;

    timeout = 10;
    while (!dut->aw_valid && --timeout) tick(1);
    dut->aw_ready = 1; tick(1); dut->aw_ready = 0;
    tick(1);

    // Accept beats with stalls (w_ready held low for 1 cycle between beats)
    for (int i = 0; i < 8; i++) {
        timeout = 5;
        while (!dut->w_valid && --timeout) tick(1);
        if ((uint64_t)dut->w_data != wb_words[i]) {
            printf("FAIL test2: beat %d data mismatch\n", i); exit(1);
        }
        dut->w_ready = 1; tick(1); dut->w_ready = 0;
        tick(1); // stall cycle
    }

    tick(1);
    dut->b_valid = 1; tick(1); dut->b_valid = 0;

    printf("PASS\n");
    delete dut; delete ctx;
    return 0;
}
