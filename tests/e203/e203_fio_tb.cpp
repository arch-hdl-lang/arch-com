// ARCH sim testbench for e203_fio — low-latency ICB fast-I/O register file
// (4 output registers mirrored to fio_out_* pins, 2 input pins readable at
// register indices 8/9). Tests: reset state, single-cycle write with pin
// mirroring, 1-cycle-latency readback of all four output registers, input
// pin reads, the addr[5:2] register indexing, unmapped-index reads returning
// 0, and rsp_valid tracking of cmd_valid.
//
// NOTE: this replaces a stale tb (VFio.h) that targeted the pre-2026-04
// PascalCase fixture naming. The construct is now `e203_fio`, so the sim
// class is Ve203_fio.
//
// Run with:
//   arch sim tests/e203/e203_fio.arch --tb tests/e203/e203_fio_tb.cpp

#include "Ve203_fio.h"
#include <cstdio>
#include <cstdint>

static int fail_count = 0;

#define CHECK(cond, fmt, ...) do { \
    if (!(cond)) { \
        printf("  FAIL: " fmt "\n", ##__VA_ARGS__); \
        fail_count++; \
    } \
} while(0)

static Ve203_fio* dut;

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
    dut->icb_cmd_read = 1;
    dut->icb_rsp_ready = 1;
    dut->fio_in_0 = 0;
    dut->fio_in_1 = 0;
    for (int i = 0; i < 3; i++) tick();
    dut->rst_n = 1;
    dut->eval();
}

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
    tick();
    dut->eval();
}

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
    dut = new Ve203_fio;

    // ── Test 1: Reset state ──────────────────────────────────────────
    printf("Test 1: Reset state\n");
    reset();
    CHECK(dut->icb_cmd_ready == 1, "cmd_ready is constant 1 in this design, got %d",
          dut->icb_cmd_ready);
    CHECK(dut->icb_rsp_valid == 0, "rsp_valid should be 0 after reset, got %d", dut->icb_rsp_valid);
    CHECK(dut->icb_rsp_err == 0, "rsp_err is tied low in this design, got %d", dut->icb_rsp_err);
    CHECK(dut->fio_out_0 == 0 && dut->fio_out_1 == 0 && dut->fio_out_2 == 0 && dut->fio_out_3 == 0,
          "all fio_out pins should be 0 after reset");

    // ── Test 2: Writes mirror to the output pins ─────────────────────
    printf("Test 2: Output pin mirroring\n");
    icb_write(0x00, 0xA0A0A0A0u);   // reg 0
    CHECK(dut->fio_out_0 == 0xA0A0A0A0u, "fio_out_0 should be 0xA0A0A0A0, got 0x%08x",
          dut->fio_out_0);
    icb_write(0x04, 0xB1B1B1B1u);   // reg 1
    CHECK(dut->fio_out_1 == 0xB1B1B1B1u, "fio_out_1 should be 0xB1B1B1B1, got 0x%08x",
          dut->fio_out_1);
    icb_write(0x08, 0xC2C2C2C2u);   // reg 2
    CHECK(dut->fio_out_2 == 0xC2C2C2C2u, "fio_out_2 should be 0xC2C2C2C2, got 0x%08x",
          dut->fio_out_2);
    icb_write(0x0C, 0xD3D3D3D3u);   // reg 3
    CHECK(dut->fio_out_3 == 0xD3D3D3D3u, "fio_out_3 should be 0xD3D3D3D3, got 0x%08x",
          dut->fio_out_3);
    CHECK(dut->fio_out_0 == 0xA0A0A0A0u, "fio_out_0 must be untouched by later writes, got 0x%08x",
          dut->fio_out_0);

    // ── Test 3: Readback of the output registers ─────────────────────
    printf("Test 3: Register readback\n");
    uint32_t v = icb_read(0x00);
    CHECK(v == 0xA0A0A0A0u, "reg 0 readback should be 0xA0A0A0A0, got 0x%08x", v);
    v = icb_read(0x04);
    CHECK(v == 0xB1B1B1B1u, "reg 1 readback should be 0xB1B1B1B1, got 0x%08x", v);
    v = icb_read(0x08);
    CHECK(v == 0xC2C2C2C2u, "reg 2 readback should be 0xC2C2C2C2, got 0x%08x", v);
    v = icb_read(0x0C);
    CHECK(v == 0xD3D3D3D3u, "reg 3 readback should be 0xD3D3D3D3, got 0x%08x", v);

    // ── Test 4: Input pins read at indices 8 and 9 ───────────────────
    printf("Test 4: Input pin reads\n");
    dut->fio_in_0 = 0x12121212u;
    dut->fio_in_1 = 0x34343434u;
    v = icb_read(0x20);             // reg 8
    CHECK(v == 0x12121212u, "fio_in_0 read (idx 8) should be 0x12121212, got 0x%08x", v);
    v = icb_read(0x24);             // reg 9
    CHECK(v == 0x34343434u, "fio_in_1 read (idx 9) should be 0x34343434, got 0x%08x", v);

    // ── Test 5: addr[5:2] indexing ignores byte offset and high bits ─
    printf("Test 5: Address decoding\n");
    v = icb_read(0x00000042u);      // bits[5:2] of 0x42 = 0 -> reg 0... 0x42>>2 = 0x10 & 0xF = 0
    CHECK(v == 0xA0A0A0A0u, "0x42 should decode to reg 0 (addr[5:2]=0), got 0x%08x", v);
    v = icb_read(0x00000107u);      // bits[5:2] of 0x107 = 1 -> reg 1, byte offset ignored
    CHECK(v == 0xB1B1B1B1u, "0x107 should decode to reg 1, got 0x%08x", v);

    // ── Test 6: Unmapped indices read 0 ──────────────────────────────
    printf("Test 6: Unmapped indices\n");
    v = icb_read(0x10);             // reg 4: not mapped
    CHECK(v == 0, "reg 4 should read 0, got 0x%08x", v);
    v = icb_read(0x3C);             // reg 15: not mapped
    CHECK(v == 0, "reg 15 should read 0, got 0x%08x", v);
    // Writes to unmapped indices must not disturb the mapped registers.
    icb_write(0x10, 0xEEEEEEEEu);
    CHECK(dut->fio_out_0 == 0xA0A0A0A0u && dut->fio_out_1 == 0xB1B1B1B1u &&
          dut->fio_out_2 == 0xC2C2C2C2u && dut->fio_out_3 == 0xD3D3D3D3u,
          "an unmapped write must not disturb regs 0-3");

    // ── Test 7: rsp_valid gap with no command ────────────────────────
    printf("Test 7: Response gap\n");
    tick();
    dut->eval();
    CHECK(dut->icb_rsp_valid == 0, "rsp_valid should drop when no command was presented, got %d",
          dut->icb_rsp_valid);

    printf("\n=== e203_fio: %d failure(s) ===\n", fail_count);
    return fail_count ? 1 : 0;
}
