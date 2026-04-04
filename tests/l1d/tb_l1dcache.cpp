#include "VL1DCache.h"
#include "verilated.h"
#include <cstdio>
#include <cstdlib>
#include <cstdint>
#include <cstring>
#include <map>

static VL1DCache* dut;
static void fail(const char* msg) { printf("FAIL: %s\n", msg); exit(1); }

// ── AXI4 behavioral memory model ─────────────────────────────────────────────
// Sparse word-addressed memory.  Uninitialized words return a distinct pattern.
static std::map<uint64_t, uint64_t> mem_model;

static uint64_t mem_read(uint64_t byte_addr) {
    uint64_t wa = byte_addr >> 3;
    auto it = mem_model.find(wa);
    if (it != mem_model.end()) return it->second;
    // Default: 0xFEED_xxxx pattern so uninitialized reads are obvious
    return 0xFEED000000000000ULL | (byte_addr & ~7ULL);
}

static void mem_write(uint64_t byte_addr, uint64_t data, uint8_t strb = 0xFF) {
    uint64_t wa  = byte_addr >> 3;
    uint64_t old = mem_read(byte_addr);
    uint64_t res = old;
    for (int b = 0; b < 8; b++)
        if ((strb >> b) & 1)
            res = (res & ~(0xFFULL << (b*8))) | (((data >> (b*8)) & 0xFF) << (b*8));
    mem_model[wa] = res;
}

static bool debug_axi = false;

// AXI4 channel state
static bool    r_active  = false;
static uint64_t r_base   = 0;
static int      r_beat   = 0;

static bool    w_active  = false;
static uint64_t aw_base  = 0;
static int      w_beat   = 0;
static bool    b_pending = false;

static void axi_drive() {
    // AR: accept immediately when not busy reading
    dut->ar_ready = r_active ? 0 : 1;

    // R: drive beat when active
    if (r_active) {
        uint64_t waddr = r_base + (uint64_t)r_beat * 8;
        dut->r_valid = 1;
        dut->r_data  = mem_read(waddr);
        dut->r_id    = 0;
        dut->r_resp  = 0;
        dut->r_last  = (r_beat == 7) ? 1 : 0;
    } else {
        dut->r_valid = 0; dut->r_data = 0;
        dut->r_id = 0; dut->r_resp = 0; dut->r_last = 0;
    }

    // AW: accept immediately when not busy writing
    dut->aw_ready = (w_active || b_pending) ? 0 : 1;

    // W: always ready to absorb beats when w_active
    dut->w_ready = w_active ? 1 : 0;

    // B: signal completion when pending
    if (b_pending) {
        dut->b_valid = 1; dut->b_id = 1; dut->b_resp = 0;
    } else {
        dut->b_valid = 0; dut->b_id = 0; dut->b_resp = 0;
    }
}

// ── tick ──────────────────────────────────────────────────────────────────────
// 1. Drive AXI inputs + eval (comb settled)
// 2. Capture handshake signals BEFORE clock (so we read correct W beat data)
// 3. Clock pulse
// 4. Update AXI model from pre-clock captures
// 5. Re-drive AXI + eval (comb settled for new state — resp_valid etc. visible)
static void tick(int n = 1) {
    for (int i = 0; i < n; i++) {
        // Step 1
        axi_drive();
        dut->eval();

        // Step 2: snapshot handshakes before rising edge
        bool ar_hs     = dut->ar_valid && dut->ar_ready;
        uint64_t ar_a  = (uint64_t)dut->ar_addr;
        bool r_hs      = r_active && dut->r_valid && dut->r_ready;
        bool aw_hs     = dut->aw_valid && dut->aw_ready;
        uint64_t aw_a  = (uint64_t)dut->aw_addr;
        bool w_hs      = w_active && dut->w_valid && dut->w_ready;
        uint64_t w_d   = (uint64_t)dut->w_data;
        uint8_t  w_s   = (uint8_t)dut->w_strb;
        bool     w_l   = dut->w_last;
        bool b_hs      = b_pending && dut->b_ready && dut->b_valid;

        // Step 3: clock
        dut->clk = 0; dut->eval();
        dut->clk = 1; dut->eval();

        // Step 4: update AXI model
        if (ar_hs && !r_active) {
            r_base = ar_a & ~63ULL; r_beat = 0; r_active = true;
        }
        if (r_hs) {
            if (r_beat == 7) r_active = false;
            else             r_beat++;
        }
        if (aw_hs) {
            aw_base = aw_a & ~63ULL; w_beat = 0; w_active = true;
        }
        if (w_hs) {
            if (debug_axi) printf("[WB] beat=%d addr=0x%llx data=0x%llx last=%d\n", w_beat, (unsigned long long)(aw_base + w_beat*8), (unsigned long long)w_d, (int)w_l);
            mem_write(aw_base + (uint64_t)w_beat * 8, w_d, w_s);
            if (w_l) { w_active = false; b_pending = true; }
            else       w_beat++;
        }
        if (r_hs) {
            if (debug_axi) printf("[Fill] beat=%d addr=0x%llx data=0x%llx\n", r_beat, (unsigned long long)(r_base + r_beat*8), (unsigned long long)dut->r_data);
        }
        if (b_hs) b_pending = false;

        // Step 5: settle comb for new state
        axi_drive();
        dut->eval();
    }
}

// ── Request helpers ────────────────────────────────────────────────────────────
static uint64_t do_load(uint64_t vaddr, int timeout = 500) {
    dut->req_valid = 1; dut->req_vaddr = vaddr;
    dut->req_data = 0; dut->req_be = 0xFF; dut->req_is_store = 0;
    int t = timeout;
    while (!dut->req_ready && --t) tick(1);
    if (!t) { printf("FAIL: load req_ready timeout 0x%016llx\n",(unsigned long long)vaddr); exit(1); }
    tick(1); dut->req_valid = 0;
    t = timeout;
    while (!dut->resp_valid && --t) tick(1);
    if (!t) { printf("FAIL: load resp_valid timeout 0x%016llx\n",(unsigned long long)vaddr); exit(1); }
    uint64_t data = dut->resp_data;
    tick(1);
    return data;
}

static void do_store(uint64_t vaddr, uint64_t data, int timeout = 500) {
    dut->req_valid = 1; dut->req_vaddr = vaddr;
    dut->req_data = data; dut->req_be = 0xFF; dut->req_is_store = 1;
    int t = timeout;
    while (!dut->req_ready && --t) tick(1);
    if (!t) { printf("FAIL: store req_ready timeout 0x%016llx\n",(unsigned long long)vaddr); exit(1); }
    tick(1); dut->req_valid = 0;
    t = timeout;
    while (!dut->resp_valid && --t) tick(1);
    if (!t) { printf("FAIL: store resp_valid timeout 0x%016llx\n",(unsigned long long)vaddr); exit(1); }
    tick(1);
}

// ── Expected fill data ─────────────────────────────────────────────────────────
// Fill FSM computes fill_addr & ~63 and requests from memory.
// mem_model returns 0xFEED_<byte_addr> by default.
// We override with explicit writes to mem_model for predictable fill data.
static uint64_t expected_fill_word(uint64_t line_base, int beat) {
    return mem_read(line_base + (uint64_t)beat * 8);
}

int main(int argc, char** argv) {
    VerilatedContext* ctx = new VerilatedContext;
    ctx->commandArgs(argc, argv);
    dut = new VL1DCache(ctx);

    // ── Reset ────────────────────────────────────────────────────────────────
    dut->rst = 1; dut->clk = 0;
    dut->req_valid = 0; dut->req_vaddr = 0; dut->req_data = 0;
    dut->req_be = 0; dut->req_is_store = 0;
    // AXI inputs idle
    dut->ar_ready = 0; dut->r_valid = 0; dut->r_data = 0;
    dut->r_id = 0; dut->r_resp = 0; dut->r_last = 0;
    dut->aw_ready = 0; dut->w_ready = 0; dut->w_data = 0; dut->w_strb = 0; dut->w_last = 0;
    dut->b_valid = 0; dut->b_id = 0; dut->b_resp = 0;
    dut->eval();
    tick(3);
    dut->rst = 0; tick(2);

    // ── Test 1: Cold load miss ─────────────────────────────────────────────
    uint64_t addr1 = 0x000000000018ULL;  // set=0, word=3
    // Pre-populate memory model for addr1's line
    uint64_t line1 = addr1 & ~63ULL;
    for (int b = 0; b < 8; b++)
        mem_write(line1 + b*8, 0xA000000000000000ULL | (line1 + b*8));

    uint64_t got1 = do_load(addr1);
    uint64_t exp1 = 0xA000000000000000ULL | (line1 + 3*8);
    if (got1 != exp1) {
        printf("FAIL test1: cold load: got=0x%016llx exp=0x%016llx\n",
               (unsigned long long)got1, (unsigned long long)exp1);
        exit(1);
    }
    printf("Test 1 PASS: cold load miss\n");

    // ── Test 2: Load same line → hit ──────────────────────────────────────
    uint64_t got2 = do_load(addr1);
    if (got2 != exp1) {
        printf("FAIL test2: load hit: got=0x%016llx exp=0x%016llx\n",
               (unsigned long long)got2, (unsigned long long)exp1);
        exit(1);
    }
    printf("Test 2 PASS: load hit\n");

    // ── Test 3: Store hit → dirty ─────────────────────────────────────────
    uint64_t sval = 0xDEADBEEFCAFEBABEULL;
    do_store(addr1, sval);
    printf("Test 3 PASS: store hit\n");

    // ── Test 4: Reload → see stored value ─────────────────────────────────
    uint64_t got4 = do_load(addr1);
    if (got4 != sval) {
        printf("FAIL test4: readback: got=0x%016llx exp=0x%016llx\n",
               (unsigned long long)got4, (unsigned long long)sval);
        exit(1);
    }
    printf("Test 4 PASS: load after store\n");

    // ── Test 5: Store miss → fill + store-merge + readback ────────────────
    uint64_t addr5 = 0x000000100040ULL;  // set=1 (addr5[11:6]=1), word=0
    uint64_t line5 = addr5 & ~63ULL;
    for (int b = 0; b < 8; b++)
        mem_write(line5 + b*8, 0xB000000000000000ULL | (line5 + b*8));

    uint64_t sval5 = 0x1234567890ABCDEFULL;
    do_store(addr5, sval5);
    uint64_t got5 = do_load(addr5);
    if (got5 != sval5) {
        printf("FAIL test5: store-miss readback: got=0x%016llx exp=0x%016llx\n",
               (unsigned long long)got5, (unsigned long long)sval5);
        exit(1);
    }
    printf("Test 5 PASS: store miss + readback\n");

    // ── Test 6: Dirty eviction ────────────────────────────────────────────
    // Fill all 8 ways of set=2 with dirty data, then trigger a 9th miss
    // which must evict one dirty way via WbCollect → WbWait → AXI write.
    //
    // Set=2 → addr[11:6]=2 → low 12 bits: 0x080..0x0BF
    // Use tags 0x100..0x107 (9 tags; first 8 fill the 8 ways)
    const uint64_t SET2 = 2;
    uint64_t base_tags[9];
    for (int t = 0; t < 9; t++) {
        // addr = tag<<12 | set<<6 | word_offset
        base_tags[t] = ((uint64_t)(0x100 + t) << 12) | (SET2 << 6);
    }

    // Populate memory for each of the 9 lines
    for (int t = 0; t < 9; t++) {
        uint64_t lb = base_tags[t] & ~63ULL;
        for (int b = 0; b < 8; b++)
            mem_write(lb + b*8, 0xC000000000000000ULL | ((uint64_t)t << 40) | (uint64_t)b);
    }

    // Cold load for 8 ways (fills them clean)
    for (int t = 0; t < 8; t++) do_load(base_tags[t]);

    // Store to each way (mark dirty, write distinct values)
    uint64_t stored_sv[8];
    for (int t = 0; t < 8; t++) {
        stored_sv[t] = 0xD000000000000000ULL | ((uint64_t)t << 32) | (uint64_t)t;
        do_store(base_tags[t], stored_sv[t]);
    }

    // Load 9th address → must evict one dirty way
    // (LRU picks the oldest — way 7 was filled last but MRU after store; way 0 is LRU)
    // The evicted line gets written to AXI memory via WbCollect + WbWait.
    debug_axi = true;
    uint64_t got6 = do_load(base_tags[8]);
    // Verify it got the fill data (from the 9th line in memory)
    uint64_t exp6 = 0xC000000000000000ULL | ((uint64_t)8 << 40) | 0ULL;  // word=0
    if (got6 != exp6) {
        printf("FAIL test6: dirty evict fill: got=0x%016llx exp=0x%016llx\n",
               (unsigned long long)got6, (unsigned long long)exp6);
        exit(1);
    }
    printf("Test 6 PASS: dirty evict + refill\n");
    printf("AXI mem word0 of base_tags[0] after WB: 0x%016llx\n", (unsigned long long)mem_read(base_tags[0] & ~63ULL));
    printf("AXI mem word1 of base_tags[0] after WB: 0x%016llx\n", (unsigned long long)mem_read((base_tags[0] & ~63ULL) + 8));

    // ── Test 7: After eviction, reload the evicted address ───────────────
    // LRU analysis: with 8 sequential cold loads (ways 7,3,5,0,6,2,4,1)
    // and 8 stores marking all dirty, the 9th load evicts way 7 (tag=0x100,
    // the LRU victim).  The writeback puts stored_sv[0] into AXI memory at
    // word 0 of that line.  Reloading base_tags[0] misses and fills from
    // AXI → resp_data should equal stored_sv[0].
    uint64_t rb7 = do_load(base_tags[0]);
    if (rb7 != stored_sv[0]) {
        printf("FAIL test7: evicted reload: got=0x%016llx exp=0x%016llx\n",
               (unsigned long long)rb7, (unsigned long long)stored_sv[0]);
        exit(1);
    }
    printf("Test 7 PASS: evicted line reloaded correctly\n");

    printf("PASS\n");
    delete dut; delete ctx;
    return 0;
}
