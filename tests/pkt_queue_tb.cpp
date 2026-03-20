// PacketQueue testbench — drives the wrapper module (not the linklist directly).
// Verifies that push/pop data round-trips correctly through the linklist
// sub-instance and that status ports (empty/full/length) track correctly.

#include "VPacketQueue.h"
#include <cstdio>
#include <cassert>
#include <cstring>

static int pass_count = 0;
static int fail_count = 0;

#define CHECK(cond, msg, ...) \
  do { \
    if (cond) { ++pass_count; } \
    else { fprintf(stderr, "FAIL [%s:%d] " msg "\n", __FILE__, __LINE__, ##__VA_ARGS__); ++fail_count; } \
  } while(0)

static VPacketQueue dut;

static void tick() {
    dut.clk = 0; dut.eval();
    dut.clk = 1; dut.eval();
}

static void reset() {
    dut.rst = 1; dut.push_valid = 0; dut.pop_valid = 0;
    tick(); tick();
    dut.rst = 0;
    tick();
}

// Push one item; waits for req_ready then drives for 1 cycle.
// Returns after the resp_valid cycle.
static void push(uint32_t data) {
    // Wait until ready
    for (int i = 0; i < 16; i++) {
        if (dut.push_ready) break;
        tick();
    }
    dut.push_valid = 1;
    dut.push_data  = data;
    tick();                   // cycle 1 — req accepted
    dut.push_valid = 0;
    tick();                   // cycle 2 — resp_valid
    assert(dut.push_resp_valid && "push_resp_valid not set after 2 cycles");
}

// Pop one item; returns resp_data.
static uint32_t pop() {
    for (int i = 0; i < 16; i++) {
        if (dut.pop_ready) break;
        tick();
    }
    dut.pop_valid = 1;
    tick();                   // cycle 1 — req accepted
    dut.pop_valid = 0;
    tick();                   // cycle 2 — resp_valid
    assert(dut.pop_resp_valid && "pop_resp_valid not set after 2 cycles");
    return dut.pop_data;
}

int main() {
    memset(&dut, 0, sizeof(dut));

    // ── Reset ───────────────────────────────────────────────────────────────
    reset();
    CHECK(dut.empty  == 1, "empty after reset");
    CHECK(dut.full   == 0, "not full after reset");
    CHECK(dut.length == 0, "length==0 after reset");

    // ── Single push/pop ─────────────────────────────────────────────────────
    push(0xDEAD);
    CHECK(dut.empty  == 0, "not empty after push");
    CHECK(dut.length == 1, "length==1 after push");

    uint32_t v = pop();
    CHECK(v == 0xDEAD, "pop returned 0x%08X, expected 0xDEAD", v);
    CHECK(dut.empty  == 1, "empty after pop");
    CHECK(dut.length == 0, "length==0 after pop");

    // ── Fill queue (DEPTH=8) then check full ────────────────────────────────
    for (uint32_t i = 0; i < 8; i++) push(0x100 + i);
    CHECK(dut.full   == 1, "full after 8 pushes");
    CHECK(dut.length == 8, "length==8 after 8 pushes");

    // ── Drain and verify FIFO order ─────────────────────────────────────────
    for (uint32_t i = 0; i < 8; i++) {
        uint32_t got = pop();
        CHECK(got == 0x100 + i, "FIFO order: pop[%u]=0x%08X expected 0x%08X", i, got, 0x100 + i);
    }
    CHECK(dut.empty  == 1, "empty after drain");
    CHECK(dut.full   == 0, "not full after drain");

    // ── Interleaved push/pop ────────────────────────────────────────────────
    push(0xAABB);
    push(0xCCDD);
    v = pop();
    CHECK(v == 0xAABB, "interleaved pop[0]=0x%08X", v);
    push(0xEEFF);
    v = pop();
    CHECK(v == 0xCCDD, "interleaved pop[1]=0x%08X", v);
    v = pop();
    CHECK(v == 0xEEFF, "interleaved pop[2]=0x%08X", v);
    CHECK(dut.empty == 1, "empty after interleaved");

    // ── Summary ─────────────────────────────────────────────────────────────
    printf("PacketQueue: %d/%d tests passed\n", pass_count, pass_count + fail_count);
    return fail_count != 0 ? 1 : 0;
}
