// Cam v2 dual-write smoke test.
//
// Exercises the four interesting port-2 scenarios:
//   1. only port 1 fires
//   2. only port 2 fires
//   3. both fire, different indices  → both commit
//   4. both fire, same index         → port 2 wins (last-write semantics)
// Plus: reset clears all valid bits.

#include "VMshr_Addr_Cam_Dual.h"
#include <cstdio>
#include <cstdlib>

static VMshr_Addr_Cam_Dual dut;
static int pass = 0, fail = 0;

#define CHECK(cond, msg, ...) do { \
  if (cond) { printf("  PASS: " msg "\n", ##__VA_ARGS__); ++pass; } \
  else      { printf("  FAIL: " msg "\n", ##__VA_ARGS__); ++fail; } \
} while (0)

static void tick() {
    dut.clk = 0; dut.eval();
    dut.clk = 1; dut.eval();
}

static void do_reset() {
    dut.rst = 1;
    dut.write_valid = dut.write2_valid = 0;
    tick(); tick();
    dut.rst = 0; tick();
}

static void clear_writes() {
    dut.write_valid = dut.write2_valid = 0;
}

int main() {
    printf("=== cam_dual_basic sim ===\n");
    do_reset();

    // ── 1. Only port 1: insert (idx 2, key 0x11) ──
    clear_writes();
    dut.write_valid = 1; dut.write_idx = 2; dut.write_key = 0x11; dut.write_set = 1;
    tick();
    clear_writes();
    dut.search_key = 0x11; dut.eval();
    CHECK(dut.search_any == 1, "port1-only insert at idx 2");
    CHECK(dut.search_first == 2, "  first = 2 (got %u)", (unsigned)dut.search_first);

    // ── 2. Only port 2: insert (idx 5, key 0x22) ──
    clear_writes();
    dut.write2_valid = 1; dut.write2_idx = 5; dut.write2_key = 0x22; dut.write2_set = 1;
    tick();
    clear_writes();
    dut.search_key = 0x22; dut.eval();
    CHECK(dut.search_any == 1, "port2-only insert at idx 5");
    CHECK(dut.search_first == 5, "  first = 5 (got %u)", (unsigned)dut.search_first);

    // ── 3. Both fire, different indices: insert (idx 7, 0x33) on p1, (idx 9, 0x44) on p2 ──
    clear_writes();
    dut.write_valid = 1;  dut.write_idx = 7;  dut.write_key = 0x33; dut.write_set = 1;
    dut.write2_valid = 1; dut.write2_idx = 9; dut.write2_key = 0x44; dut.write2_set = 1;
    tick();
    clear_writes();
    dut.search_key = 0x33; dut.eval();
    CHECK(dut.search_any == 1 && dut.search_first == 7, "concurrent w1=0x33@7 commits");
    dut.search_key = 0x44; dut.eval();
    CHECK(dut.search_any == 1 && dut.search_first == 9, "concurrent w2=0x44@9 commits");

    // ── 4. Both fire, SAME index: p1 sets key 0xAA at idx 12, p2 sets key 0xBB at idx 12.
    //      Port 2 must win — search 0xBB hits, search 0xAA misses ──
    clear_writes();
    dut.write_valid = 1;  dut.write_idx = 12;  dut.write_key = 0xAA; dut.write_set = 1;
    dut.write2_valid = 1; dut.write2_idx = 12; dut.write2_key = 0xBB; dut.write2_set = 1;
    tick();
    clear_writes();
    dut.search_key = 0xBB; dut.eval();
    CHECK(dut.search_any == 1 && dut.search_first == 12,
          "same-idx conflict: port 2 (0xBB) wins (got any=%u first=%u)",
          (unsigned)dut.search_any, (unsigned)dut.search_first);
    dut.search_key = 0xAA; dut.eval();
    CHECK(dut.search_any == 0,
          "same-idx conflict: port 1 (0xAA) loses (got any=%u)",
          (unsigned)dut.search_any);

    // ── 5. Same-idx, p1 sets, p2 clears: clear wins (port 2 last) ──
    // First plant a valid entry at idx 4 with key 0x55.
    clear_writes();
    dut.write_valid = 1; dut.write_idx = 4; dut.write_key = 0x55; dut.write_set = 1;
    tick();
    // Now p1 tries to re-set 0x66 at idx 4, p2 clears idx 4.
    clear_writes();
    dut.write_valid = 1;  dut.write_idx = 4;  dut.write_key = 0x66; dut.write_set = 1;
    dut.write2_valid = 1; dut.write2_idx = 4; dut.write2_key = 0;    dut.write2_set = 0;
    tick();
    clear_writes();
    dut.search_key = 0x66; dut.eval();
    CHECK(dut.search_any == 0, "same-idx set/clear: clear (port 2) wins");

    // ── 6. Reset clears everything ──
    do_reset();
    dut.search_key = 0x11; dut.eval();
    CHECK(dut.search_any == 0 && dut.search_mask == 0, "reset clears all");

    printf("=== %d pass / %d fail ===\n", pass, fail);
    return fail == 0 ? 0 : 1;
}
