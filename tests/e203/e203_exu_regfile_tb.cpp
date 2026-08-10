// ARCH sim testbench for e203_exu_regfile — E203 integer register file.
// Tests: x0 hardwired to zero on both read ports (reads 0, writes discarded),
// sync write / async read of all 31 real registers on both ports, dual-port
// independent reads, write-enable gating, no write-through (old value visible
// in the write cycle, new value after the edge), overwrite, and the dedicated
// x1 (ra) link-register output.
//
// NOTE: this replaces a stale tb (VExuRegfile.h) that targeted an earlier
// revision of the fixture before the e203 fixtures were rewritten against the
// real E203 RTL, which renamed the construct to `e203_exu_regfile`. The old
// tb has not compiled since. Ported to the current class name
// (Ve203_exu_regfile). The current-generation e203_exu_regfile_vltor_tb.cpp
// covers the Verilator flavor and is untouched.
//
// Run with:
//   arch sim tests/e203/e203_exu_regfile.arch --tb tests/e203/e203_exu_regfile_tb.cpp

#include "Ve203_exu_regfile.h"
#include <cstdio>
#include <cstdint>

static int fail_count = 0;

#define CHECK(cond, fmt, ...) do { \
    if (!(cond)) { \
        printf("  FAIL: " fmt "\n", ##__VA_ARGS__); \
        fail_count++; \
    } \
} while(0)

static Ve203_exu_regfile* dut;

static void tick() {
    dut->clk = 0; dut->eval();
    dut->clk = 1; dut->eval();
}

static void reset() {
    dut->rst_n = 0;
    dut->test_mode = 0;
    dut->read_src1_idx = 0;
    dut->read_src2_idx = 0;
    dut->wbck_dest_wen = 0;
    dut->wbck_dest_idx = 0;
    dut->wbck_dest_dat = 0;
    for (int i = 0; i < 3; i++) tick();
    dut->rst_n = 1;
    dut->eval();
}

// Write one register through the sync write port.
static void write_reg(uint8_t idx, uint32_t dat) {
    dut->wbck_dest_wen = 1;
    dut->wbck_dest_idx = idx;
    dut->wbck_dest_dat = dat;
    dut->eval();
    tick();
    dut->wbck_dest_wen = 0;
    dut->eval();
}

// Async read via port 1.
static uint32_t read1(uint8_t idx) {
    dut->read_src1_idx = idx;
    dut->eval();
    return dut->read_src1_dat;
}

// Async read via port 2.
static uint32_t read2(uint8_t idx) {
    dut->read_src2_idx = idx;
    dut->eval();
    return dut->read_src2_dat;
}

int main() {
    dut = new Ve203_exu_regfile;

    // ── Test 1: x0 reads zero and discards writes ────────────────────
    printf("Test 1: x0 hardwired zero\n");
    reset();
    CHECK(read1(0) == 0, "x0 should read 0 on port 1, got 0x%08x", dut->read_src1_dat);
    CHECK(read2(0) == 0, "x0 should read 0 on port 2, got 0x%08x", dut->read_src2_dat);
    write_reg(0, 0xDEADBEEFu);       // attempt to write x0
    CHECK(read1(0) == 0, "x0 must stay 0 after a write attempt, got 0x%08x", dut->read_src1_dat);
    CHECK(read2(0) == 0, "x0 must stay 0 on port 2 too, got 0x%08x", dut->read_src2_dat);

    // ── Test 2: Write/read every real register on both ports ─────────
    printf("Test 2: All 31 registers\n");
    reset();
    for (uint32_t i = 1; i < 32; i++) {
        write_reg(i, 0xA0000000u + i * 0x01010101u);
    }
    for (uint32_t i = 1; i < 32; i++) {
        uint32_t exp = 0xA0000000u + i * 0x01010101u;
        CHECK(read1(i) == exp, "x%u port1 should read 0x%08x, got 0x%08x", i, exp, dut->read_src1_dat);
        CHECK(read2(i) == exp, "x%u port2 should read 0x%08x, got 0x%08x", i, exp, dut->read_src2_dat);
    }

    // ── Test 3: Dual-port independence ───────────────────────────────
    printf("Test 3: Dual-port reads\n");
    dut->read_src1_idx = 5;
    dut->read_src2_idx = 17;
    dut->eval();
    CHECK(dut->read_src1_dat == 0xA0000000u + 5 * 0x01010101u,
          "port1 x5 wrong while port2 reads x17, got 0x%08x", dut->read_src1_dat);
    CHECK(dut->read_src2_dat == 0xA0000000u + 17 * 0x01010101u,
          "port2 x17 wrong while port1 reads x5, got 0x%08x", dut->read_src2_dat);

    // ── Test 4: Write-enable gating ──────────────────────────────────
    printf("Test 4: Write-enable gating\n");
    reset();
    write_reg(9, 0x11111111u);
    // wen low: data/idx present but no write may happen.
    dut->wbck_dest_wen = 0;
    dut->wbck_dest_idx = 9;
    dut->wbck_dest_dat = 0x22222222u;
    tick();
    CHECK(read1(9) == 0x11111111u, "x9 must hold with wen=0, got 0x%08x", dut->read_src1_dat);

    // ── Test 5: No write-through; update visible after the edge ──────
    printf("Test 5: Sync write timing\n");
    reset();
    write_reg(12, 0x0BADF00Du);
    dut->wbck_dest_wen = 1;
    dut->wbck_dest_idx = 12;
    dut->wbck_dest_dat = 0xFEEDC0DEu;
    dut->read_src1_idx = 12;
    dut->eval();
    // Before the clock edge the read port must still show the old value.
    CHECK(dut->read_src1_dat == 0x0BADF00Du,
          "async read must not write-through before the edge, got 0x%08x", dut->read_src1_dat);
    tick();
    dut->wbck_dest_wen = 0;
    dut->eval();
    CHECK(dut->read_src1_dat == 0xFEEDC0DEu,
          "new value visible after the edge, got 0x%08x", dut->read_src1_dat);
    // Overwrite again.
    write_reg(12, 0x5A5A5A5Au);
    CHECK(read1(12) == 0x5A5A5A5Au, "overwrite should land, got 0x%08x", dut->read_src1_dat);

    // ── Test 6: x1 link-register output ──────────────────────────────
    printf("Test 6: x1 output\n");
    reset();
    write_reg(1, 0x00000080u);       // ra = boot return address
    CHECK(dut->x1_r == 0x80, "x1_r should track rf[1], got 0x%08x", dut->x1_r);
    CHECK(read1(1) == 0x80, "port1 x1 should agree, got 0x%08x", dut->read_src1_dat);
    write_reg(1, 0x00000200u);
    CHECK(dut->x1_r == 0x200, "x1_r should update with rf[1], got 0x%08x", dut->x1_r);
    // Writing another register must not disturb x1_r.
    write_reg(2, 0xFFFFFFFFu);
    CHECK(dut->x1_r == 0x200, "x1_r must not change on an x2 write, got 0x%08x", dut->x1_r);

    printf("\n=== e203_exu_regfile: %d failure(s) ===\n", fail_count);
    return fail_count ? 1 : 0;
}
