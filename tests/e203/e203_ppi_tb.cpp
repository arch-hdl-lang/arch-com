// ARCH sim testbench for e203_ppi — ICB-to-APB bridge with address-based
// demux to four APB slaves (GPIO 0x10012xxx, UART 0x10013xxx, SPI 0x10014xxx,
// Timer 0x02xxxxxx). Tests: reset state, the IDLE->SETUP->ACCESS APB protocol
// sequencing (psel before penable), address decode steering to each slave,
// read-data muxing, write forwarding, APB wait states, response holding under
// ICB backpressure, cmd_ready lockout while a response is pending, and the
// unmapped-address fallthrough.
//
// NOTE: this replaces a stale tb (VPpi.h) that targeted the pre-2026-04
// PascalCase fixture naming. The construct is now `e203_ppi`, so the sim
// class is Ve203_ppi.
//
// Run with:
//   arch sim tests/e203/e203_ppi.arch --tb tests/e203/e203_ppi_tb.cpp

#include "Ve203_ppi.h"
#include <cstdio>
#include <cstdint>

static int fail_count = 0;

#define CHECK(cond, fmt, ...) do { \
    if (!(cond)) { \
        printf("  FAIL: " fmt "\n", ##__VA_ARGS__); \
        fail_count++; \
    } \
} while(0)

static Ve203_ppi* dut;

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
    dut->icb_cmd_read = 0;
    dut->icb_rsp_ready = 1;
    dut->apb0_prdata = 0; dut->apb0_pready = 1;
    dut->apb1_prdata = 0; dut->apb1_pready = 1;
    dut->apb2_prdata = 0; dut->apb2_pready = 1;
    dut->apb3_prdata = 0; dut->apb3_pready = 1;
    for (int i = 0; i < 3; i++) tick();
    dut->rst_n = 1;
    dut->eval();
}

// Issue one ICB command and step to the SETUP state (fsm 0 -> 1).
static void icb_cmd(uint32_t addr, uint32_t wdata, int read) {
    dut->icb_cmd_valid = 1;
    dut->icb_cmd_addr = addr;
    dut->icb_cmd_wdata = wdata;
    dut->icb_cmd_read = read;
    dut->eval();
    CHECK(dut->icb_cmd_ready == 1, "cmd_ready should be 1 in IDLE, got %d", dut->icb_cmd_ready);
    tick();                        // accept: latch cmd, enter SETUP
    dut->icb_cmd_valid = 0;
    dut->eval();
}

int main() {
    dut = new Ve203_ppi;

    // ── Test 1: Reset state ──────────────────────────────────────────
    printf("Test 1: Reset state\n");
    reset();
    CHECK(dut->icb_cmd_ready == 1, "cmd_ready should be 1 after reset, got %d", dut->icb_cmd_ready);
    CHECK(dut->icb_rsp_valid == 0, "rsp_valid should be 0 after reset, got %d", dut->icb_rsp_valid);
    CHECK(dut->icb_rsp_err == 0, "rsp_err is tied low in this design, got %d", dut->icb_rsp_err);
    CHECK(dut->apb0_psel == 0 && dut->apb1_psel == 0 && dut->apb2_psel == 0 && dut->apb3_psel == 0,
          "no psel should assert at idle");
    CHECK(dut->apb0_penable == 0 && dut->apb1_penable == 0 && dut->apb2_penable == 0 &&
          dut->apb3_penable == 0, "no penable should assert at idle");

    // ── Test 2: GPIO read with full APB sequencing ───────────────────
    printf("Test 2: GPIO read (apb0)\n");
    reset();
    dut->apb0_prdata = 0xCAFE0001u;
    icb_cmd(0x10012004u, 0, 1);
    // SETUP: psel high, penable low, address/direction forwarded.
    CHECK(dut->apb0_psel == 1, "apb0_psel should be 1 in SETUP, got %d", dut->apb0_psel);
    CHECK(dut->apb0_penable == 0, "apb0_penable should be 0 in SETUP, got %d", dut->apb0_penable);
    CHECK(dut->apb0_paddr == 0x10012004u, "apb0_paddr should be 0x10012004, got 0x%08x",
          dut->apb0_paddr);
    CHECK(dut->apb0_pwrite == 0, "apb0_pwrite should be 0 for a read, got %d", dut->apb0_pwrite);
    CHECK(dut->apb1_psel == 0 && dut->apb2_psel == 0 && dut->apb3_psel == 0,
          "only apb0 may be selected for a GPIO address");
    tick();                        // SETUP -> ACCESS
    dut->eval();
    CHECK(dut->apb0_psel == 1, "apb0_psel should stay 1 in ACCESS, got %d", dut->apb0_psel);
    CHECK(dut->apb0_penable == 1, "apb0_penable should be 1 in ACCESS, got %d", dut->apb0_penable);
    tick();                        // pready=1: capture rdata, respond
    dut->eval();
    CHECK(dut->icb_rsp_valid == 1, "rsp_valid should assert after ACCESS, got %d", dut->icb_rsp_valid);
    CHECK(dut->icb_rsp_rdata == 0xCAFE0001u, "rsp_rdata should be 0xCAFE0001, got 0x%08x",
          dut->icb_rsp_rdata);
    CHECK(dut->apb0_psel == 0, "apb0_psel should drop after the access, got %d", dut->apb0_psel);
    tick();                        // rsp_ready=1: response consumed
    dut->eval();
    CHECK(dut->icb_rsp_valid == 0, "rsp_valid should clear once accepted, got %d", dut->icb_rsp_valid);

    // ── Test 3: UART write forwarding ────────────────────────────────
    printf("Test 3: UART write (apb1)\n");
    reset();
    icb_cmd(0x10013010u, 0xDDCCBBAAu, 0);
    CHECK(dut->apb1_psel == 1, "apb1_psel should be 1 in SETUP, got %d", dut->apb1_psel);
    CHECK(dut->apb1_pwrite == 1, "apb1_pwrite should be 1 for a write, got %d", dut->apb1_pwrite);
    CHECK(dut->apb1_pwdata == 0xDDCCBBAAu, "apb1_pwdata should be 0xDDCCBBAA, got 0x%08x",
          dut->apb1_pwdata);
    CHECK(dut->apb0_psel == 0 && dut->apb2_psel == 0 && dut->apb3_psel == 0,
          "only apb1 may be selected for a UART address");
    tick();                        // ACCESS
    dut->eval();
    CHECK(dut->apb1_penable == 1, "apb1_penable should be 1 in ACCESS, got %d", dut->apb1_penable);
    tick();
    dut->eval();
    CHECK(dut->icb_rsp_valid == 1, "write should complete with a response, got rsp_valid=%d",
          dut->icb_rsp_valid);
    tick(); dut->eval();

    // ── Test 4: SPI and Timer decode ─────────────────────────────────
    printf("Test 4: SPI (apb2) and Timer (apb3) decode\n");
    reset();
    dut->apb2_prdata = 0x22222222u;
    icb_cmd(0x10014000u, 0, 1);
    CHECK(dut->apb2_psel == 1, "apb2_psel should be 1 for an SPI address, got %d", dut->apb2_psel);
    CHECK(dut->apb0_psel == 0 && dut->apb1_psel == 0 && dut->apb3_psel == 0,
          "only apb2 may be selected for an SPI address");
    tick(); tick();
    dut->eval();
    CHECK(dut->icb_rsp_rdata == 0x22222222u, "SPI read should return 0x22222222, got 0x%08x",
          dut->icb_rsp_rdata);
    tick(); dut->eval();

    dut->apb3_prdata = 0x33333333u;
    icb_cmd(0x0200BFF8u, 0, 1);    // CLINT mtime region
    CHECK(dut->apb3_psel == 1, "apb3_psel should be 1 for a timer address, got %d", dut->apb3_psel);
    CHECK(dut->apb0_psel == 0 && dut->apb1_psel == 0 && dut->apb2_psel == 0,
          "only apb3 may be selected for a timer address");
    tick(); tick();
    dut->eval();
    CHECK(dut->icb_rsp_rdata == 0x33333333u, "timer read should return 0x33333333, got 0x%08x",
          dut->icb_rsp_rdata);
    tick(); dut->eval();

    // ── Test 5: APB wait states hold ACCESS ──────────────────────────
    printf("Test 5: APB wait states\n");
    reset();
    dut->apb0_pready = 0;
    dut->apb0_prdata = 0x55AA55AAu;
    icb_cmd(0x10012000u, 0, 1);
    tick();                        // SETUP -> ACCESS
    for (int i = 0; i < 3; i++) {
        tick(); dut->eval();
        CHECK(dut->apb0_psel == 1 && dut->apb0_penable == 1,
              "ACCESS must hold psel+penable while pready=0 (cycle %d)", i);
        CHECK(dut->icb_rsp_valid == 0, "no response while the slave stalls (cycle %d)", i);
    }
    dut->apb0_pready = 1;
    tick();
    dut->eval();
    CHECK(dut->icb_rsp_valid == 1, "rsp_valid should assert once pready rises, got %d",
          dut->icb_rsp_valid);
    CHECK(dut->icb_rsp_rdata == 0x55AA55AAu, "rsp_rdata should be 0x55AA55AA, got 0x%08x",
          dut->icb_rsp_rdata);
    tick(); dut->eval();

    // ── Test 6: Response held under ICB backpressure ─────────────────
    printf("Test 6: ICB backpressure\n");
    reset();
    dut->apb0_prdata = 0x12345678u;
    dut->icb_rsp_ready = 0;
    icb_cmd(0x10012008u, 0, 1);
    tick(); tick();                // through SETUP+ACCESS
    dut->eval();
    CHECK(dut->icb_rsp_valid == 1, "rsp_valid should assert, got %d", dut->icb_rsp_valid);
    for (int i = 0; i < 3; i++) {
        // While the response is pending, no new command may be accepted.
        CHECK(dut->icb_cmd_ready == 0, "cmd_ready must be 0 while a response is pending (cycle %d)", i);
        tick(); dut->eval();
        CHECK(dut->icb_rsp_valid == 1, "rsp_valid must hold under backpressure (cycle %d)", i);
        CHECK(dut->icb_rsp_rdata == 0x12345678u, "rsp_rdata must hold under backpressure (cycle %d)", i);
    }
    dut->icb_rsp_ready = 1;
    tick(); dut->eval();
    CHECK(dut->icb_rsp_valid == 0, "rsp_valid should clear once rsp_ready rises, got %d",
          dut->icb_rsp_valid);
    CHECK(dut->icb_cmd_ready == 1, "cmd_ready should return once the response drains, got %d",
          dut->icb_cmd_ready);

    // ── Test 7: Unmapped address returns zero data ───────────────────
    printf("Test 7: Unmapped address\n");
    reset();
    dut->apb0_prdata = 0xFFFFFFFFu;   // make sure nothing leaks through the mux
    dut->apb1_prdata = 0xFFFFFFFFu;
    dut->apb2_prdata = 0xFFFFFFFFu;
    dut->apb3_prdata = 0xFFFFFFFFu;
    icb_cmd(0xDEAD0000u, 0, 1);
    CHECK(dut->apb0_psel == 0 && dut->apb1_psel == 0 && dut->apb2_psel == 0 && dut->apb3_psel == 0,
          "no slave may be selected for an unmapped address");
    tick(); tick();                // sel_pready falls through to true
    dut->eval();
    CHECK(dut->icb_rsp_valid == 1, "unmapped access should still complete, got rsp_valid=%d",
          dut->icb_rsp_valid);
    CHECK(dut->icb_rsp_rdata == 0, "unmapped read data should be 0, got 0x%08x", dut->icb_rsp_rdata);
    CHECK(dut->icb_rsp_err == 0, "rsp_err stays 0 in this design, got %d", dut->icb_rsp_err);

    printf("\n=== e203_ppi: %d failure(s) ===\n", fail_count);
    return fail_count ? 1 : 0;
}
