// ARCH sim testbench for e203_sram_ctrl — ICB slave front-end over a
// single-port latency-1 SRAM bank. Tests: reset state, write-then-readback
// through the ICB interface, the fixed 1-cycle response latency, multiple
// distinct words, byte-to-word address mapping (addr[13:2], so 0x4000
// aliases word 0), back-to-back pipelined reads, and rsp_valid dropping when
// no command was accepted.
//
// NOTE: this replaces a stale tb (VSramCtrl.h) that targeted the pre-2026-04
// PascalCase fixture naming. The construct is now `e203_sram_ctrl`, so the
// sim class is Ve203_sram_ctrl.
//
// Run with:
//   arch sim tests/e203/e203_sram_ctrl.arch --tb tests/e203/e203_sram_ctrl_tb.cpp

#include "Ve203_sram_ctrl.h"
#include <cstdio>
#include <cstdint>

static int fail_count = 0;

#define CHECK(cond, fmt, ...) do { \
    if (!(cond)) { \
        printf("  FAIL: " fmt "\n", ##__VA_ARGS__); \
        fail_count++; \
    } \
} while(0)

static Ve203_sram_ctrl* dut;

static void tick() {
    dut->clk = 0; dut->eval();
    dut->clk = 1; dut->eval();
}

static void reset() {
    dut->rst_n = 0;
    dut->icb_cmd_valid = 0;
    dut->icb_cmd_addr = 0;
    dut->icb_cmd_wdata = 0;
    dut->icb_cmd_wmask = 0xF;
    dut->icb_cmd_read = 1;      // idle as "read" so the RAM never writes
    dut->icb_rsp_ready = 1;
    for (int i = 0; i < 3; i++) tick();
    dut->rst_n = 1;
    dut->eval();
}

// One-cycle ICB write (cmd_ready is constant 1 in this design).
static void icb_write(uint32_t addr, uint32_t data) {
    dut->icb_cmd_valid = 1;
    dut->icb_cmd_addr = addr;
    dut->icb_cmd_wdata = data;
    dut->icb_cmd_read = 0;
    tick();
    dut->icb_cmd_valid = 0;
    dut->icb_cmd_read = 1;
    dut->eval();
    CHECK(dut->icb_rsp_valid == 1, "write to 0x%08x should respond 1 cycle later, got rsp_valid=%d",
          addr, dut->icb_rsp_valid);
    tick();                     // drain the response cycle
    dut->eval();
}

// One-cycle ICB read: returns rdata from the response cycle.
static uint32_t icb_read(uint32_t addr) {
    dut->icb_cmd_valid = 1;
    dut->icb_cmd_addr = addr;
    dut->icb_cmd_read = 1;
    tick();
    dut->icb_cmd_valid = 0;
    dut->eval();
    CHECK(dut->icb_rsp_valid == 1, "read of 0x%08x should respond 1 cycle later, got rsp_valid=%d",
          addr, dut->icb_rsp_valid);
    uint32_t v = dut->icb_rsp_rdata;
    tick();
    dut->eval();
    return v;
}

int main() {
    dut = new Ve203_sram_ctrl;

    // ── Test 1: Reset state ──────────────────────────────────────────
    printf("Test 1: Reset state\n");
    reset();
    CHECK(dut->icb_cmd_ready == 1, "cmd_ready is constant 1 in this design, got %d",
          dut->icb_cmd_ready);
    CHECK(dut->icb_rsp_valid == 0, "rsp_valid should be 0 after reset, got %d", dut->icb_rsp_valid);
    CHECK(dut->icb_rsp_err == 0, "rsp_err is tied low in this design, got %d", dut->icb_rsp_err);

    // ── Test 2: Write then read back ─────────────────────────────────
    printf("Test 2: Write/readback\n");
    icb_write(0x00000100u, 0xABCD1234u);
    uint32_t v = icb_read(0x00000100u);
    CHECK(v == 0xABCD1234u, "readback of 0x100 should be 0xABCD1234, got 0x%08x", v);

    // ── Test 3: Multiple distinct words ──────────────────────────────
    printf("Test 3: Multiple words\n");
    icb_write(0x00000000u, 0x11111111u);
    icb_write(0x00000004u, 0x22222222u);
    icb_write(0x00003FFCu, 0x33333333u);   // last word of the 4096-word bank
    v = icb_read(0x00000000u);
    CHECK(v == 0x11111111u, "word 0 should be 0x11111111, got 0x%08x", v);
    v = icb_read(0x00000004u);
    CHECK(v == 0x22222222u, "word 1 should be 0x22222222, got 0x%08x", v);
    v = icb_read(0x00003FFCu);
    CHECK(v == 0x33333333u, "last word should be 0x33333333, got 0x%08x", v);
    // Overwrite in place.
    icb_write(0x00000004u, 0x44444444u);
    v = icb_read(0x00000004u);
    CHECK(v == 0x44444444u, "word 1 should update to 0x44444444, got 0x%08x", v);
    v = icb_read(0x00000000u);
    CHECK(v == 0x11111111u, "word 0 must be untouched by the word-1 write, got 0x%08x", v);

    // ── Test 4: Address mapping uses addr[13:2] ──────────────────────
    printf("Test 4: Address aliasing\n");
    // 0x4000 has bit 14 set but bits [13:2] zero: it aliases word 0.
    v = icb_read(0x00004000u);
    CHECK(v == 0x11111111u, "0x4000 should alias word 0 (addr[13:2]), got 0x%08x", v);
    // Byte offsets within a word (bits [1:0]) are ignored.
    v = icb_read(0x00000007u);
    CHECK(v == 0x44444444u, "0x7 should alias word 1 (byte offset ignored), got 0x%08x", v);

    // ── Test 5: Back-to-back pipelined reads ─────────────────────────
    printf("Test 5: Pipelined reads\n");
    // Present read of word 0; while its response is returning, present read
    // of word 1 in the same cycle.
    dut->icb_cmd_valid = 1;
    dut->icb_cmd_read = 1;
    dut->icb_cmd_addr = 0x00000000u;
    tick();
    dut->icb_cmd_addr = 0x00000004u;   // second read presented during first response
    dut->eval();
    CHECK(dut->icb_rsp_valid == 1, "first response should be valid, got %d", dut->icb_rsp_valid);
    CHECK(dut->icb_rsp_rdata == 0x11111111u, "first response should be word 0, got 0x%08x",
          dut->icb_rsp_rdata);
    tick();
    dut->icb_cmd_valid = 0;
    dut->eval();
    CHECK(dut->icb_rsp_valid == 1, "second response should follow immediately, got %d",
          dut->icb_rsp_valid);
    CHECK(dut->icb_rsp_rdata == 0x44444444u, "second response should be word 1, got 0x%08x",
          dut->icb_rsp_rdata);

    // ── Test 6: rsp_valid drops with no command ──────────────────────
    printf("Test 6: Response gap\n");
    tick();
    dut->eval();
    CHECK(dut->icb_rsp_valid == 0, "rsp_valid should drop when no command was accepted, got %d",
          dut->icb_rsp_valid);

    printf("\n=== e203_sram_ctrl: %d failure(s) ===\n", fail_count);
    return fail_count ? 1 : 0;
}
