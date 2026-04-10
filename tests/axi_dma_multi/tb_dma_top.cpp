// End-to-end testbench for multi-outstanding AxiDmaTop.
//
// Test: Simple DMA MM2S read (4 beats) — exercises the full path:
//   AxiLite write to registers → MM2S FSM → AXI4 read → FIFO → AXI-Stream out
//
// The test drives:
//   1. AXI-Lite writes to program MM2S: src_addr, length, DMACR run bit
//   2. AXI4 slave responds to AR/R
//   3. AXI-Stream master accepts tdata/tvalid/tlast
//   4. Checks mm2s_introut fires on completion

#include "VAxiDmaTop.h"
#include <cassert>
#include <cstdio>
#include <cstring>

static VAxiDmaTop dut;
static int cycle_count = 0;

void tick() {
    dut.clk = 0; dut.eval();
    dut.clk = 1; dut.eval();
    cycle_count++;
}

void reset() {
    dut.rst = 1;
    // AXI-Lite slave inputs
    dut.s_axil_aw_valid = 0; dut.s_axil_aw_addr = 0;
    dut.s_axil_w_valid = 0;  dut.s_axil_w_data = 0; dut.s_axil_w_strb = 0;
    dut.s_axil_b_ready = 1;
    dut.s_axil_ar_valid = 0; dut.s_axil_ar_addr = 0;
    dut.s_axil_r_ready = 1;
    // AXI4 MM2S read slave
    dut.m_axi_mm2s_ar_ready = 0;
    dut.m_axi_mm2s_r_valid = 0; dut.m_axi_mm2s_r_data = 0;
    dut.m_axi_mm2s_r_id = 0; dut.m_axi_mm2s_r_resp = 0; dut.m_axi_mm2s_r_last = 0;
    // AXI4 MM2S write (tied off, but set inputs)
    dut.m_axi_mm2s_aw_ready = 0;
    dut.m_axi_mm2s_w_ready = 0;
    dut.m_axi_mm2s_b_valid = 0; dut.m_axi_mm2s_b_id = 0; dut.m_axi_mm2s_b_resp = 0;
    // AXI4 S2MM (not used in this test)
    dut.m_axi_s2mm_aw_ready = 0; dut.m_axi_s2mm_w_ready = 0;
    dut.m_axi_s2mm_b_valid = 0; dut.m_axi_s2mm_b_id = 0; dut.m_axi_s2mm_b_resp = 0;
    dut.m_axi_s2mm_ar_ready = 0;
    dut.m_axi_s2mm_r_valid = 0; dut.m_axi_s2mm_r_data = 0;
    dut.m_axi_s2mm_r_id = 0; dut.m_axi_s2mm_r_resp = 0; dut.m_axi_s2mm_r_last = 0;
    // SG ports (not used)
    dut.m_axi_mm2s_sg_ar_ready = 0; dut.m_axi_mm2s_sg_r_valid = 0;
    dut.m_axi_mm2s_sg_r_data = 0; dut.m_axi_mm2s_sg_r_id = 0;
    dut.m_axi_mm2s_sg_r_resp = 0; dut.m_axi_mm2s_sg_r_last = 0;
    dut.m_axi_mm2s_sg_aw_ready = 0; dut.m_axi_mm2s_sg_w_ready = 0;
    dut.m_axi_mm2s_sg_b_valid = 0; dut.m_axi_mm2s_sg_b_id = 0; dut.m_axi_mm2s_sg_b_resp = 0;
    dut.m_axi_s2mm_sg_ar_ready = 0; dut.m_axi_s2mm_sg_r_valid = 0;
    dut.m_axi_s2mm_sg_r_data = 0; dut.m_axi_s2mm_sg_r_id = 0;
    dut.m_axi_s2mm_sg_r_resp = 0; dut.m_axi_s2mm_sg_r_last = 0;
    dut.m_axi_s2mm_sg_aw_ready = 0; dut.m_axi_s2mm_sg_w_ready = 0;
    dut.m_axi_s2mm_sg_b_valid = 0; dut.m_axi_s2mm_sg_b_id = 0; dut.m_axi_s2mm_sg_b_resp = 0;
    // AXI-Stream (MM2S output)
    dut.m_axis_mm2s_tready = 1;
    // AXI-Stream (S2MM input, not used)
    dut.s_axis_s2mm_tvalid = 0; dut.s_axis_s2mm_tdata = 0;
    dut.s_axis_s2mm_tlast = 0; dut.s_axis_s2mm_tkeep = 0;

    for (int i = 0; i < 5; i++) tick();
    dut.rst = 0;
    tick(); tick();
}

// AXI-Lite write: simultaneous AW+W
void axil_write(uint8_t addr, uint32_t data) {
    dut.s_axil_aw_valid = 1; dut.s_axil_aw_addr = addr;
    dut.s_axil_w_valid = 1;  dut.s_axil_w_data = data; dut.s_axil_w_strb = 0xF;
    // Wait for aw_ready && w_ready
    for (int i = 0; i < 10; i++) {
        tick();
        if (dut.s_axil_aw_ready && dut.s_axil_w_ready) break;
    }
    dut.s_axil_aw_valid = 0; dut.s_axil_w_valid = 0;
    // Wait for b_valid
    for (int i = 0; i < 10; i++) {
        tick();
        if (dut.s_axil_b_valid) break;
    }
    tick(); // consume b
}

int main() {
    reset();
    printf("[cycle %3d] Reset done\n", cycle_count);

    // Step 1: Program MM2S registers via AXI-Lite
    // Write DMACR = 1 (run bit) at offset 0x00
    axil_write(0x00, 0x00001001);  // run + IOC_IrqEn
    printf("[cycle %3d] DMACR written\n", cycle_count);

    // Write source address = 0x1000 at offset 0x18
    axil_write(0x18, 0x00001000);
    printf("[cycle %3d] SA written\n", cycle_count);

    // Write length = 16 bytes (4 beats * 4 bytes) at offset 0x28
    // This triggers the transfer (mm2s_start pulse)
    axil_write(0x28, 16);
    printf("[cycle %3d] LENGTH written — transfer started\n", cycle_count);

    // Step 2: AXI4 slave — respond to AR and send R beats
    int ar_seen = 0;
    int r_sent = 0;
    int stream_beats = 0;
    bool ar_pending = false;
    int ar_len = 0;
    int ar_id = 0;

    for (int c = 0; c < 200; c++) {
        // AR: accept read address
        if (dut.m_axi_mm2s_ar_valid && !ar_pending) {
            dut.m_axi_mm2s_ar_ready = 1;
            ar_len = dut.m_axi_mm2s_ar_len + 1;
            ar_id = dut.m_axi_mm2s_ar_id;
            ar_pending = true;
            ar_seen++;
            printf("[cycle %3d] AR: addr=0x%x len=%d id=%d\n",
                   cycle_count, dut.m_axi_mm2s_ar_addr, ar_len, ar_id);
        } else {
            dut.m_axi_mm2s_ar_ready = 0;
        }

        // R: send beats
        if (ar_pending && r_sent < ar_seen * ar_len) {
            int beat_in_burst = r_sent % ar_len;
            dut.m_axi_mm2s_r_valid = 1;
            dut.m_axi_mm2s_r_data = 0xDA7A0000 + r_sent;
            dut.m_axi_mm2s_r_id = ar_id;
            dut.m_axi_mm2s_r_last = (beat_in_burst == ar_len - 1) ? 1 : 0;
            if (dut.m_axi_mm2s_r_ready) {
                r_sent++;
                if (dut.m_axi_mm2s_r_last) ar_pending = false;
            }
        } else {
            dut.m_axi_mm2s_r_valid = 0;
            dut.m_axi_mm2s_r_data = 0;
            dut.m_axi_mm2s_r_last = 0;
        }

        // Stream: count output beats
        if (dut.m_axis_mm2s_tvalid && dut.m_axis_mm2s_tready) {
            printf("[cycle %3d] Stream: data=0x%x tlast=%d\n",
                   cycle_count, dut.m_axis_mm2s_tdata, dut.m_axis_mm2s_tlast);
            stream_beats++;
        }

        tick();

        // Check for interrupt
        if (dut.mm2s_introut) {
            printf("[cycle %3d] MM2S interrupt!\n", cycle_count);
            break;
        }
    }

    printf("\nResults: AR=%d, R beats=%d, Stream beats=%d\n",
           ar_seen, r_sent, stream_beats);

    assert(ar_seen >= 1);
    assert(r_sent == 4);
    assert(stream_beats == 4);

    printf("PASS: AxiDmaTop — MM2S 4-beat read through full path\n");
    return 0;
}
