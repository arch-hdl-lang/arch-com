// MAC learning table testbench. Exercises:
//   1. Cold lookup → miss
//   2. Learn (mac_a, port_2); lookup mac_a → hit on port 2
//   3. Multiple learns; each MAC routes to its own port
//   4. Lookup of an unknown MAC → miss (broadcast path)
//   5. Re-learn the same MAC at a new slot+port → routing updates
//      (oldest entry still hits per LSB-priority of cam.search_first)

#include "Vmac_table.h"
#include <cstdio>
#include <cstdint>

static Vmac_table dut;
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
    dut.learn_valid = 0;
    dut.lookup_mac = 0;
    tick(); tick();
    dut.rst = 0; tick();
}

static void learn(uint64_t mac, uint32_t port, uint32_t idx) {
    dut.learn_valid = 1;
    dut.learn_mac = mac;
    dut.learn_port = port;
    dut.learn_idx = idx;
    tick();
    dut.learn_valid = 0;
}

int main() {
    printf("=== mac_table sim ===\n");
    do_reset();

    // ── 1. Cold lookup: any MAC misses ──
    dut.lookup_mac = 0xDEADBEEFCAFEULL; dut.eval();
    CHECK(dut.lookup_hit == 0, "cold lookup misses");

    // ── 2. Learn one entry, look it up ──
    learn(0xAABBCCDDEEFFULL, 2, 0);
    dut.lookup_mac = 0xAABBCCDDEEFFULL; dut.eval();
    CHECK(dut.lookup_hit == 1, "post-learn lookup hits");
    CHECK(dut.lookup_port == 2, "  routed to port 2 (got %u)", (unsigned)dut.lookup_port);

    // ── 3. Multiple distinct entries route to distinct ports ──
    learn(0x111111111111ULL, 1, 1);
    learn(0x222222222222ULL, 3, 2);
    learn(0x333333333333ULL, 0, 3);

    dut.lookup_mac = 0x111111111111ULL; dut.eval();
    CHECK(dut.lookup_hit == 1 && dut.lookup_port == 1, "mac1 → port 1");
    dut.lookup_mac = 0x222222222222ULL; dut.eval();
    CHECK(dut.lookup_hit == 1 && dut.lookup_port == 3, "mac2 → port 3");
    dut.lookup_mac = 0x333333333333ULL; dut.eval();
    CHECK(dut.lookup_hit == 1 && dut.lookup_port == 0, "mac3 → port 0");

    // ── 4. Unknown MAC misses ──
    dut.lookup_mac = 0x999999999999ULL; dut.eval();
    CHECK(dut.lookup_hit == 0, "unknown MAC misses (broadcast path)");

    // ── 5. Re-learn the same MAC at a new slot — both entries match,
    //      cam.search_first returns the LSB-priority slot, so the
    //      OLDER entry still wins. Caller's port_table for that slot
    //      hasn't changed → port stays the same. (To migrate, the
    //      caller would clear the old slot first; out of scope here.)
    learn(0xAABBCCDDEEFFULL, 3, 5);
    dut.lookup_mac = 0xAABBCCDDEEFFULL; dut.eval();
    CHECK(dut.lookup_hit == 1, "re-learn lookup still hits");
    CHECK(dut.lookup_port == 2,
          "LSB-priority of search_first → original slot 0 wins (port 2; got %u)",
          (unsigned)dut.lookup_port);

    printf("=== %d pass / %d fail ===\n", pass, fail);
    return fail == 0 ? 0 : 1;
}
