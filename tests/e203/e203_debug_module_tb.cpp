// ARCH sim testbench for e203_debug_module — minimal RISC-V Debug Spec 0.13
// debug module behind an APB slave. Tests: reset state, dmcontrol write +
// readback with dmactive gating of halt_req, the one-cycle resumereq pulse,
// dmstatus mirroring of the hart halted/running inputs, data0 write/readback,
// abstract register write commands (dbg_reg_wen/addr/wdata) and their
// one-cycle auto-clear, abstract register read commands capturing
// dbg_reg_rdata into data0, and unmapped-address reads.
//
// NOTE: this replaces a stale tb (VDebugModule.h) that targeted the
// pre-2026-04 PascalCase fixture naming. The construct is now
// `e203_debug_module`, so the sim class is Ve203_debug_module.
//
// The abstractcs (0x12) source comment says "busy bit (bit 12)" but the code
// emits {cmd_valid_r, 31'b0}, which puts busy at bit 31. This tb checks the
// code's actual bit position.
//
// Run with:
//   arch sim tests/e203/e203_debug_module.arch --tb tests/e203/e203_debug_module_tb.cpp

#include "Ve203_debug_module.h"
#include <cstdio>
#include <cstdint>

static int fail_count = 0;

#define CHECK(cond, fmt, ...) do { \
    if (!(cond)) { \
        printf("  FAIL: " fmt "\n", ##__VA_ARGS__); \
        fail_count++; \
    } \
} while(0)

static Ve203_debug_module* dut;

static void tick() {
    dut->clk = 0; dut->eval();
    dut->clk = 1; dut->eval();
}

static void reset() {
    dut->rst_n = 0;
    dut->psel = 0;
    dut->penable = 0;
    dut->paddr = 0;
    dut->pwdata = 0;
    dut->pwrite = 0;
    dut->hart_halted = 0;
    dut->hart_running = 1;
    dut->dbg_reg_rdata = 0;
    for (int i = 0; i < 3; i++) tick();
    dut->rst_n = 1;
    dut->eval();
}

static void apb_write(uint32_t addr, uint32_t data) {
    dut->psel = 1; dut->penable = 0;
    dut->paddr = addr; dut->pwdata = data; dut->pwrite = 1;
    tick();
    dut->penable = 1;
    tick();
    dut->psel = 0; dut->penable = 0; dut->pwrite = 0;
    dut->eval();
}

static uint32_t apb_read(uint32_t addr) {
    dut->psel = 1; dut->penable = 0;
    dut->paddr = addr; dut->pwrite = 0;
    tick();
    dut->penable = 1;
    dut->eval();
    uint32_t v = dut->prdata;
    tick();
    dut->psel = 0; dut->penable = 0;
    dut->eval();
    return v;
}

// prdata decodes paddr combinationally (no psel qualification in this
// design), so a zero-cost peek avoids burning clock edges on registers that
// auto-clear after one cycle.
static uint32_t peek(uint32_t addr) {
    dut->paddr = addr;
    dut->eval();
    return dut->prdata;
}

int main() {
    dut = new Ve203_debug_module;

    // ── Test 1: Reset state ──────────────────────────────────────────
    printf("Test 1: Reset state\n");
    reset();
    CHECK(dut->halt_req == 0, "halt_req should be 0 after reset, got %d", dut->halt_req);
    CHECK(dut->resume_req == 0, "resume_req should be 0 after reset, got %d", dut->resume_req);
    CHECK(dut->dbg_reg_wen == 0, "dbg_reg_wen should be 0 after reset, got %d", dut->dbg_reg_wen);
    CHECK(dut->pready == 1, "pready is constant 1 in this design, got %d", dut->pready);
    uint32_t v = apb_read(0x10);
    CHECK(v == 0, "dmcontrol should read 0 after reset, got 0x%08x", v);

    // ── Test 2: haltreq gated by dmactive ────────────────────────────
    printf("Test 2: dmcontrol haltreq/dmactive\n");
    // haltreq without dmactive: latched but the output stays masked.
    apb_write(0x10, 0x80000000u);
    CHECK(dut->halt_req == 0, "halt_req must stay 0 while dmactive=0, got %d", dut->halt_req);
    v = apb_read(0x10);
    CHECK((v >> 31) == 1, "dmcontrol bit 31 (haltreq) should read back 1, got 0x%08x", v);
    CHECK((v & 1) == 0, "dmcontrol bit 0 (dmactive) should read back 0, got 0x%08x", v);
    // haltreq + dmactive: output asserts.
    apb_write(0x10, 0x80000001u);
    CHECK(dut->halt_req == 1, "halt_req should assert with haltreq+dmactive, got %d", dut->halt_req);
    v = apb_read(0x10);
    CHECK((v >> 31) == 1 && (v & 1) == 1, "dmcontrol should read back haltreq+dmactive, got 0x%08x", v);
    // Clearing haltreq drops the output.
    apb_write(0x10, 0x00000001u);
    CHECK(dut->halt_req == 0, "halt_req should drop when haltreq clears, got %d", dut->halt_req);

    // ── Test 3: resumereq is a one-cycle pulse ───────────────────────
    printf("Test 3: resumereq pulse\n");
    reset();
    apb_write(0x10, 0x40000001u);   // dmactive + resumereq
    CHECK(dut->resume_req == 1, "resume_req should assert right after the write, got %d",
          dut->resume_req);
    v = peek(0x10);
    CHECK(((v >> 30) & 1) == 1, "dmcontrol bit 30 (resumereq) should be set this cycle, got 0x%08x", v);
    tick();
    dut->eval();
    CHECK(dut->resume_req == 0, "resume_req should auto-clear after one cycle, got %d",
          dut->resume_req);
    v = peek(0x10);
    CHECK(((v >> 30) & 1) == 0, "dmcontrol bit 30 should auto-clear, got 0x%08x", v);

    // ── Test 4: dmstatus mirrors hart state ──────────────────────────
    printf("Test 4: dmstatus\n");
    reset();
    dut->hart_halted = 0; dut->hart_running = 1;
    v = apb_read(0x11);
    CHECK(((v >> 8) & 0x3) == 0, "halted bits [9:8] should be 0 while running, got 0x%08x", v);
    CHECK(((v >> 2) & 0x3) == 0x3, "running bits [3:2] should be set while running, got 0x%08x", v);
    dut->hart_halted = 1; dut->hart_running = 0;
    v = apb_read(0x11);
    CHECK(((v >> 8) & 0x3) == 0x3, "halted bits [9:8] should be set while halted, got 0x%08x", v);
    CHECK(((v >> 2) & 0x3) == 0, "running bits [3:2] should be 0 while halted, got 0x%08x", v);

    // ── Test 5: data0 write/readback ─────────────────────────────────
    printf("Test 5: data0\n");
    reset();
    apb_write(0x04, 0xFACE1234u);
    v = apb_read(0x04);
    CHECK(v == 0xFACE1234u, "data0 readback should be 0xFACE1234, got 0x%08x", v);

    // ── Test 6: Abstract register write command ──────────────────────
    printf("Test 6: Abstract write command\n");
    reset();
    apb_write(0x04, 0x11223344u);   // payload into data0
    apb_write(0x17, 0x00010000u | 0x1002u);   // bit16=write, reg addr 0x1002
    CHECK(dut->dbg_reg_wen == 1, "dbg_reg_wen should assert for a write command, got %d",
          dut->dbg_reg_wen);
    CHECK(dut->dbg_reg_addr == 0x1002, "dbg_reg_addr should be 0x1002, got 0x%04x",
          dut->dbg_reg_addr);
    CHECK(dut->dbg_reg_wdata == 0x11223344u, "dbg_reg_wdata should be data0, got 0x%08x",
          dut->dbg_reg_wdata);
    v = peek(0x12);
    CHECK((v >> 31) == 1, "abstractcs busy should be set during the command, got 0x%08x", v);
    tick();
    dut->eval();
    CHECK(dut->dbg_reg_wen == 0, "dbg_reg_wen should auto-clear after one cycle, got %d",
          dut->dbg_reg_wen);
    v = peek(0x12);
    CHECK((v >> 31) == 0, "abstractcs busy should clear after one cycle, got 0x%08x", v);

    // ── Test 7: Abstract register read command ───────────────────────
    printf("Test 7: Abstract read command\n");
    reset();
    dut->dbg_reg_rdata = 0xBEEF5678u;
    apb_write(0x17, 0x00000FEDu);   // bit16=0: read reg 0xFED
    CHECK(dut->dbg_reg_wen == 0, "dbg_reg_wen must stay 0 for a read command, got %d",
          dut->dbg_reg_wen);
    CHECK(dut->dbg_reg_addr == 0x0FED, "dbg_reg_addr should be 0x0FED, got 0x%04x",
          dut->dbg_reg_addr);
    tick();                          // capture dbg_reg_rdata into data0
    dut->eval();
    v = apb_read(0x04);
    CHECK(v == 0xBEEF5678u, "data0 should capture dbg_reg_rdata, got 0x%08x", v);

    // ── Test 8: Unmapped address reads 0 ─────────────────────────────
    printf("Test 8: Unmapped address\n");
    v = apb_read(0x20);
    CHECK(v == 0, "unmapped offset 0x20 should read 0, got 0x%08x", v);

    printf("\n=== e203_debug_module: %d failure(s) ===\n", fail_count);
    return fail_count ? 1 : 0;
}
