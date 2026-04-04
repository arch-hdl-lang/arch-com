#include "VFsmCacheCtrl.h"
#include "verilated.h"
#include <cstdio>
#include <cstdlib>
#include <cstdint>
#include <cstring>

static VFsmCacheCtrl* dut;
static void fail(const char* msg) { printf("FAIL: %s\n", msg); exit(1); }

// ── SRAM models ──────────────────────────────────────────────────────────────
static uint64_t tag_sram[8][64];
static uint64_t data_sram[4096];
static uint64_t lru_sram[64];

// Previous-cycle read addresses (latency-1 model: data out 1 cycle after addr)
static uint8_t  prev_tag_rd_addr[8];
static bool     prev_tag_rd_en[8];
static uint8_t  prev_lru_rd_addr;
static bool     prev_lru_rd_en;
static uint16_t prev_data_rd_addr;
static bool     prev_data_rd_en;

// ── LRU reference model ───────────────────────────────────────────────────────
static uint8_t lru_victim(uint8_t tree) {
    uint8_t idx = 0;
    for (int d = 0; d < 3; d++) {
        int node = (1 << d) - 1 + idx;
        if (((tree >> node) & 1) == 0) idx = (idx << 1) | 1;
        else                           idx =  idx << 1;
    }
    return idx & 7;
}
static uint8_t lru_update(uint8_t tree, uint8_t way) {
    uint8_t r = tree, step = 0;
    for (int d = 0; d < 3; d++) {
        int bit = (way >> (2 - d)) & 1;
        int node = (1 << d) - 1 + step;
        if (bit) r |=  (1 << node);
        else     r &= ~(1 << node);
        step = (step << 1) | bit;
    }
    return r & 0x7F;
}

// ── Fill FSM model ────────────────────────────────────────────────────────────
static bool     fill_pending   = false;
static int      fill_countdown = 0;
static uint64_t fill_words[8]  = {};
#define FILL_LATENCY 5

static void fill_tick_pre() {
    if (dut->fill_start && !fill_pending) {
        fill_pending   = true;
        fill_countdown = FILL_LATENCY;
        uint64_t base  = (uint64_t)dut->fill_addr & ~63ULL;
        for (int i = 0; i < 8; i++)
            fill_words[i] = 0xF111000000000000ULL | (base + (uint64_t)i * 8);
    }
    dut->fill_done = 0;
    dut->fill_word_0 = fill_words[0]; dut->fill_word_1 = fill_words[1];
    dut->fill_word_2 = fill_words[2]; dut->fill_word_3 = fill_words[3];
    dut->fill_word_4 = fill_words[4]; dut->fill_word_5 = fill_words[5];
    dut->fill_word_6 = fill_words[6]; dut->fill_word_7 = fill_words[7];
    if (fill_pending) {
        if (--fill_countdown == 0) {
            fill_pending   = false;
            dut->fill_done = 1;
        }
    }
}

// ── WB FSM model ──────────────────────────────────────────────────────────────
static bool wb_pending   = false;
static int  wb_countdown = 0;
#define WB_LATENCY 3

static void wb_tick_pre() {
    if (dut->wb_start && !wb_pending) {
        wb_pending   = true;
        wb_countdown = WB_LATENCY;
    }
    dut->wb_done = 0;
    if (wb_pending) {
        if (--wb_countdown == 0) {
            wb_pending = false;
            dut->wb_done = 1;
        }
    }
}

// ── Drive SRAM read data + LRU module ────────────────────────────────────────
// Called after clock to present data for addresses issued this cycle.
static void drive_rd_and_lru() {
    dut->tag_rd_data_0 = prev_tag_rd_en[0] ? tag_sram[0][prev_tag_rd_addr[0]] : 0;
    dut->tag_rd_data_1 = prev_tag_rd_en[1] ? tag_sram[1][prev_tag_rd_addr[1]] : 0;
    dut->tag_rd_data_2 = prev_tag_rd_en[2] ? tag_sram[2][prev_tag_rd_addr[2]] : 0;
    dut->tag_rd_data_3 = prev_tag_rd_en[3] ? tag_sram[3][prev_tag_rd_addr[3]] : 0;
    dut->tag_rd_data_4 = prev_tag_rd_en[4] ? tag_sram[4][prev_tag_rd_addr[4]] : 0;
    dut->tag_rd_data_5 = prev_tag_rd_en[5] ? tag_sram[5][prev_tag_rd_addr[5]] : 0;
    dut->tag_rd_data_6 = prev_tag_rd_en[6] ? tag_sram[6][prev_tag_rd_addr[6]] : 0;
    dut->tag_rd_data_7 = prev_tag_rd_en[7] ? tag_sram[7][prev_tag_rd_addr[7]] : 0;
    dut->lru_rd_data   = prev_lru_rd_en  ? (uint8_t)(lru_sram[prev_lru_rd_addr] & 0x7F) : 0;
    dut->data_rd_data  = prev_data_rd_en ? data_sram[prev_data_rd_addr & 0xFFF] : 0;
    dut->eval();
    uint8_t tree = dut->lru_tree_in & 0x7F;
    dut->lru_victim_way = lru_victim(tree);
    if (dut->lru_access_en) dut->lru_tree_out = lru_update(tree, dut->lru_access_way & 7);
    else                    dut->lru_tree_out  = tree;
    dut->eval();
}

// ── Main tick ─────────────────────────────────────────────────────────────────
// Sequence per tick:
//   1. drive_rd_and_lru()            — present SRAM data for addresses from last cycle
//   2. fill/wb pre-tick              — update fill/wb FSM countdown, assert done if ready
//   3. capture write & rd signals    — snapshot posedge-triggered signals before clock
//   4. clock pulse                   — FSM registers update
//   5. apply SRAM writes             — write captured data to SRAM arrays
//   6. drive_rd_and_lru() again      — present SRAM data for addresses just issued this cycle
//                                      (makes comb outputs like resp_data correct immediately)
static void full_tick(int n = 1) {
    for (int i = 0; i < n; i++) {
        // Step 1
        drive_rd_and_lru();

        // Step 2
        fill_tick_pre();
        wb_tick_pre();
        dut->eval();

        // Step 3: Snapshot before rising edge
        bool     tw_en[8];   uint8_t tw_addr[8];   uint64_t tw_data[8];
        tw_en[0]=dut->tag_wr_en_0; tw_addr[0]=dut->tag_wr_addr_0&0x3F; tw_data[0]=dut->tag_wr_data_0;
        tw_en[1]=dut->tag_wr_en_1; tw_addr[1]=dut->tag_wr_addr_1&0x3F; tw_data[1]=dut->tag_wr_data_1;
        tw_en[2]=dut->tag_wr_en_2; tw_addr[2]=dut->tag_wr_addr_2&0x3F; tw_data[2]=dut->tag_wr_data_2;
        tw_en[3]=dut->tag_wr_en_3; tw_addr[3]=dut->tag_wr_addr_3&0x3F; tw_data[3]=dut->tag_wr_data_3;
        tw_en[4]=dut->tag_wr_en_4; tw_addr[4]=dut->tag_wr_addr_4&0x3F; tw_data[4]=dut->tag_wr_data_4;
        tw_en[5]=dut->tag_wr_en_5; tw_addr[5]=dut->tag_wr_addr_5&0x3F; tw_data[5]=dut->tag_wr_data_5;
        tw_en[6]=dut->tag_wr_en_6; tw_addr[6]=dut->tag_wr_addr_6&0x3F; tw_data[6]=dut->tag_wr_data_6;
        tw_en[7]=dut->tag_wr_en_7; tw_addr[7]=dut->tag_wr_addr_7&0x3F; tw_data[7]=dut->tag_wr_data_7;
        bool     dw_en   = dut->data_wr_en;
        uint16_t dw_addr = dut->data_wr_addr & 0xFFF;
        uint64_t dw_data = dut->data_wr_data;
        bool     lw_en   = dut->lru_wr_en;
        uint8_t  lw_addr = dut->lru_wr_addr & 0x3F;
        uint8_t  lw_data = dut->lru_wr_data & 0x7F;
        prev_tag_rd_en[0]=dut->tag_rd_en_0; prev_tag_rd_addr[0]=dut->tag_rd_addr_0;
        prev_tag_rd_en[1]=dut->tag_rd_en_1; prev_tag_rd_addr[1]=dut->tag_rd_addr_1;
        prev_tag_rd_en[2]=dut->tag_rd_en_2; prev_tag_rd_addr[2]=dut->tag_rd_addr_2;
        prev_tag_rd_en[3]=dut->tag_rd_en_3; prev_tag_rd_addr[3]=dut->tag_rd_addr_3;
        prev_tag_rd_en[4]=dut->tag_rd_en_4; prev_tag_rd_addr[4]=dut->tag_rd_addr_4;
        prev_tag_rd_en[5]=dut->tag_rd_en_5; prev_tag_rd_addr[5]=dut->tag_rd_addr_5;
        prev_tag_rd_en[6]=dut->tag_rd_en_6; prev_tag_rd_addr[6]=dut->tag_rd_addr_6;
        prev_tag_rd_en[7]=dut->tag_rd_en_7; prev_tag_rd_addr[7]=dut->tag_rd_addr_7;
        prev_lru_rd_en  = dut->lru_rd_en;   prev_lru_rd_addr  = dut->lru_rd_addr;
        prev_data_rd_en = dut->data_rd_en;  prev_data_rd_addr = dut->data_rd_addr;

        // Step 4: Clock
        dut->clk = 0; dut->eval();
        dut->clk = 1; dut->eval();

        // Step 5: Apply writes
        for (int w = 0; w < 8; w++)
            if (tw_en[w]) tag_sram[w][tw_addr[w]] = tw_data[w];
        if (dw_en) data_sram[dw_addr] = dw_data;
        if (lw_en) lru_sram[lw_addr]  = lw_data;

        // Step 6: Drive SRAM outputs for new FSM state (makes resp_data etc. valid on return)
        drive_rd_and_lru();
    }
}

// ── Request helpers ────────────────────────────────────────────────────────────
static uint64_t do_load(uint64_t vaddr, int timeout = 100) {
    dut->req_valid = 1; dut->req_vaddr = vaddr; dut->req_data = 0;
    dut->req_be = 0xFF; dut->req_is_store = 0;
    int t = timeout;
    while (!dut->req_ready && --t) full_tick(1);
    if (!t) { printf("FAIL: load req_ready timeout 0x%016llx\n", (unsigned long long)vaddr); exit(1); }
    full_tick(1); dut->req_valid = 0;
    t = timeout;
    while (!dut->resp_valid && --t) full_tick(1);
    if (!t) { printf("FAIL: load resp_valid timeout 0x%016llx\n", (unsigned long long)vaddr); exit(1); }
    uint64_t data = dut->resp_data;
    full_tick(1);
    return data;
}

static void do_store(uint64_t vaddr, uint64_t data, int timeout = 100) {
    dut->req_valid = 1; dut->req_vaddr = vaddr; dut->req_data = data;
    dut->req_be = 0xFF; dut->req_is_store = 1;
    int t = timeout;
    while (!dut->req_ready && --t) full_tick(1);
    if (!t) { printf("FAIL: store req_ready timeout 0x%016llx\n", (unsigned long long)vaddr); exit(1); }
    full_tick(1); dut->req_valid = 0;
    t = timeout;
    while (!dut->resp_valid && --t) full_tick(1);
    if (!t) { printf("FAIL: store resp_valid timeout 0x%016llx\n", (unsigned long long)vaddr); exit(1); }
    full_tick(1);
}

int main(int argc, char** argv) {
    VerilatedContext* ctx = new VerilatedContext;
    ctx->commandArgs(argc, argv);
    dut = new VFsmCacheCtrl(ctx);

    memset(tag_sram,  0, sizeof(tag_sram));
    memset(data_sram, 0, sizeof(data_sram));
    memset(lru_sram,  0, sizeof(lru_sram));
    memset(prev_tag_rd_en,   0, sizeof(prev_tag_rd_en));
    memset(prev_tag_rd_addr, 0, sizeof(prev_tag_rd_addr));
    prev_lru_rd_en = false; prev_lru_rd_addr = 0;
    prev_data_rd_en = false; prev_data_rd_addr = 0;

    dut->req_valid = 0; dut->req_vaddr = 0; dut->req_data = 0;
    dut->req_be = 0; dut->req_is_store = 0;
    dut->tag_rd_data_0=0; dut->tag_rd_data_1=0; dut->tag_rd_data_2=0; dut->tag_rd_data_3=0;
    dut->tag_rd_data_4=0; dut->tag_rd_data_5=0; dut->tag_rd_data_6=0; dut->tag_rd_data_7=0;
    dut->lru_rd_data=0; dut->data_rd_data=0;
    dut->lru_tree_out=0; dut->lru_victim_way=0;
    dut->fill_done=0; dut->fill_word_0=0; dut->fill_word_1=0; dut->fill_word_2=0;
    dut->fill_word_3=0; dut->fill_word_4=0; dut->fill_word_5=0; dut->fill_word_6=0;
    dut->fill_word_7=0; dut->wb_done=0;

    dut->rst = 1; dut->clk = 0; dut->eval();
    full_tick(3);
    dut->rst = 0; full_tick(1);

    // ── Test 1: Cold load miss ─────────────────────────────────────────────
    uint64_t addr1 = 0x80000000018ULL;  // set=0, word=3
    uint64_t data1 = do_load(addr1);
    uint64_t exp1  = 0xF111000000000000ULL | ((addr1 & ~63ULL) + 3*8);
    if (data1 != exp1) {
        printf("FAIL: test1 cold load: got=0x%016llx exp=0x%016llx\n",
               (unsigned long long)data1, (unsigned long long)exp1);
        exit(1);
    }
    printf("Test 1 PASS: cold load miss\n");

    // ── Test 2: Load same address → cache hit ─────────────────────────────
    uint64_t data2 = do_load(addr1);
    if (data2 != exp1) {
        printf("FAIL: test2 load hit: got=0x%016llx exp=0x%016llx\n",
               (unsigned long long)data2, (unsigned long long)exp1);
        exit(1);
    }
    printf("Test 2 PASS: load hit\n");

    // ── Test 3: Store to loaded line → hit ────────────────────────────────
    uint64_t sval = 0xDEADBEEFCAFEBABEULL;
    do_store(addr1, sval);
    printf("Test 3 PASS: store hit\n");

    // ── Test 4: Reload → see stored value ─────────────────────────────────
    uint64_t data4 = do_load(addr1);
    if (data4 != sval) {
        printf("FAIL: test4 readback: got=0x%016llx exp=0x%016llx\n",
               (unsigned long long)data4, (unsigned long long)sval);
        exit(1);
    }
    printf("Test 4 PASS: load after store\n");

    // ── Test 5: 8 cold misses to different sets ───────────────────────────
    for (int s = 1; s <= 8; s++) {
        uint64_t a = 0x80000000000ULL | ((uint64_t)s << 6);
        do_load(a);
    }
    printf("Test 5 PASS: 8 cold misses\n");

    // ── Test 6: Store miss → fill + write + readback ──────────────────────
    uint64_t sa6 = 0x90000000100ULL;   // set=4, word=2
    uint64_t sd6 = 0x1234567890ABCDEFULL;
    do_store(sa6, sd6);
    uint64_t rb6 = do_load(sa6);
    if (rb6 != sd6) {
        printf("FAIL: test6 store-miss readback: got=0x%016llx exp=0x%016llx\n",
               (unsigned long long)rb6, (unsigned long long)sd6);
        exit(1);
    }
    printf("Test 6 PASS: store miss + readback\n");

    printf("PASS\n");
    delete dut; delete ctx;
    return 0;
}
