// TDD testbench for FsmSgEngine — scatter-gather descriptor processing.
// Descriptor format (4 words, 16 bytes):
//   Word 0: NXTDESC  — pointer to next descriptor (0 = end of chain)
//   Word 1: BUF_ADDR — buffer address for data transfer
//   Word 2: CONTROL  — length[25:0] in bytes
//   Word 3: STATUS   — written by DMA: transferred[25:0], Cmplt[31]
#include "VFsmSgEngine.h"
#include <cstdio>
#include <cstdlib>
#include <cstring>

static VFsmSgEngine dut;
static int cycle_count = 0;

// Descriptor memory (shared with AXI4 model)
static uint32_t desc_mem[1024]; // 4KB word-addressed

static void tick() {
    dut.clk = 0; dut.eval();
    dut.clk = 1; dut.eval();
    cycle_count++;
}

static void reset() {
    memset(desc_mem, 0, sizeof(desc_mem));
    dut.rst = 1;
    dut.sg_start = 0;
    dut.curdesc = 0; dut.taildesc = 0;
    dut.xfer_done = 0;
    dut.sg_ar_ready = 0;
    dut.sg_r_valid = 0; dut.sg_r_data = 0; dut.sg_r_last = 0;
    dut.sg_aw_ready = 0;
    dut.sg_w_ready = 0;
    dut.sg_b_valid = 0;
    tick(); tick();
    dut.rst = 0;
    tick();
    cycle_count = 0;
}

#define ASSERT_EQ(a, b, msg) do { \
    if ((a) != (b)) { \
        printf("FAIL %s: got=0x%x exp=0x%x at cycle %d\n", msg, (unsigned)(a), (unsigned)(b), cycle_count); \
        exit(1); \
    } \
} while(0)

// ── AXI4 read model for descriptor fetch ────────────────────────────────────
static int ar_pending = 0;
static uint32_t ar_addr_l = 0;
static int ar_len_l = 0, r_beat = 0;

static void axi_read_model() {
    if (dut.sg_ar_valid && !ar_pending) {
        dut.sg_ar_ready = 1;
        ar_addr_l = dut.sg_ar_addr;
        ar_len_l = dut.sg_ar_len;
        ar_pending = 1; r_beat = 0;
    } else {
        dut.sg_ar_ready = 0;
    }
    if (ar_pending && r_beat <= ar_len_l) {
        uint32_t wa = (ar_addr_l >> 2) + r_beat;
        dut.sg_r_valid = 1;
        dut.sg_r_data = desc_mem[wa & 0x3FF];
        dut.sg_r_last = (r_beat == ar_len_l) ? 1 : 0;
        if (dut.sg_r_ready) r_beat++;
    } else {
        if (ar_pending && r_beat > ar_len_l) ar_pending = 0;
        dut.sg_r_valid = 0; dut.sg_r_last = 0;
    }
}

// ── AXI4 write model for status writeback ───────────────────────────────────
static int aw_pending = 0, w_beat = 0, b_pending = 0;
static uint32_t aw_addr_l = 0;

static void axi_write_model() {
    if (dut.sg_aw_valid && !aw_pending) {
        dut.sg_aw_ready = 1;
        aw_addr_l = dut.sg_aw_addr;
        aw_pending = 1; w_beat = 0;
    } else {
        dut.sg_aw_ready = 0;
    }
    if (aw_pending) {
        dut.sg_w_ready = 1;
        if (dut.sg_w_valid) {
            uint32_t wa = (aw_addr_l >> 2) + w_beat;
            desc_mem[wa & 0x3FF] = dut.sg_w_data;
            w_beat++;
            if (dut.sg_w_last) { b_pending = 1; aw_pending = 0; }
        }
    } else { dut.sg_w_ready = 0; }
    if (b_pending) {
        dut.sg_b_valid = 1;
        if (dut.sg_b_ready) b_pending = 0;
    } else { dut.sg_b_valid = 0; }
}

static void run_models() { axi_read_model(); axi_write_model(); }
static void run(int n) { for (int i = 0; i < n; i++) { run_models(); tick(); } }

// Helper: write a descriptor at byte address
static void write_desc(uint32_t byte_addr, uint32_t next_ptr, uint32_t buf_addr, uint32_t length) {
    uint32_t wa = byte_addr >> 2;
    desc_mem[wa + 0] = next_ptr;
    desc_mem[wa + 1] = buf_addr;
    desc_mem[wa + 2] = length;      // CONTROL: length[25:0]
    desc_mem[wa + 3] = 0;           // STATUS: cleared, DMA writes Cmplt
}

// ═══════════════════════════════════════════════════════════════════════════════
// Test 1: Single descriptor — fetch, trigger transfer, write status
// ═══════════════════════════════════════════════════════════════════════════════
static void test_single_descriptor() {
    reset();
    ar_pending = 0; aw_pending = 0; b_pending = 0;

    // Place one descriptor at address 0x100
    // next_ptr = 0x100 (points to itself = tail), buf_addr = 0x2000, length = 16 bytes
    write_desc(0x100, 0x100, 0x2000, 16);

    // Start SG: curdesc = 0x100, taildesc = 0x100 (single descriptor)
    dut.curdesc = 0x100;
    dut.taildesc = 0x100;
    dut.sg_start = 1;
    tick();
    dut.sg_start = 0;

    // Run until transfer is triggered
    for (int i = 0; i < 30; i++) {
        run_models();
        tick();
        if (dut.xfer_start) break;
    }

    // Verify transfer parameters
    dut.eval();
    ASSERT_EQ(dut.xfer_start, 1, "xfer_start asserted");
    ASSERT_EQ(dut.xfer_addr, 0x2000u, "xfer_addr = buf_addr");
    ASSERT_EQ(dut.xfer_num_beats, 4u, "xfer_num_beats = 16/4 = 4");

    // Simulate transfer completion
    tick();
    dut.xfer_done = 1;
    tick();
    dut.xfer_done = 0;

    // Run until sg_done pulses (status writeback happens in between)
    int sg_done_seen = 0;
    for (int i = 0; i < 40; i++) {
        run_models();
        tick();
        dut.eval();
        if (dut.sg_done) { sg_done_seen = 1; break; }
    }

    // Check status word written back to descriptor (word 3 at 0x10C)
    uint32_t status = desc_mem[0x100/4 + 3];
    ASSERT_EQ(status >> 31, 1u, "Cmplt bit set in status");
    ASSERT_EQ(status & 0x3FFFFFF, 16u, "transferred length in status");
    ASSERT_EQ(sg_done_seen, 1, "sg_done asserted");

    printf("Test 1 PASS: single descriptor\n");
}

// ═══════════════════════════════════════════════════════════════════════════════
// Test 2: Three-descriptor chain
// ═══════════════════════════════════════════════════════════════════════════════
static void test_descriptor_chain() {
    reset();
    ar_pending = 0; aw_pending = 0; b_pending = 0;

    // Desc 0 at 0x100: next=0x110, buf=0x1000, len=8
    write_desc(0x100, 0x110, 0x1000, 8);
    // Desc 1 at 0x110: next=0x120, buf=0x2000, len=12
    write_desc(0x110, 0x120, 0x2000, 12);
    // Desc 2 at 0x120: next=0x120, buf=0x3000, len=16 (tail)
    write_desc(0x120, 0x120, 0x3000, 16);

    dut.curdesc = 0x100;
    dut.taildesc = 0x120;
    dut.sg_start = 1;
    tick();
    dut.sg_start = 0;

    int xfer_count = 0;
    uint32_t expected_addrs[3] = {0x1000, 0x2000, 0x3000};
    uint32_t expected_beats[3] = {2, 3, 4}; // 8/4, 12/4, 16/4

    for (int cycle = 0; cycle < 300 && xfer_count < 3; cycle++) {
        run_models();
        tick();

        if (dut.xfer_start) {
            dut.eval();
            char msg[64];
            snprintf(msg, sizeof(msg), "chain xfer[%d] addr", xfer_count);
            ASSERT_EQ(dut.xfer_addr, expected_addrs[xfer_count], msg);
            snprintf(msg, sizeof(msg), "chain xfer[%d] beats", xfer_count);
            ASSERT_EQ(dut.xfer_num_beats, expected_beats[xfer_count], msg);

            // Simulate transfer done after a few cycles
            for (int j = 0; j < 3; j++) { run_models(); tick(); }
            dut.xfer_done = 1;
            run_models(); tick();
            dut.xfer_done = 0;
            xfer_count++;
        }
    }

    ASSERT_EQ(xfer_count, 3, "3 descriptors processed");

    // Wait for sg_done
    int sg_done_seen = 0;
    for (int i = 0; i < 60; i++) {
        run_models();
        tick();
        dut.eval();
        if (dut.sg_done) { sg_done_seen = 1; break; }
    }

    // All 3 descriptors should have Cmplt set
    for (int i = 0; i < 3; i++) {
        uint32_t addr = 0x100 + i * 0x10;
        uint32_t status = desc_mem[addr/4 + 3];
        char msg[64]; snprintf(msg, sizeof(msg), "desc[%d] Cmplt", i);
        ASSERT_EQ(status >> 31, 1u, msg);
    }
    ASSERT_EQ(sg_done_seen, 1, "sg_done after chain");

    printf("Test 2 PASS: 3-descriptor chain\n");
}

int main() {
    test_single_descriptor();
    test_descriptor_chain();
    printf("PASS\n");
    return 0;
}
