// Verilator testbench for AxiDmaTop — exercises MM2S and S2MM with VCD waveform dump.
#include "VAxiDmaTop.h"
#include "verilated.h"
#include "verilated_vcd_c.h"
#include <cstdio>
#include <cstdlib>
#include <cstring>

static VAxiDmaTop* dut;
static VerilatedVcdC* tfp;
static vluint64_t sim_time = 0;

static uint32_t axi_mem[4096];

static void tick() {
    dut->clk = 0;
    dut->eval();
    tfp->dump(sim_time++);
    dut->clk = 1;
    dut->eval();
    tfp->dump(sim_time++);
}

static void reset() {
    memset(axi_mem, 0, sizeof(axi_mem));
    dut->rst = 1;
    dut->s_axil_aw_addr = 0; dut->s_axil_aw_valid = 0;
    dut->s_axil_w_data = 0; dut->s_axil_w_strb = 0xF; dut->s_axil_w_valid = 0;
    dut->s_axil_b_ready = 1;
    dut->s_axil_ar_addr = 0; dut->s_axil_ar_valid = 0;
    dut->s_axil_r_ready = 1;
    dut->m_axi_mm2s_ar_ready = 0;
    dut->m_axi_mm2s_r_valid = 0; dut->m_axi_mm2s_r_data = 0; dut->m_axi_mm2s_r_last = 0;
    dut->m_axi_mm2s_r_id = 0; dut->m_axi_mm2s_r_resp = 0;
    dut->m_axi_s2mm_aw_ready = 0;
    dut->m_axi_s2mm_w_ready = 0;
    dut->m_axi_s2mm_b_valid = 0; dut->m_axi_s2mm_b_id = 0; dut->m_axi_s2mm_b_resp = 0;
    dut->m_axis_mm2s_tready = 0;
    dut->s_axis_s2mm_tvalid = 0; dut->s_axis_s2mm_tdata = 0;
    dut->s_axis_s2mm_tlast = 0; dut->s_axis_s2mm_tkeep = 0;
    // SG ports
    dut->m_axi_mm2s_sg_ar_ready = 0;
    dut->m_axi_mm2s_sg_r_valid = 0; dut->m_axi_mm2s_sg_r_data = 0; dut->m_axi_mm2s_sg_r_last = 0;
    dut->m_axi_mm2s_sg_r_id = 0; dut->m_axi_mm2s_sg_r_resp = 0;
    dut->m_axi_mm2s_sg_aw_ready = 0;
    dut->m_axi_mm2s_sg_w_ready = 0;
    dut->m_axi_mm2s_sg_b_valid = 0; dut->m_axi_mm2s_sg_b_id = 0; dut->m_axi_mm2s_sg_b_resp = 0;
    dut->m_axi_s2mm_sg_ar_ready = 0;
    dut->m_axi_s2mm_sg_r_valid = 0; dut->m_axi_s2mm_sg_r_data = 0; dut->m_axi_s2mm_sg_r_last = 0;
    dut->m_axi_s2mm_sg_r_id = 0; dut->m_axi_s2mm_sg_r_resp = 0;
    dut->m_axi_s2mm_sg_aw_ready = 0;
    dut->m_axi_s2mm_sg_w_ready = 0;
    dut->m_axi_s2mm_sg_b_valid = 0; dut->m_axi_s2mm_sg_b_id = 0; dut->m_axi_s2mm_sg_b_resp = 0;
    tick(); tick(); tick();
    dut->rst = 0;
    tick();
}

#define ASSERT_EQ(a, b, msg) do { \
    if ((a) != (b)) { \
        printf("FAIL %s: got=0x%x exp=0x%x\n", msg, (unsigned)(a), (unsigned)(b)); \
        tfp->close(); exit(1); \
    } \
} while(0)

static void axil_write(uint32_t addr, uint32_t data) {
    dut->s_axil_aw_addr = addr; dut->s_axil_aw_valid = 1;
    dut->s_axil_w_data = data; dut->s_axil_w_strb = 0xF; dut->s_axil_w_valid = 1;
    for (int i = 0; i < 20; i++) { tick(); if (dut->s_axil_b_valid) break; }
    dut->s_axil_aw_valid = 0; dut->s_axil_w_valid = 0;
    tick();
}

// ── MM2S AXI4 read slave model ──────────────────────────────────────────────
static int mm2s_ar_pending = 0;
static uint32_t mm2s_ar_addr_l = 0;
static int mm2s_ar_len_l = 0, mm2s_r_beat = 0;

static void mm2s_model() {
    if (dut->m_axi_mm2s_ar_valid && !mm2s_ar_pending) {
        dut->m_axi_mm2s_ar_ready = 1;
        mm2s_ar_addr_l = dut->m_axi_mm2s_ar_addr;
        mm2s_ar_len_l = dut->m_axi_mm2s_ar_len;
        mm2s_ar_pending = 1; mm2s_r_beat = 0;
    } else {
        dut->m_axi_mm2s_ar_ready = 0;
    }
    if (mm2s_ar_pending && mm2s_r_beat <= mm2s_ar_len_l) {
        uint32_t wa = (mm2s_ar_addr_l >> 2) + mm2s_r_beat;
        dut->m_axi_mm2s_r_valid = 1;
        dut->m_axi_mm2s_r_data = axi_mem[wa & 0xFFF];
        dut->m_axi_mm2s_r_last = (mm2s_r_beat == mm2s_ar_len_l) ? 1 : 0;
        if (dut->m_axi_mm2s_r_ready) mm2s_r_beat++;
    } else {
        if (mm2s_ar_pending && mm2s_r_beat > mm2s_ar_len_l) mm2s_ar_pending = 0;
        dut->m_axi_mm2s_r_valid = 0; dut->m_axi_mm2s_r_last = 0;
    }
}

// ── S2MM AXI4 write slave model ─────────────────────────────────────────────
static int s2mm_aw_pending = 0, s2mm_w_beat = 0, s2mm_b_pending = 0;
static uint32_t s2mm_aw_addr_l = 0;

static void s2mm_model() {
    if (dut->m_axi_s2mm_aw_valid && !s2mm_aw_pending) {
        dut->m_axi_s2mm_aw_ready = 1;
        s2mm_aw_addr_l = dut->m_axi_s2mm_aw_addr;
        s2mm_aw_pending = 1; s2mm_w_beat = 0;
    } else {
        dut->m_axi_s2mm_aw_ready = 0;
    }
    if (s2mm_aw_pending) {
        dut->m_axi_s2mm_w_ready = 1;
        if (dut->m_axi_s2mm_w_valid) {
            uint32_t wa = (s2mm_aw_addr_l >> 2) + s2mm_w_beat;
            axi_mem[wa & 0xFFF] = dut->m_axi_s2mm_w_data;
            s2mm_w_beat++;
            if (dut->m_axi_s2mm_w_last) { s2mm_b_pending = 1; s2mm_aw_pending = 0; }
        }
    } else { dut->m_axi_s2mm_w_ready = 0; }
    if (s2mm_b_pending) {
        dut->m_axi_s2mm_b_valid = 1;
        if (dut->m_axi_s2mm_b_ready) s2mm_b_pending = 0;
    } else { dut->m_axi_s2mm_b_valid = 0; }
}

// ── AXIS models ─────────────────────────────────────────────────────────────
static uint32_t sink_buf[256]; static int sink_cnt = 0;
static uint32_t src_buf[256]; static int src_len = 0, src_idx = 0;

static void axis_sink() {
    dut->m_axis_mm2s_tready = 1;
    if (dut->m_axis_mm2s_tvalid) sink_buf[sink_cnt++] = dut->m_axis_mm2s_tdata;
}
static void axis_source() {
    if (src_idx < src_len) {
        dut->s_axis_s2mm_tvalid = 1;
        dut->s_axis_s2mm_tdata = src_buf[src_idx];
        dut->s_axis_s2mm_tlast = (src_idx == src_len - 1) ? 1 : 0;
        if (dut->s_axis_s2mm_tready) src_idx++;
    } else {
        dut->s_axis_s2mm_tvalid = 0; dut->s_axis_s2mm_tdata = 0; dut->s_axis_s2mm_tlast = 0;
    }
}
static void run_models() { mm2s_model(); s2mm_model(); axis_sink(); axis_source(); }
static void run(int n) { for (int i = 0; i < n; i++) { run_models(); tick(); } }

// ═════════════════════════════════════════════════════════════════════════════
int main(int argc, char** argv) {
    Verilated::commandArgs(argc, argv);
    Verilated::traceEverOn(true);
    dut = new VAxiDmaTop;
    tfp = new VerilatedVcdC;
    dut->trace(tfp, 99);
    tfp->open("axi_dma.vcd");

    // ── Test 1: MM2S ─────────────────────────────────────────────────────
    reset();
    mm2s_ar_pending = 0; s2mm_aw_pending = 0; s2mm_b_pending = 0;
    sink_cnt = 0;
    for (int i = 0; i < 4; i++) axi_mem[0x400 + i] = 0xCAFE0000 + i;
    axil_write(0x00, (1 << 12) | 1);
    axil_write(0x18, 0x1000);
    axil_write(0x28, 16);
    run(50);
    ASSERT_EQ(sink_cnt, 4, "MM2S beat count");
    for (int i = 0; i < 4; i++) ASSERT_EQ(sink_buf[i], (uint32_t)(0xCAFE0000+i), "MM2S data");
    ASSERT_EQ(dut->mm2s_introut, 1, "MM2S interrupt");
    printf("Test 1 PASS: MM2S\n");

    // ── Test 2: S2MM ─────────────────────────────────────────────────────
    reset();
    mm2s_ar_pending = 0; s2mm_aw_pending = 0; s2mm_b_pending = 0;
    src_len = 4; src_idx = 0;
    for (int i = 0; i < 4; i++) src_buf[i] = 0xBEEF0000 + i;
    axil_write(0x30, (1 << 12) | 1);
    axil_write(0x48, 0x2000);
    axil_write(0x58, 16);
    run(100);
    for (int i = 0; i < 4; i++) ASSERT_EQ(axi_mem[0x800+i], (uint32_t)(0xBEEF0000+i), "S2MM mem");
    ASSERT_EQ(dut->s2mm_introut, 1, "S2MM interrupt");
    printf("Test 2 PASS: S2MM\n");

    printf("PASS\n");
    tfp->close();
    delete dut;
    return 0;
}
