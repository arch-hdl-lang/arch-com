#include "VModuleLruUpdate.h"
#include "verilated.h"
#include <cstdio>
#include <cstdlib>
#include <cstdint>

static VModuleLruUpdate* dut;

// ── Reference model (matches pseudo_lru_tree_policy.arch algorithm) ─────────
// Victim: traverse tree following bit=0 → go right (high ways), bit=1 → go left
static uint8_t victim_select(uint8_t tree) {
    uint8_t idx = 0;
    for (int depth = 0; depth < 3; depth++) {
        int node = (1 << depth) - 1 + idx;
        if (((tree >> node) & 1) == 0)
            idx = (idx << 1) | 1;
        else
            idx = idx << 1;
    }
    return idx & 0x7;
}

// Update: mark access_way as MRU by setting path bits to point away from it
static uint8_t tree_update(uint8_t tree, uint8_t way) {
    uint8_t result = tree;
    uint32_t step = 0;
    for (int depth = 0; depth < 3; depth++) {
        int bit = (way >> (2 - depth)) & 1;
        int node = (1 << depth) - 1 + step;
        if (bit) result |= (1 << node);
        else     result &= ~(1 << node);
        step = (step << 1) | bit;
    }
    return result & 0x7F;
}

static void fail(const char* msg, int a = -1, int b = -1) {
    if (a >= 0) printf("FAIL: %s (got=%d expected=%d)\n", msg, a, b);
    else        printf("FAIL: %s\n", msg);
    exit(1);
}

static void eval(uint8_t tree, uint8_t way, bool en) {
    dut->tree_in    = tree & 0x7F;
    dut->access_way = way  & 0x7;
    dut->access_en  = en ? 1 : 0;
    dut->eval();
}

int main(int argc, char** argv) {
    VerilatedContext* ctx = new VerilatedContext;
    ctx->commandArgs(argc, argv);
    dut = new VModuleLruUpdate(ctx);

    // ── Test 1: access_en=false → tree_out must equal tree_in ─────────────
    for (int t = 0; t < 128; t++) {
        eval(t, 0, false);
        if ((int)dut->tree_out != t)
            fail("access_en=0: tree_out != tree_in", dut->tree_out, t);
    }

    // ── Test 2: victim_way matches reference model for all trees ──────────
    for (int t = 0; t < 128; t++) {
        eval(t, 0, false);
        uint8_t expected = victim_select(t);
        if ((int)dut->victim_way != (int)expected)
            fail("victim_way mismatch", dut->victim_way, expected);
    }

    // ── Test 3: tree_out matches reference model for all (tree, way) ──────
    int mismatches = 0;
    for (int t = 0; t < 128; t++) {
        for (int w = 0; w < 8; w++) {
            eval(t, w, true);
            uint8_t exp_victim = victim_select(t);
            uint8_t exp_tree   = tree_update(t, w);
            if ((int)dut->victim_way != (int)exp_victim) {
                printf("FAIL: tree=0x%02x way=%d: victim got=%d exp=%d\n",
                       t, w, dut->victim_way, exp_victim);
                mismatches++;
            }
            if ((int)dut->tree_out != (int)exp_tree) {
                printf("FAIL: tree=0x%02x way=%d: tree_out got=0x%02x exp=0x%02x\n",
                       t, w, (uint8_t)dut->tree_out, exp_tree);
                mismatches++;
            }
            if (mismatches >= 10) { printf("Too many failures, stopping\n"); exit(1); }
        }
    }
    if (mismatches > 0) exit(1);

    // ── Test 4: Spot-check known values ───────────────────────────────────
    // All-zeros tree → victim = way 7
    eval(0, 0, true);
    if (dut->victim_way != 7) fail("all-zeros victim != 7", dut->victim_way, 7);

    // All-ones tree → victim = way 0
    eval(0x7F, 0, true);
    if (dut->victim_way != 0) fail("all-ones victim != 0", dut->victim_way, 0);

    // Access way 7 from all-zeros: tree_out = 0b1000101 = 0x45
    eval(0, 7, true);
    if (dut->tree_out != 0x45) fail("way7 update from 0: tree_out", dut->tree_out, 0x45);

    // Sequential update: access ways 0..7 in order, track tree state matches reference
    {
        uint8_t tree = 0;
        for (int w = 0; w < 8; w++) {
            eval(tree, w, true);
            uint8_t exp_v = victim_select(tree);
            uint8_t exp_t = tree_update(tree, w);
            if ((uint8_t)dut->victim_way != exp_v)
                fail("sequential: victim_way mismatch", dut->victim_way, exp_v);
            if ((uint8_t)dut->tree_out != exp_t)
                fail("sequential: tree_out mismatch", dut->tree_out, exp_t);
            tree = exp_t;
        }
    }

    printf("PASS\n");
    delete dut; delete ctx;
    return 0;
}
