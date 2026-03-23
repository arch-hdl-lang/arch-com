// Testbench for E203 BIU — arch sim
#include "VBiu.h"
#include <cstdio>

static int errors = 0;
static int test_num = 0;

#define BOOL(x) ((x) & 1)
#define CHECK(cond, ...) do { \
    test_num++; \
    if (!(cond)) { errors++; printf("FAIL test %d: ", test_num); printf(__VA_ARGS__); printf("\n"); } \
    else { printf("PASS test %d\n", test_num); } \
} while(0)

int main() {
    VBiu m;
    m.lsu_addr = 0; m.lsu_wdata = 0; m.lsu_wstrb = 0;
    m.lsu_wen = 0; m.lsu_ren = 0;
    m.itcm_rd_data = 0; m.dtcm_rd_data = 0;
    m.eval();

    // ── Test 1: ITCM read ───────────────────────────────────────────
    m.lsu_addr = 0x80000100;  // ITCM offset 0x100, word addr = 0x40
    m.lsu_ren = 1; m.lsu_wen = 0;
    m.itcm_rd_data = 0xDEADBEEF;
    m.eval();
    CHECK(BOOL(m.itcm_rd_en), "ITCM read: rd_en=%d", m.itcm_rd_en);
    CHECK(m.itcm_rd_addr == 0x40, "ITCM read: addr=0x%X (exp 0x40)", m.itcm_rd_addr);
    CHECK(!BOOL(m.dtcm_rd_en), "ITCM read: dtcm_rd_en=%d (exp 0)", m.dtcm_rd_en);
    CHECK(m.lsu_rdata == 0xDEADBEEF, "ITCM read: rdata=0x%X", m.lsu_rdata);

    // ── Test 5: DTCM read ───────────────────────────────────────────
    m.lsu_addr = 0x90000200;  // DTCM offset 0x200, word addr = 0x80
    m.dtcm_rd_data = 0xCAFEBABE;
    m.eval();
    CHECK(BOOL(m.dtcm_rd_en), "DTCM read: rd_en=%d", m.dtcm_rd_en);
    CHECK(m.dtcm_rd_addr == 0x80, "DTCM read: addr=0x%X (exp 0x80)", m.dtcm_rd_addr);
    CHECK(!BOOL(m.itcm_rd_en), "DTCM read: itcm_rd_en=%d (exp 0)", m.itcm_rd_en);
    CHECK(m.lsu_rdata == 0xCAFEBABEu, "DTCM read: rdata=0x%X", m.lsu_rdata);

    // ── Test 9: DTCM write ──────────────────────────────────────────
    m.lsu_addr = 0x90000010;
    m.lsu_ren = 0; m.lsu_wen = 1;
    m.lsu_wdata = 0x12345678;
    m.lsu_wstrb = 0xF;
    m.eval();
    CHECK(BOOL(m.dtcm_wr_en), "DTCM write: wr_en=%d", m.dtcm_wr_en);
    CHECK(m.dtcm_wr_addr == 0x4, "DTCM write: addr=0x%X (exp 0x4)", m.dtcm_wr_addr);
    CHECK(m.dtcm_wr_data == 0x12345678u, "DTCM write: wdata=0x%X", m.dtcm_wr_data);
    CHECK(m.dtcm_wr_be == 0xF, "DTCM write: wr_be=0x%X", m.dtcm_wr_be);
    CHECK(!BOOL(m.itcm_wr_en), "DTCM write: itcm_wr_en=%d (exp 0)", m.itcm_wr_en);

    // ── Test 14: ITCM write ─────────────────────────────────────────
    m.lsu_addr = 0x80000008;
    m.lsu_wdata = 0xAABBCCDD;
    m.eval();
    CHECK(BOOL(m.itcm_wr_en), "ITCM write: wr_en=%d", m.itcm_wr_en);
    CHECK(m.itcm_wr_addr == 0x2, "ITCM write: addr=0x%X (exp 0x2)", m.itcm_wr_addr);
    CHECK(m.itcm_wr_data == 0xAABBCCDDu, "ITCM write: wdata=0x%X", m.itcm_wr_data);

    // ── Test 17: External address (neither ITCM nor DTCM) ───────────
    m.lsu_addr = 0x20000000;
    m.lsu_ren = 1; m.lsu_wen = 0;
    m.eval();
    CHECK(!BOOL(m.itcm_rd_en), "ext: itcm_rd_en=%d", m.itcm_rd_en);
    CHECK(!BOOL(m.dtcm_rd_en), "ext: dtcm_rd_en=%d", m.dtcm_rd_en);
    CHECK(m.lsu_rdata == 0, "ext: rdata=0x%X (exp 0)", m.lsu_rdata);

    // ── Test 20: Byte-strobe write to DTCM ──────────────────────────
    m.lsu_addr = 0x90000004;
    m.lsu_ren = 0; m.lsu_wen = 1;
    m.lsu_wstrb = 0x2;  // byte 1 only
    m.lsu_wdata = 0x0000FF00;
    m.eval();
    CHECK(m.dtcm_wr_be == 0x2, "byte strobe: wr_be=0x%X (exp 0x2)", m.dtcm_wr_be);

    // ── Test 21: Idle (no ren, no wen) ──────────────────────────────
    m.lsu_ren = 0; m.lsu_wen = 0;
    m.lsu_addr = 0x80000000;
    m.eval();
    CHECK(!BOOL(m.itcm_rd_en), "idle: itcm_rd_en=%d", m.itcm_rd_en);
    CHECK(!BOOL(m.itcm_wr_en), "idle: itcm_wr_en=%d", m.itcm_wr_en);

    printf("\n=== BIU test: %d tests, %d errors ===\n", test_num, errors);
    return errors ? 1 : 0;
}
