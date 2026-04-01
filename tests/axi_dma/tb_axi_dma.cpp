#include "VAxiDmaTop.h"
#include <cstdio>
#include <cstdlib>
#include <cstring>

static VAxiDmaTop dut;
static int cycle_count = 0;

// ── AXI4 memory model ────────────────────────────────────────────────────────
static uint32_t axi_mem[4096]; // 16KB, word-addressed

// ── AXIS sink (captures MM2S output) ─────────────────────────────────────────
static uint32_t axis_sink_buf[256];
static int axis_sink_count = 0;

// ── AXIS source (drives S2MM input) ──────────────────────────────────────────
static uint32_t axis_src_buf[256];
static int axis_src_len = 0;
static int axis_src_idx = 0;

static void tick() {
    dut.clk = 0; dut.eval();
    dut.clk = 1; dut.eval();
    cycle_count++;
}

static void reset() {
    memset(axi_mem, 0, sizeof(axi_mem));
    axis_sink_count = 0;
    axis_src_len = 0;
    axis_src_idx = 0;

    dut.rst = 1;
    dut.s_awaddr = 0; dut.s_awvalid = 0;
    dut.s_wdata = 0; dut.s_wstrb = 0xF; dut.s_wvalid = 0;
    dut.s_bready = 1;
    dut.s_araddr = 0; dut.s_arvalid = 0;
    dut.s_rready = 1;
    dut.mm2s_ar_ready = 0;
    dut.mm2s_r_valid = 0; dut.mm2s_r_data = 0; dut.mm2s_r_last = 0;
    dut.s2mm_aw_ready = 0;
    dut.s2mm_w_ready = 0;
    dut.s2mm_b_valid = 0;
    dut.m_axis_tready = 0;
    dut.s_axis_tvalid = 0; dut.s_axis_tdata = 0; dut.s_axis_tlast = 0;

    tick(); tick(); tick();
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

// ── AXI4-Lite helpers ────────────────────────────────────────────────────────

static void axil_write(uint32_t addr, uint32_t data) {
    dut.s_awaddr = addr;
    dut.s_awvalid = 1;
    dut.s_wdata = data;
    dut.s_wstrb = 0xF;
    dut.s_wvalid = 1;
    dut.s_bready = 1;
    for (int i = 0; i < 20; i++) {
        tick();
        if (dut.s_bvalid) break;
    }
    dut.s_awvalid = 0;
    dut.s_wvalid = 0;
    tick();
}

static uint32_t axil_read(uint32_t addr) {
    dut.s_araddr = addr;
    dut.s_arvalid = 1;
    dut.s_rready = 1;
    for (int i = 0; i < 20; i++) {
        tick();
        if (dut.s_rvalid) break;
    }
    dut.s_arvalid = 0;
    uint32_t val = dut.s_rdata;
    tick();
    return val;
}

// ── AXI4 read slave model (for MM2S) ────────────────────────────────────────
// Responds to AR with R data from axi_mem, one beat per cycle.
static int mm2s_ar_pending = 0;
static uint32_t mm2s_ar_addr_latched = 0;
static int mm2s_ar_len_latched = 0;
static int mm2s_r_beat = 0;

static void mm2s_mem_model() {
    // Accept AR
    if (dut.mm2s_ar_valid && !mm2s_ar_pending) {
        dut.mm2s_ar_ready = 1;
        mm2s_ar_addr_latched = dut.mm2s_ar_addr;
        mm2s_ar_len_latched = dut.mm2s_ar_len;
        mm2s_ar_pending = 1;
        mm2s_r_beat = 0;
    } else {
        dut.mm2s_ar_ready = 0;
    }

    // Drive R data — present current beat, advance on handshake.
    // Don't clear r_valid on the acceptance cycle; let the next call clear it.
    if (mm2s_ar_pending && mm2s_r_beat <= mm2s_ar_len_latched) {
        uint32_t word_addr = (mm2s_ar_addr_latched >> 2) + mm2s_r_beat;
        dut.mm2s_r_valid = 1;
        dut.mm2s_r_data = axi_mem[word_addr & 0xFFF];
        dut.mm2s_r_last = (mm2s_r_beat == mm2s_ar_len_latched) ? 1 : 0;

        if (dut.mm2s_r_ready) {
            mm2s_r_beat++;
        }
    } else {
        if (mm2s_ar_pending && mm2s_r_beat > mm2s_ar_len_latched) {
            mm2s_ar_pending = 0;
        }
        dut.mm2s_r_valid = 0;
        dut.mm2s_r_last = 0;
    }
}

// ── AXI4 write slave model (for S2MM) ───────────────────────────────────────
static int s2mm_aw_pending = 0;
static uint32_t s2mm_aw_addr_latched = 0;
static int s2mm_w_beat = 0;
static int s2mm_b_pending = 0;

static void s2mm_mem_model() {
    // Accept AW
    if (dut.s2mm_aw_valid && !s2mm_aw_pending) {
        dut.s2mm_aw_ready = 1;
        s2mm_aw_addr_latched = dut.s2mm_aw_addr;
        s2mm_aw_pending = 1;
        s2mm_w_beat = 0;
    } else {
        dut.s2mm_aw_ready = 0;
    }

    // Accept W data — keep w_ready=1 through the last beat so FSM sees
    // the simultaneous w_ready & w_last for its transition condition.
    if (s2mm_aw_pending) {
        dut.s2mm_w_ready = 1;
        if (dut.s2mm_w_valid) {
            uint32_t word_addr = (s2mm_aw_addr_latched >> 2) + s2mm_w_beat;
            axi_mem[word_addr & 0xFFF] = dut.s2mm_w_data;
            s2mm_w_beat++;
            if (dut.s2mm_w_last) {
                // Don't clear w_ready yet — FSM needs to see it this cycle.
                // Set b_pending; aw_pending will be cleared next cycle.
                s2mm_b_pending = 1;
                s2mm_aw_pending = 0;
            }
        }
    } else {
        dut.s2mm_w_ready = 0;
    }

    // B response — hold b_valid until b_ready handshake completes.
    // Don't clear b_valid on the acceptance cycle (FSM needs to see it).
    if (s2mm_b_pending) {
        dut.s2mm_b_valid = 1;
        if (dut.s2mm_b_ready) {
            s2mm_b_pending = 0;
            // Keep b_valid=1 through this tick so FSM sees it
        }
    } else {
        dut.s2mm_b_valid = 0;
    }
}

// ── AXIS sink (captures MM2S stream output) ─────────────────────────────────
static void axis_sink_model() {
    dut.m_axis_tready = 1;
    if (dut.m_axis_tvalid && dut.m_axis_tready) {
        axis_sink_buf[axis_sink_count++] = dut.m_axis_tdata;
    }
}

// ── AXIS source (drives S2MM stream input) ──────────────────────────────────
static void axis_source_model() {
    if (axis_src_idx < axis_src_len) {
        dut.s_axis_tvalid = 1;
        dut.s_axis_tdata = axis_src_buf[axis_src_idx];
        dut.s_axis_tlast = (axis_src_idx == axis_src_len - 1) ? 1 : 0;
        if (dut.s_axis_tready) {
            axis_src_idx++;
        }
    } else {
        dut.s_axis_tvalid = 0;
        dut.s_axis_tdata = 0;
        dut.s_axis_tlast = 0;
    }
}

static void run_models() {
    mm2s_mem_model();
    s2mm_mem_model();
    axis_sink_model();
    axis_source_model();
}

static void run_cycles(int n) {
    for (int i = 0; i < n; i++) {
        run_models();
        tick();
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Test 1: MM2S transfer — read 4 words from memory, verify stream output
// ═══════════════════════════════════════════════════════════════════════════════
static void test_mm2s() {
    reset();
    mm2s_ar_pending = 0; s2mm_aw_pending = 0; s2mm_b_pending = 0;

    // Pre-load memory at 0x1000 (word addr 0x400)
    for (int i = 0; i < 4; i++)
        axi_mem[0x400 + i] = 0xCAFE0000 + i;

    // Configure MM2S: RS=1, IOC_IrqEn=1
    axil_write(0x00, (1 << 12) | 1);
    // Set source address
    axil_write(0x18, 0x1000);
    // Set length = 16 bytes (4 beats) — triggers transfer
    axil_write(0x28, 16);

    // Run until transfer completes
    run_cycles(50);

    // Verify stream output
    ASSERT_EQ(axis_sink_count, 4, "MM2S stream beat count");
    for (int i = 0; i < 4; i++) {
        char msg[64]; snprintf(msg, sizeof(msg), "MM2S stream data[%d]", i);
        ASSERT_EQ(axis_sink_buf[i], (uint32_t)(0xCAFE0000 + i), msg);
    }

    // Verify interrupt
    ASSERT_EQ(dut.mm2s_introut, 1, "MM2S interrupt asserted");

    // Verify DMASR.IOC_Irq
    uint32_t sr = axil_read(0x04);
    ASSERT_EQ((sr >> 12) & 1, 1, "DMASR.IOC_Irq set");

    // Clear interrupt via W1C
    axil_write(0x04, 1 << 12);
    dut.eval();
    tick();
    dut.eval();
    ASSERT_EQ(dut.mm2s_introut, 0, "MM2S interrupt cleared");

    printf("Test 1 PASS: MM2S 4-beat transfer\n");
}

// ═══════════════════════════════════════════════════════════════════════════════
// Test 2: S2MM transfer — stream 4 words in, verify memory contents
// ═══════════════════════════════════════════════════════════════════════════════
static void test_s2mm() {
    reset();
    mm2s_ar_pending = 0; s2mm_aw_pending = 0; s2mm_b_pending = 0;

    // Prepare source data
    axis_src_len = 4;
    axis_src_idx = 0;
    for (int i = 0; i < 4; i++)
        axis_src_buf[i] = 0xBEEF0000 + i;

    // Configure S2MM: RS=1, IOC_IrqEn=1
    axil_write(0x30, (1 << 12) | 1);
    // Set dest address
    axil_write(0x48, 0x2000);
    // Set length = 16 bytes — arms channel
    axil_write(0x58, 16);

    // Run until transfer completes
    run_cycles(100);

    // Verify memory contents at 0x2000 (word addr 0x800)
    for (int i = 0; i < 4; i++) {
        char msg[64]; snprintf(msg, sizeof(msg), "S2MM mem[%d]", i);
        ASSERT_EQ(axi_mem[0x800 + i], (uint32_t)(0xBEEF0000 + i), msg);
    }

    // Give interrupt a couple cycles to propagate
    run_cycles(5);
    dut.eval();

    // Verify interrupt
    ASSERT_EQ(dut.s2mm_introut, 1, "S2MM interrupt asserted");

    printf("Test 2 PASS: S2MM 4-beat transfer\n");
}

// ═══════════════════════════════════════════════════════════════════════════════
// Test 3: Register readback
// ═══════════════════════════════════════════════════════════════════════════════
static void test_register_readback() {
    reset();

    axil_write(0x00, 0x1001);
    ASSERT_EQ(axil_read(0x00), 0x1001u, "MM2S_DMACR readback");

    axil_write(0x18, 0x12345678);
    ASSERT_EQ(axil_read(0x18), 0x12345678u, "MM2S_SA readback");

    axil_write(0x30, 0x0001);
    ASSERT_EQ(axil_read(0x30), 0x0001u, "S2MM_DMACR readback");

    axil_write(0x48, 0xABCD0000);
    ASSERT_EQ(axil_read(0x48), 0xABCD0000u, "S2MM_DA readback");

    printf("Test 3 PASS: register readback\n");
}

// ═══════════════════════════════════════════════════════════════════════════════
// Test 4: Bidirectional — MM2S and S2MM simultaneously
// ═══════════════════════════════════════════════════════════════════════════════
static void test_bidirectional() {
    reset();
    mm2s_ar_pending = 0; s2mm_aw_pending = 0; s2mm_b_pending = 0;
    axis_sink_count = 0;

    // Pre-load memory for MM2S
    for (int i = 0; i < 4; i++)
        axi_mem[0x400 + i] = 0xAA000000 + i;

    // Prepare S2MM source data
    axis_src_len = 4;
    axis_src_idx = 0;
    for (int i = 0; i < 4; i++)
        axis_src_buf[i] = 0xBB000000 + i;

    // Configure both channels
    axil_write(0x00, (1 << 12) | 1); // MM2S DMACR
    axil_write(0x18, 0x1000);         // MM2S SA
    axil_write(0x30, (1 << 12) | 1); // S2MM DMACR
    axil_write(0x48, 0x3000);         // S2MM DA

    // Start both: write LENGTH registers
    axil_write(0x28, 16); // MM2S LENGTH
    axil_write(0x58, 16); // S2MM LENGTH

    // Run until both complete
    run_cycles(100);

    // Verify MM2S output
    ASSERT_EQ(axis_sink_count, 4, "bidir MM2S beat count");
    for (int i = 0; i < 4; i++) {
        char msg[64]; snprintf(msg, sizeof(msg), "bidir MM2S data[%d]", i);
        ASSERT_EQ(axis_sink_buf[i], (uint32_t)(0xAA000000 + i), msg);
    }

    // Verify S2MM memory (0x3000 = word addr 0xC00)
    for (int i = 0; i < 4; i++) {
        char msg[64]; snprintf(msg, sizeof(msg), "bidir S2MM mem[%d]", i);
        ASSERT_EQ(axi_mem[0xC00 + i], (uint32_t)(0xBB000000 + i), msg);
    }

    // Both interrupts should fire
    ASSERT_EQ(dut.mm2s_introut, 1, "bidir MM2S interrupt");
    ASSERT_EQ(dut.s2mm_introut, 1, "bidir S2MM interrupt");

    printf("Test 4 PASS: bidirectional transfer\n");
}

int main() {
    test_mm2s();
    test_s2mm();
    test_register_readback();
    test_bidirectional();
    printf("PASS\n");
    return 0;
}
