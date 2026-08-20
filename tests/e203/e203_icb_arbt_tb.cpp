// ARCH sim testbench for e203_icb_arbt — 2-master round-robin ICB arbiter.
// Tests: reset state, solo-master command forwarding (addr/wdata/wmask/read)
// with ready steering, response demux by transaction owner (rsp_owner),
// rsp_ready backrouting from the owning master, error forwarding,
// round-robin alternation when both masters request, and s_cmd_ready gating
// of the grant handshake.
//
// NOTE: this replaces a stale tb (VIcbArbt.h) that targeted the pre-2026-04
// PascalCase fixture naming. The construct is now `e203_icb_arbt`, so the sim
// class is Ve203_icb_arbt.
//
// Run with:
//   arch sim tests/e203/e203_icb_arbt.arch --tb tests/e203/e203_icb_arbt_tb.cpp

#include "Ve203_icb_arbt.h"
#include <cstdio>
#include <cstdint>

static int fail_count = 0;

#define CHECK(cond, fmt, ...) do { \
    if (!(cond)) { \
        printf("  FAIL: " fmt "\n", ##__VA_ARGS__); \
        fail_count++; \
    } \
} while(0)

static Ve203_icb_arbt* dut;

static void tick() {
    dut->clk = 0; dut->eval();
    dut->clk = 1; dut->eval();
}

static void reset() {
    dut->rst_n = 0;
    dut->m0_cmd_valid = 0; dut->m0_cmd_addr = 0; dut->m0_cmd_wdata = 0;
    dut->m0_cmd_wmask = 0xF; dut->m0_cmd_read = 0; dut->m0_rsp_ready = 1;
    dut->m1_cmd_valid = 0; dut->m1_cmd_addr = 0; dut->m1_cmd_wdata = 0;
    dut->m1_cmd_wmask = 0xF; dut->m1_cmd_read = 0; dut->m1_rsp_ready = 1;
    dut->s_cmd_ready = 1;
    dut->s_rsp_valid = 0; dut->s_rsp_rdata = 0; dut->s_rsp_err = 0;
    for (int i = 0; i < 3; i++) tick();
    dut->rst_n = 1;
    dut->eval();
}

int main() {
    dut = new Ve203_icb_arbt;

    // ── Test 1: Reset state ──────────────────────────────────────────
    printf("Test 1: Reset state\n");
    reset();
    CHECK(dut->s_cmd_valid == 0, "s_cmd_valid should be 0 with no requests, got %d", dut->s_cmd_valid);
    CHECK(dut->m0_cmd_ready == 0, "m0_cmd_ready should be 0 with no request, got %d", dut->m0_cmd_ready);
    CHECK(dut->m1_cmd_ready == 0, "m1_cmd_ready should be 0 with no request, got %d", dut->m1_cmd_ready);
    CHECK(dut->m0_rsp_valid == 0 && dut->m1_rsp_valid == 0,
          "no rsp_valid should assert with no slave response");

    // ── Test 2: Solo m0 read command + response routing ──────────────
    printf("Test 2: Solo m0 transaction\n");
    reset();
    dut->m0_cmd_valid = 1;
    dut->m0_cmd_addr = 0x80001000u;
    dut->m0_cmd_read = 1;
    dut->eval();
    CHECK(dut->s_cmd_valid == 1, "s_cmd_valid should follow m0 request, got %d", dut->s_cmd_valid);
    CHECK(dut->s_cmd_addr == 0x80001000u, "s_cmd_addr should be m0's 0x80001000, got 0x%08x",
          dut->s_cmd_addr);
    CHECK(dut->s_cmd_read == 1, "s_cmd_read should follow m0, got %d", dut->s_cmd_read);
    CHECK(dut->m0_cmd_ready == 1, "m0_cmd_ready should be 1 when granted+slave ready, got %d",
          dut->m0_cmd_ready);
    CHECK(dut->m1_cmd_ready == 0, "m1_cmd_ready must stay 0 (not granted), got %d", dut->m1_cmd_ready);
    tick();                        // command handshake: rsp_owner = m0
    dut->m0_cmd_valid = 0;
    dut->s_rsp_valid = 1;
    dut->s_rsp_rdata = 0xAA550011u;
    dut->eval();
    CHECK(dut->m0_rsp_valid == 1, "response should route to m0, got %d", dut->m0_rsp_valid);
    CHECK(dut->m1_rsp_valid == 0, "response must not route to m1, got %d", dut->m1_rsp_valid);
    CHECK(dut->m0_rsp_rdata == 0xAA550011u, "m0_rsp_rdata should be 0xAA550011, got 0x%08x",
          dut->m0_rsp_rdata);
    CHECK(dut->m0_rsp_err == 0, "m0_rsp_err should be 0, got %d", dut->m0_rsp_err);
    CHECK(dut->s_rsp_ready == 1, "s_rsp_ready should mirror m0_rsp_ready=1, got %d", dut->s_rsp_ready);
    dut->m0_rsp_ready = 0;
    dut->eval();
    CHECK(dut->s_rsp_ready == 0, "s_rsp_ready should mirror m0_rsp_ready=0, got %d", dut->s_rsp_ready);
    dut->m0_rsp_ready = 1;
    dut->s_rsp_valid = 0;
    dut->eval();

    // ── Test 3: Solo m1 write command + error response ───────────────
    printf("Test 3: Solo m1 transaction\n");
    reset();
    dut->m1_cmd_valid = 1;
    dut->m1_cmd_addr = 0x10014008u;
    dut->m1_cmd_wdata = 0xDEADBEEFu;
    dut->m1_cmd_wmask = 0x3;
    dut->m1_cmd_read = 0;
    dut->eval();
    CHECK(dut->s_cmd_valid == 1, "s_cmd_valid should follow m1 request, got %d", dut->s_cmd_valid);
    CHECK(dut->s_cmd_addr == 0x10014008u, "s_cmd_addr should be m1's, got 0x%08x", dut->s_cmd_addr);
    CHECK(dut->s_cmd_wdata == 0xDEADBEEFu, "s_cmd_wdata should be m1's, got 0x%08x", dut->s_cmd_wdata);
    CHECK(dut->s_cmd_wmask == 0x3, "s_cmd_wmask should be m1's 0x3, got 0x%x", dut->s_cmd_wmask);
    CHECK(dut->s_cmd_read == 0, "s_cmd_read should be 0 for m1's write, got %d", dut->s_cmd_read);
    CHECK(dut->m1_cmd_ready == 1, "m1_cmd_ready should be 1 when granted, got %d", dut->m1_cmd_ready);
    CHECK(dut->m0_cmd_ready == 0, "m0_cmd_ready must stay 0, got %d", dut->m0_cmd_ready);
    tick();                        // handshake: rsp_owner = m1
    dut->m1_cmd_valid = 0;
    dut->s_rsp_valid = 1;
    dut->s_rsp_rdata = 0;
    dut->s_rsp_err = 1;
    dut->eval();
    CHECK(dut->m1_rsp_valid == 1, "response should route to m1, got %d", dut->m1_rsp_valid);
    CHECK(dut->m0_rsp_valid == 0, "response must not route to m0, got %d", dut->m0_rsp_valid);
    CHECK(dut->m1_rsp_err == 1, "m1_rsp_err should forward the slave error, got %d", dut->m1_rsp_err);
    dut->m1_rsp_ready = 0;
    dut->eval();
    CHECK(dut->s_rsp_ready == 0, "s_rsp_ready should mirror m1_rsp_ready=0, got %d", dut->s_rsp_ready);
    dut->m1_rsp_ready = 1;
    dut->s_rsp_valid = 0;
    dut->s_rsp_err = 0;
    dut->eval();

    // ── Test 4: Round-robin alternation with both requesting ─────────
    printf("Test 4: Round-robin alternation\n");
    reset();
    dut->m0_cmd_valid = 1; dut->m0_cmd_addr = 0xAAAA0000u;
    dut->m1_cmd_valid = 1; dut->m1_cmd_addr = 0xBBBB0000u;
    dut->eval();
    // After reset last_grant=0 (m0 was "last"), so with both requesting the
    // first grant goes to m1.
    CHECK(dut->m1_cmd_ready == 1 && dut->m0_cmd_ready == 0,
          "first contended grant should go to m1 (m0=%d m1=%d)",
          dut->m0_cmd_ready, dut->m1_cmd_ready);
    CHECK(dut->s_cmd_addr == 0xBBBB0000u, "s_cmd_addr should be m1's during its grant, got 0x%08x",
          dut->s_cmd_addr);
    tick();                        // m1 handshake, last_grant=m1
    dut->eval();
    CHECK(dut->m0_cmd_ready == 1 && dut->m1_cmd_ready == 0,
          "second contended grant should alternate to m0 (m0=%d m1=%d)",
          dut->m0_cmd_ready, dut->m1_cmd_ready);
    CHECK(dut->s_cmd_addr == 0xAAAA0000u, "s_cmd_addr should be m0's during its grant, got 0x%08x",
          dut->s_cmd_addr);
    tick();                        // m0 handshake, last_grant=m0
    dut->eval();
    CHECK(dut->m1_cmd_ready == 1 && dut->m0_cmd_ready == 0,
          "third contended grant should alternate back to m1 (m0=%d m1=%d)",
          dut->m0_cmd_ready, dut->m1_cmd_ready);
    dut->m0_cmd_valid = 0; dut->m1_cmd_valid = 0;
    dut->eval();

    // ── Test 5: s_cmd_ready gates the handshake ──────────────────────
    printf("Test 5: Slave backpressure\n");
    reset();
    dut->s_cmd_ready = 0;
    dut->m0_cmd_valid = 1;
    dut->m0_cmd_addr = 0x12340000u;
    dut->eval();
    CHECK(dut->s_cmd_valid == 1, "s_cmd_valid should assert even when the slave stalls, got %d",
          dut->s_cmd_valid);
    CHECK(dut->m0_cmd_ready == 0, "m0_cmd_ready must be 0 while s_cmd_ready=0, got %d",
          dut->m0_cmd_ready);
    // No handshake -> rsp_owner must not move: complete a prior-owner check.
    tick();
    dut->s_rsp_valid = 1;
    dut->s_rsp_rdata = 0x77777777u;
    dut->eval();
    CHECK(dut->m0_rsp_valid == 1, "rsp_owner should still be m0 (reset default, no handshake), got %d",
          dut->m0_rsp_valid);
    dut->s_rsp_valid = 0;
    dut->s_cmd_ready = 1;
    dut->eval();
    CHECK(dut->m0_cmd_ready == 1, "m0_cmd_ready should assert once the slave is ready, got %d",
          dut->m0_cmd_ready);

    printf("\n=== e203_icb_arbt: %d failure(s) ===\n", fail_count);
    return fail_count ? 1 : 0;
}
