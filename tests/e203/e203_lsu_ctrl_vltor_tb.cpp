// Verilator testbench for E203 LsuCtrl — cross-check
#include "VLsuCtrl.h"
#include "verilated.h"
#include <cstdio>

static int errors = 0;
static int test_num = 0;

#define CHECK(cond, ...) do { \
    test_num++; \
    if (!(cond)) { errors++; printf("FAIL test %d: ", test_num); printf(__VA_ARGS__); printf("\n"); } \
    else { printf("PASS test %d\n", test_num); } \
} while(0)

int main(int argc, char** argv) {
    Verilated::commandArgs(argc, argv);
    auto m = new VLsuCtrl;

    // ── SW ────────────────────────────────────────────────────
    m->is_store = 1; m->is_load = 0;
    m->addr = 0x1000; m->wdata = 0xDEADBEEF; m->funct3 = 2;
    m->mem_rdata = 0;
    m->eval();
    CHECK(m->mem_wdata == 0xDEADBEEF, "SW: wdata=0x%X", m->mem_wdata);
    CHECK(m->mem_wstrb == 0xF, "SW: wstrb=0x%X", m->mem_wstrb);

    // ── SB offset 0-3 ────────────────────────────────────────
    m->funct3 = 0; m->wdata = 0xAB;
    m->addr = 0x2000; m->eval();
    CHECK(m->mem_wstrb == 0x1, "SB+0: wstrb");
    m->addr = 0x2001; m->eval();
    CHECK(m->mem_wstrb == 0x2, "SB+1: wstrb");
    m->addr = 0x2002; m->eval();
    CHECK(m->mem_wstrb == 0x4, "SB+2: wstrb");
    m->addr = 0x2003; m->eval();
    CHECK(m->mem_wstrb == 0x8, "SB+3: wstrb");

    // ── SH offset 0,2 ────────────────────────────────────────
    m->funct3 = 1; m->wdata = 0xCAFE;
    m->addr = 0x3000; m->eval();
    CHECK(m->mem_wstrb == 0x3, "SH+0: wstrb");
    m->addr = 0x3002; m->eval();
    CHECK(m->mem_wstrb == 0xC, "SH+2: wstrb");

    // ── LB signed ────────────────────────────────────────────
    m->is_store = 0; m->is_load = 1;
    m->mem_rdata = 0xAABBCC80; m->funct3 = 0;
    m->addr = 0x5000; m->eval();
    CHECK(m->load_result == 0xFFFFFF80, "LB+0: 0x%X", m->load_result);
    m->addr = 0x5001; m->eval();
    CHECK(m->load_result == 0xFFFFFFCC, "LB+1: 0x%X", m->load_result);
    m->addr = 0x5003; m->eval();
    CHECK(m->load_result == 0xFFFFFFAA, "LB+3: 0x%X", m->load_result);

    // ── LBU unsigned ─────────────────────────────────────────
    m->funct3 = 4; m->addr = 0x5000; m->eval();
    CHECK(m->load_result == 0x00000080, "LBU+0: 0x%X", m->load_result);

    // ── LH signed ────────────────────────────────────────────
    m->mem_rdata = 0x1234F00D; m->funct3 = 1;
    m->addr = 0x6000; m->eval();
    CHECK(m->load_result == 0xFFFFF00D, "LH+0: 0x%X", m->load_result);
    m->addr = 0x6002; m->eval();
    CHECK(m->load_result == 0x00001234, "LH+2: 0x%X", m->load_result);

    // ── LHU unsigned ─────────────────────────────────────────
    m->funct3 = 5; m->addr = 0x6000; m->eval();
    CHECK(m->load_result == 0x0000F00D, "LHU+0: 0x%X", m->load_result);

    // ── LW ───────────────────────────────────────────────────
    m->funct3 = 2; m->mem_rdata = 0x12345678; m->addr = 0x4000; m->eval();
    CHECK(m->load_result == 0x12345678, "LW: 0x%X", m->load_result);

    printf("\n=== LsuCtrl Verilator: %d tests, %d errors ===\n", test_num, errors);
    delete m;
    return errors ? 1 : 0;
}
