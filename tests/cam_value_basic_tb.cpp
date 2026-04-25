// Cam v3 value_type smoke test.
// Insert (key, value) tuples, search by key, read back the value.

#include "VTag_Value_Cam.h"
#include <cstdio>
#include <cstdint>

static VTag_Value_Cam dut;
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
    tick(); tick();
    dut.rst = 0; tick();
}

static void write(uint32_t idx, uint32_t key, uint32_t value, bool set) {
    dut.write_valid = 1;
    dut.write_idx = idx;
    dut.write_key = key;
    dut.write_value = value;
    dut.write_set = set ? 1 : 0;
    tick();
    dut.write_valid = 0;
}

int main() {
    printf("=== cam_value_basic sim ===\n");
    do_reset();

    // Empty CAM: no match.
    dut.search_key = 0x100; dut.eval();
    CHECK(dut.search_any == 0, "empty: no match");

    // Insert 3 entries with distinct (key, value).
    write(0, 0x100, 0xDEAD0000, true);
    write(1, 0x200, 0xDEAD0001, true);
    write(2, 0x300, 0xDEAD0002, true);

    // Lookup each: hit + correct value.
    dut.search_key = 0x100; dut.eval();
    CHECK(dut.search_any == 1 && dut.search_first == 0 && dut.read_value == 0xDEAD0000,
          "key=0x100 → idx 0, value DEAD0000 (got first=%u value=0x%08x)",
          (unsigned)dut.search_first, (unsigned)dut.read_value);

    dut.search_key = 0x200; dut.eval();
    CHECK(dut.search_any == 1 && dut.search_first == 1 && dut.read_value == 0xDEAD0001,
          "key=0x200 → idx 1, value DEAD0001");

    dut.search_key = 0x300; dut.eval();
    CHECK(dut.search_any == 1 && dut.search_first == 2 && dut.read_value == 0xDEAD0002,
          "key=0x300 → idx 2, value DEAD0002");

    // Unknown key: search_any=0; read_value is whatever happens to be at idx 0
    // (caller must qualify with search_any).
    dut.search_key = 0x999; dut.eval();
    CHECK(dut.search_any == 0, "unknown key: search_any == 0 (read_value undefined)");

    // Update value at existing slot — overwrite with set=true.
    write(1, 0x200, 0xCAFE2222, true);
    dut.search_key = 0x200; dut.eval();
    CHECK(dut.read_value == 0xCAFE2222, "update slot 1 value (got 0x%08x)", (unsigned)dut.read_value);

    // Clear slot 0 — same key now misses.
    write(0, 0, 0, false);
    dut.search_key = 0x100; dut.eval();
    CHECK(dut.search_any == 0, "post-clear: key 0x100 misses");

    // Reset wipes everything.
    do_reset();
    dut.search_key = 0x200; dut.eval();
    CHECK(dut.search_any == 0, "post-reset: empty");

    printf("=== %d pass / %d fail ===\n", pass, fail);
    return fail == 0 ? 0 : 1;
}
