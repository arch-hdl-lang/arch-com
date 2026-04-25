// Phase C smoke test for the cam construct.
//
// Drives the Mshr_Addr_Cam CAM, writes a few entries, searches, and
// checks the match outputs cycle by cycle.

#include "VMshr_Addr_Cam.h"
#include <cstdio>
#include <cstdlib>

static VMshr_Addr_Cam dut;
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
    dut.write_valid = 0;
    dut.search_key = 0;
    tick(); tick();
    dut.rst = 0; tick();
}

static void cam_write(uint32_t idx, uint32_t key, bool set) {
    dut.write_valid = 1;
    dut.write_idx = idx;
    dut.write_key = key;
    dut.write_set = set ? 1 : 0;
    tick();
    dut.write_valid = 0;
}

int main() {
    printf("=== cam_basic sim ===\n");
    do_reset();

    // Initially empty: any search returns no match.
    dut.search_key = 0x123; dut.eval();
    CHECK(dut.search_any == 0, "post-reset search_any == 0");
    CHECK(dut.search_mask == 0, "post-reset search_mask == 0");

    // Insert key 0x55 at idx 3, key 0x77 at idx 5.
    cam_write(3, 0x55, true);
    cam_write(5, 0x77, true);
    cam_write(7, 0x55, true);  // duplicate key at different index

    // Search for 0x77 → only idx 5 matches.
    dut.search_key = 0x77; dut.eval();
    CHECK(dut.search_any == 1, "search 0x77 hit");
    CHECK(dut.search_mask == (1u << 5), "search 0x77 mask = bit 5 (got 0x%x)", (unsigned)dut.search_mask);
    CHECK(dut.search_first == 5, "search 0x77 first = 5 (got %u)", (unsigned)dut.search_first);

    // Search for 0x55 → idx 3 and idx 7 match; first should be 3 (LSB).
    dut.search_key = 0x55; dut.eval();
    CHECK(dut.search_any == 1, "search 0x55 hit");
    CHECK(dut.search_mask == ((1u << 3) | (1u << 7)), "search 0x55 mask (got 0x%x)", (unsigned)dut.search_mask);
    CHECK(dut.search_first == 3, "search 0x55 first = 3 (got %u)", (unsigned)dut.search_first);

    // Search for 0xAB → no match.
    dut.search_key = 0xAB; dut.eval();
    CHECK(dut.search_any == 0, "search 0xAB no hit");
    CHECK(dut.search_mask == 0, "search 0xAB mask 0");

    // Clear idx 3; search 0x55 → only idx 7 left.
    cam_write(3, 0, false);
    dut.search_key = 0x55; dut.eval();
    CHECK(dut.search_mask == (1u << 7), "post-clear mask bit 7 (got 0x%x)", (unsigned)dut.search_mask);
    CHECK(dut.search_first == 7, "post-clear first = 7");

    printf("=== %d pass / %d fail ===\n", pass, fail);
    return fail == 0 ? 0 : 1;
}
