// ARCH sim testbench for e203_icb2apb — ICB-to-APB protocol bridge with an
// IDLE->SETUP->ACCESS FSM. Tests: reset state, APB two-phase sequencing (psel
// asserted one cycle before penable), read transactions returning prdata,
// write transactions forwarding wdata/wstrb, APB wait-state stretching,
// pslverr propagation to icb_rsp_err, response holding under ICB
// backpressure, and cmd_ready lockout while a response is pending.
//
// NOTE: this replaces a stale tb (VIcb2Apb.h) that targeted the pre-2026-04
// PascalCase fixture naming. The construct is now `e203_icb2apb`, so the sim
// class is Ve203_icb2apb.
//
// Run with:
//   arch sim tests/e203/e203_icb2apb.arch --tb tests/e203/e203_icb2apb_tb.cpp

#include "Ve203_icb2apb.h"
#include <cstdio>
#include <cstdint>

static int fail_count = 0;

#define CHECK(cond, fmt, ...) do { \
    if (!(cond)) { \
        printf("  FAIL: " fmt "\n", ##__VA_ARGS__); \
        fail_count++; \
    } \
} while(0)

static Ve203_icb2apb* dut;

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
    dut->prdata = 0;
    dut->pready = 1;
    dut->pslverr = 0;
    for (int i = 0; i < 3; i++) tick();
    dut->rst_n = 1;
    dut->eval();
}

// Issue one ICB command and take the accept edge (fsm IDLE -> SETUP).
static void icb_cmd(uint32_t addr, uint32_t wdata, uint32_t wmask, int read) {
    dut->icb_cmd_valid = 1;
    dut->icb_cmd_addr = addr;
    dut->icb_cmd_wdata = wdata;
    dut->icb_cmd_wmask = wmask;
    dut->icb_cmd_read = read;
    dut->eval();
    CHECK(dut->icb_cmd_ready == 1, "cmd_ready should be 1 in IDLE, got %d", dut->icb_cmd_ready);
    tick();
    dut->icb_cmd_valid = 0;
    dut->eval();
}

int main() {
    dut = new Ve203_icb2apb;

    // ── Test 1: Reset state ──────────────────────────────────────────
    printf("Test 1: Reset state\n");
    reset();
    CHECK(dut->icb_cmd_ready == 1, "cmd_ready should be 1 after reset, got %d", dut->icb_cmd_ready);
    CHECK(dut->icb_rsp_valid == 0, "rsp_valid should be 0 after reset, got %d", dut->icb_rsp_valid);
    CHECK(dut->psel == 0, "psel should be 0 in IDLE, got %d", dut->psel);
    CHECK(dut->penable == 0, "penable should be 0 in IDLE, got %d", dut->penable);

    // ── Test 2: Read transaction with two-phase APB sequencing ───────
    printf("Test 2: APB read\n");
    reset();
    dut->prdata = 0x600DF00Du;
    icb_cmd(0x10012000u, 0, 0xF, 1);
    // SETUP phase.
    CHECK(dut->psel == 1, "psel should be 1 in SETUP, got %d", dut->psel);
    CHECK(dut->penable == 0, "penable should be 0 in SETUP, got %d", dut->penable);
    CHECK(dut->paddr == 0x10012000u, "paddr should be 0x10012000, got 0x%08x", dut->paddr);
    CHECK(dut->pwrite == 0, "pwrite should be 0 for a read, got %d", dut->pwrite);
    CHECK(dut->icb_cmd_ready == 0, "cmd_ready must drop mid-transaction, got %d", dut->icb_cmd_ready);
    tick();                        // SETUP -> ACCESS
    dut->eval();
    CHECK(dut->psel == 1, "psel should stay 1 in ACCESS, got %d", dut->psel);
    CHECK(dut->penable == 1, "penable should be 1 in ACCESS, got %d", dut->penable);
    tick();                        // pready=1: capture and respond
    dut->eval();
    CHECK(dut->icb_rsp_valid == 1, "rsp_valid should assert after ACCESS, got %d", dut->icb_rsp_valid);
    CHECK(dut->icb_rsp_rdata == 0x600DF00Du, "rsp_rdata should be 0x600DF00D, got 0x%08x",
          dut->icb_rsp_rdata);
    CHECK(dut->icb_rsp_err == 0, "rsp_err should be 0 for a clean access, got %d", dut->icb_rsp_err);
    CHECK(dut->psel == 0 && dut->penable == 0, "APB signals should drop after the access");
    tick();                        // response consumed (rsp_ready=1)
    dut->eval();
    CHECK(dut->icb_rsp_valid == 0, "rsp_valid should clear once accepted, got %d", dut->icb_rsp_valid);
    CHECK(dut->icb_cmd_ready == 1, "cmd_ready should return after the response, got %d",
          dut->icb_cmd_ready);

    // ── Test 3: Write transaction forwards wdata and strobes ─────────
    printf("Test 3: APB write\n");
    reset();
    icb_cmd(0x10013004u, 0x12AB34CDu, 0x3, 0);
    CHECK(dut->pwrite == 1, "pwrite should be 1 for a write, got %d", dut->pwrite);
    CHECK(dut->pwdata == 0x12AB34CDu, "pwdata should be 0x12AB34CD, got 0x%08x", dut->pwdata);
    CHECK(dut->pstrb == 0x3, "pstrb should forward wmask 0x3, got 0x%x", dut->pstrb);
    tick();                        // ACCESS
    dut->eval();
    CHECK(dut->penable == 1, "penable should be 1 in ACCESS, got %d", dut->penable);
    tick();
    dut->eval();
    CHECK(dut->icb_rsp_valid == 1, "write should complete with a response, got %d",
          dut->icb_rsp_valid);
    tick(); dut->eval();

    // ── Test 4: APB wait states stretch ACCESS ───────────────────────
    printf("Test 4: Wait states\n");
    reset();
    dut->pready = 0;
    dut->prdata = 0x0BADBEEFu;
    icb_cmd(0x10012004u, 0, 0xF, 1);
    tick();                        // SETUP -> ACCESS
    for (int i = 0; i < 4; i++) {
        tick(); dut->eval();
        CHECK(dut->psel == 1 && dut->penable == 1,
              "ACCESS must hold psel+penable while pready=0 (cycle %d)", i);
        CHECK(dut->icb_rsp_valid == 0, "no response while the slave stalls (cycle %d)", i);
    }
    dut->pready = 1;
    tick();
    dut->eval();
    CHECK(dut->icb_rsp_valid == 1, "rsp_valid should assert once pready rises, got %d",
          dut->icb_rsp_valid);
    CHECK(dut->icb_rsp_rdata == 0x0BADBEEFu, "rsp_rdata should be 0x0BADBEEF, got 0x%08x",
          dut->icb_rsp_rdata);
    tick(); dut->eval();

    // ── Test 5: pslverr propagates to icb_rsp_err ────────────────────
    printf("Test 5: Slave error\n");
    reset();
    dut->pslverr = 1;
    icb_cmd(0x10012008u, 0, 0xF, 1);
    tick(); tick();                // SETUP + ACCESS(pready=1)
    dut->eval();
    CHECK(dut->icb_rsp_valid == 1, "errored access should still respond, got %d", dut->icb_rsp_valid);
    CHECK(dut->icb_rsp_err == 1, "rsp_err should carry pslverr, got %d", dut->icb_rsp_err);
    dut->pslverr = 0;
    tick(); dut->eval();

    // ── Test 6: Response held under ICB backpressure ─────────────────
    printf("Test 6: ICB backpressure\n");
    reset();
    dut->prdata = 0x13579BDFu;
    dut->icb_rsp_ready = 0;
    icb_cmd(0x1001200Cu, 0, 0xF, 1);
    tick(); tick();
    dut->eval();
    CHECK(dut->icb_rsp_valid == 1, "rsp_valid should assert, got %d", dut->icb_rsp_valid);
    for (int i = 0; i < 3; i++) {
        CHECK(dut->icb_cmd_ready == 0, "cmd_ready must be 0 while a response is pending (cycle %d)", i);
        tick(); dut->eval();
        CHECK(dut->icb_rsp_valid == 1, "rsp_valid must hold under backpressure (cycle %d)", i);
        CHECK(dut->icb_rsp_rdata == 0x13579BDFu, "rsp_rdata must hold under backpressure (cycle %d)", i);
    }
    dut->icb_rsp_ready = 1;
    tick(); dut->eval();
    CHECK(dut->icb_rsp_valid == 0, "rsp_valid should clear once rsp_ready rises, got %d",
          dut->icb_rsp_valid);
    CHECK(dut->icb_cmd_ready == 1, "cmd_ready should return once the response drains, got %d",
          dut->icb_cmd_ready);

    printf("\n=== e203_icb2apb: %d failure(s) ===\n", fail_count);
    return fail_count ? 1 : 0;
}
