// Testbench for E203 LsuCtrl — arch sim
#include "VLsuCtrl.h"
#include <cstdio>
#include <cstdlib>

static int errors = 0;
static int test_num = 0;

#define CHECK(cond, ...) do { \
    test_num++; \
    if (!(cond)) { errors++; printf("FAIL test %d: ", test_num); printf(__VA_ARGS__); printf("\n"); } \
    else { printf("PASS test %d\n", test_num); } \
} while(0)

int main() {
    VLsuCtrl m;
    m.is_load = 0; m.is_store = 0; m.addr = 0; m.wdata = 0;
    m.funct3 = 0; m.mem_rdata = 0;
    m.eval();

    // ════════════════════════════════════════════════════════════
    // STORE TESTS
    // ════════════════════════════════════════════════════════════

    // ── SW (word store) at aligned address ────────────────────
    m.is_store = 1; m.is_load = 0;
    m.addr = 0x1000; m.wdata = 0xDEADBEEF; m.funct3 = 2; // SW
    m.eval();
    CHECK(m.mem_addr == 0x1000, "SW: mem_addr=0x%X", m.mem_addr);
    CHECK(m.mem_wdata == 0xDEADBEEF, "SW: mem_wdata=0x%X", m.mem_wdata);
    CHECK(m.mem_wstrb == 0xF, "SW: mem_wstrb=0x%X", m.mem_wstrb);
    CHECK(m.mem_wen == 1, "SW: mem_wen");

    // ── SB (byte store) at offset 0 ──────────────────────────
    m.addr = 0x2000; m.wdata = 0x000000AB; m.funct3 = 0; // SB
    m.eval();
    CHECK(m.mem_addr == 0x2000, "SB+0: mem_addr=0x%X", m.mem_addr);
    CHECK(m.mem_wstrb == 0x1, "SB+0: wstrb=0x%X", m.mem_wstrb);
    CHECK((m.mem_wdata & 0xFF) == 0xAB, "SB+0: wdata low byte=0x%X", m.mem_wdata & 0xFF);

    // ── SB at offset 1 ───────────────────────────────────────
    m.addr = 0x2001; m.wdata = 0x000000CD; m.funct3 = 0;
    m.eval();
    CHECK(m.mem_addr == 0x2000, "SB+1: addr aligned=0x%X", m.mem_addr);
    CHECK(m.mem_wstrb == 0x2, "SB+1: wstrb=0x%X", m.mem_wstrb);
    CHECK(((m.mem_wdata >> 8) & 0xFF) == 0xCD, "SB+1: byte1=0x%X", (m.mem_wdata >> 8) & 0xFF);

    // ── SB at offset 2 ───────────────────────────────────────
    m.addr = 0x2002; m.wdata = 0x000000EF; m.funct3 = 0;
    m.eval();
    CHECK(m.mem_wstrb == 0x4, "SB+2: wstrb=0x%X", m.mem_wstrb);
    CHECK(((m.mem_wdata >> 16) & 0xFF) == 0xEF, "SB+2: byte2=0x%X", (m.mem_wdata >> 16) & 0xFF);

    // ── SB at offset 3 ───────────────────────────────────────
    m.addr = 0x2003; m.wdata = 0x00000012; m.funct3 = 0;
    m.eval();
    CHECK(m.mem_wstrb == 0x8, "SB+3: wstrb=0x%X", m.mem_wstrb);
    CHECK(((m.mem_wdata >> 24) & 0xFF) == 0x12, "SB+3: byte3=0x%X", (m.mem_wdata >> 24) & 0xFF);

    // ── SH (halfword store) at offset 0 ──────────────────────
    m.addr = 0x3000; m.wdata = 0x0000CAFE; m.funct3 = 1; // SH
    m.eval();
    CHECK(m.mem_wstrb == 0x3, "SH+0: wstrb=0x%X", m.mem_wstrb);
    CHECK((m.mem_wdata & 0xFFFF) == 0xCAFE, "SH+0: half=0x%X", m.mem_wdata & 0xFFFF);

    // ── SH at offset 2 ───────────────────────────────────────
    m.addr = 0x3002; m.wdata = 0x0000FACE; m.funct3 = 1;
    m.eval();
    CHECK(m.mem_wstrb == 0xC, "SH+2: wstrb=0x%X", m.mem_wstrb);
    CHECK(((m.mem_wdata >> 16) & 0xFFFF) == 0xFACE, "SH+2: half=0x%X", (m.mem_wdata >> 16) & 0xFFFF);

    // ════════════════════════════════════════════════════════════
    // LOAD TESTS
    // ════════════════════════════════════════════════════════════
    m.is_store = 0; m.is_load = 1;

    // ── LW (word load) ───────────────────────────────────────
    m.addr = 0x4000; m.mem_rdata = 0x12345678; m.funct3 = 2; // LW
    m.eval();
    CHECK(m.load_result == 0x12345678, "LW: result=0x%X", m.load_result);
    CHECK(m.mem_wstrb == 0, "LW: wstrb=0 (read)");
    CHECK(m.mem_wen == 0, "LW: wen=0");

    // ── LB (signed byte) from offset 0 ──────────────────────
    m.addr = 0x5000; m.mem_rdata = 0xAABBCC80; m.funct3 = 0; // LB
    m.eval();
    CHECK(m.load_result == 0xFFFFFF80, "LB+0: sign-ext 0x80 → 0x%X", m.load_result);

    // ── LB from offset 1 ─────────────────────────────────────
    m.addr = 0x5001; m.mem_rdata = 0xAABBCC80; m.funct3 = 0;
    m.eval();
    CHECK(m.load_result == 0xFFFFFFCC, "LB+1: sign-ext 0xCC → 0x%X", m.load_result);

    // ── LB from offset 2 ─────────────────────────────────────
    m.addr = 0x5002; m.mem_rdata = 0xAABBCC80; m.funct3 = 0;
    m.eval();
    CHECK(m.load_result == 0xFFFFFFBB, "LB+2: sign-ext 0xBB → 0x%X", m.load_result);

    // ── LB from offset 3 ─────────────────────────────────────
    m.addr = 0x5003; m.mem_rdata = 0xAABBCC80; m.funct3 = 0;
    m.eval();
    CHECK(m.load_result == 0xFFFFFFAA, "LB+3: sign-ext 0xAA → 0x%X", m.load_result);

    // ── LBU (unsigned byte) from offset 0 ────────────────────
    m.addr = 0x5000; m.mem_rdata = 0xAABBCC80; m.funct3 = 4; // LBU
    m.eval();
    CHECK(m.load_result == 0x00000080, "LBU+0: zero-ext 0x80 → 0x%X", m.load_result);

    // ── LBU from offset 3 ────────────────────────────────────
    m.addr = 0x5003; m.mem_rdata = 0xAABBCC80; m.funct3 = 4;
    m.eval();
    CHECK(m.load_result == 0x000000AA, "LBU+3: zero-ext 0xAA → 0x%X", m.load_result);

    // ── LH (signed halfword) from offset 0 ───────────────────
    m.addr = 0x6000; m.mem_rdata = 0x1234F00D; m.funct3 = 1; // LH
    m.eval();
    CHECK(m.load_result == 0xFFFFF00D, "LH+0: sign-ext 0xF00D → 0x%X", m.load_result);

    // ── LH from offset 2 ─────────────────────────────────────
    m.addr = 0x6002; m.mem_rdata = 0x1234F00D; m.funct3 = 1;
    m.eval();
    CHECK(m.load_result == 0x00001234, "LH+2: sign-ext 0x1234 → 0x%X", m.load_result);

    // ── LHU (unsigned halfword) from offset 0 ────────────────
    m.addr = 0x6000; m.mem_rdata = 0x1234F00D; m.funct3 = 5; // LHU
    m.eval();
    CHECK(m.load_result == 0x0000F00D, "LHU+0: zero-ext 0xF00D → 0x%X", m.load_result);

    // ── LH positive value (no sign extension) ────────────────
    m.addr = 0x6000; m.mem_rdata = 0xFFFF7FFF; m.funct3 = 1; // LH
    m.eval();
    CHECK(m.load_result == 0x00007FFF, "LH pos: 0x7FFF → 0x%X", m.load_result);

    // ── LB positive value ────────────────────────────────────
    m.addr = 0x5000; m.mem_rdata = 0xFFFFFF7F; m.funct3 = 0; // LB
    m.eval();
    CHECK(m.load_result == 0x0000007F, "LB pos: 0x7F → 0x%X", m.load_result);

    // ── Idle (no load, no store) ─────────────────────────────
    m.is_store = 0; m.is_load = 0;
    m.eval();
    CHECK(m.mem_wstrb == 0, "idle: wstrb=0");
    CHECK(m.mem_wen == 0, "idle: wen=0");

    printf("\n=== LsuCtrl: %d tests, %d errors ===\n", test_num, errors);
    return errors ? 1 : 0;
}
