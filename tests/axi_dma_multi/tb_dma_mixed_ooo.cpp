// End-to-end testbench: concurrent MM2S read + S2MM write with
// interleaved out-of-order completions.
//
// Scenario:
//   - MM2S: 1 burst read of 4 beats from 0x1000
//   - S2MM: 1 burst write of 4 beats to 0x2000
//   Both channels run concurrently. The AXI slave interleaves
//   R data beats and B write response to stress simultaneous operation.
//
// Timeline:
//   1. Program both MM2S and S2MM via AXI-Lite
//   2. Feed S2MM stream data (s_axis_s2mm)
//   3. AXI slave: accept MM2S AR, accept S2MM AW
//   4. Interleave: send 2 R beats, then B response, then 2 more R beats
//   5. Verify: MM2S stream output has all 4 beats, S2MM write has all 4 W beats
//   6. Both interrupts fire

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
    memset(&dut, 0, sizeof(dut));
    dut.rst = 1;
    dut.s_axil_b_ready = 1;
    dut.s_axil_r_ready = 1;
    dut.m_axis_mm2s_tready = 1;
    for (int i = 0; i < 5; i++) tick();
    dut.rst = 0;
    tick(); tick();
}

void axil_write(uint8_t addr, uint32_t data) {
    dut.s_axil_aw_valid = 1; dut.s_axil_aw_addr = addr;
    dut.s_axil_w_valid = 1;  dut.s_axil_w_data = data; dut.s_axil_w_strb = 0xF;
    for (int i = 0; i < 10; i++) {
        tick();
        if (dut.s_axil_aw_ready && dut.s_axil_w_ready) break;
    }
    dut.s_axil_aw_valid = 0; dut.s_axil_w_valid = 0;
    for (int i = 0; i < 10; i++) {
        tick();
        if (dut.s_axil_b_valid) break;
    }
    tick();
}

int main() {
    reset();
    printf("[cycle %3d] Reset done\n", cycle_count);

    // ── Program MM2S: read 4 beats from 0x1000 ──────────────────────
    axil_write(0x00, 0x00001001);   // DMACR run + IOC_IrqEn
    axil_write(0x18, 0x00001000);   // SA = 0x1000
    axil_write(0x28, 16);           // LENGTH = 16 bytes → 4 beats → triggers start
    printf("[cycle %3d] MM2S programmed\n", cycle_count);

    // ── Program S2MM: write 4 beats to 0x2000 ──────────────────────
    axil_write(0x30, 0x00001001);   // S2MM DMACR run + IOC_IrqEn
    axil_write(0x48, 0x00002000);   // DA = 0x2000
    axil_write(0x58, 16);           // LENGTH = 16 bytes → triggers start
    printf("[cycle %3d] S2MM programmed\n", cycle_count);

    // ── Run both channels concurrently ──────────────────────────────
    int mm2s_ar_seen = 0;
    int mm2s_r_sent = 0;
    int mm2s_stream_beats = 0;

    int s2mm_aw_seen = 0;
    int s2mm_w_beats = 0;
    int s2mm_b_sent = 0;
    int s2mm_stream_fed = 0;

    bool mm2s_ar_pending = false;
    int mm2s_ar_len = 0;

    bool s2mm_aw_pending = false;
    bool s2mm_b_queued = false;
    int s2mm_b_delay = 0;

    bool mm2s_intr_seen = false;
    bool s2mm_intr_seen = false;

    for (int c = 0; c < 300; c++) {
        // ── Feed S2MM stream input ──────────────────────────────────
        if (s2mm_stream_fed < 4) {
            dut.s_axis_s2mm_tvalid = 1;
            dut.s_axis_s2mm_tdata  = 0x57520000 | s2mm_stream_fed;
            dut.s_axis_s2mm_tlast  = (s2mm_stream_fed == 3) ? 1 : 0;
            dut.s_axis_s2mm_tkeep  = 0xF;
            if (dut.s_axis_s2mm_tready) {
                s2mm_stream_fed++;
            }
        } else {
            dut.s_axis_s2mm_tvalid = 0;
        }

        // ── MM2S AXI slave: accept AR ───────────────────────────────
        if (dut.m_axi_mm2s_ar_valid && !mm2s_ar_pending) {
            dut.m_axi_mm2s_ar_ready = 1;
            mm2s_ar_len = dut.m_axi_mm2s_ar_len + 1;
            mm2s_ar_seen++;
            mm2s_ar_pending = true;
            printf("[cycle %3d] MM2S AR: addr=0x%x len=%d\n",
                   cycle_count, dut.m_axi_mm2s_ar_addr, mm2s_ar_len);
        } else {
            dut.m_axi_mm2s_ar_ready = 0;
        }

        // ── S2MM AXI slave: accept AW ───────────────────────────────
        if (dut.m_axi_s2mm_aw_valid && !s2mm_aw_pending) {
            dut.m_axi_s2mm_aw_ready = 1;
            s2mm_aw_seen++;
            s2mm_aw_pending = true;
            printf("[cycle %3d] S2MM AW: addr=0x%x len=%d\n",
                   cycle_count, dut.m_axi_s2mm_aw_addr, dut.m_axi_s2mm_aw_len + 1);
        } else {
            dut.m_axi_s2mm_aw_ready = 0;
        }

        // ── S2MM AXI slave: accept W beats ──────────────────────────
        dut.m_axi_s2mm_w_ready = s2mm_aw_pending ? 1 : 0;
        if (dut.m_axi_s2mm_w_valid && dut.m_axi_s2mm_w_ready) {
            s2mm_w_beats++;
            printf("[cycle %3d] S2MM W: data=0x%08x last=%d (beat %d)\n",
                   cycle_count, dut.m_axi_s2mm_w_data, dut.m_axi_s2mm_w_last, s2mm_w_beats);
            if (dut.m_axi_s2mm_w_last) {
                s2mm_aw_pending = false;
                s2mm_b_queued = true;
                s2mm_b_delay = 3;  // delay B response by 3 cycles
            }
        }

        // ── Interleaved R data: send 2 beats, pause, send 2 more ───
        dut.m_axi_mm2s_r_valid = 0;
        if (mm2s_ar_pending && mm2s_r_sent < mm2s_ar_len) {
            // Send R beats with a gap: beat 0,1 immediately, then pause
            // for B response, then beat 2,3
            bool send_r = false;
            if (mm2s_r_sent < 2) {
                send_r = true;  // first 2 beats
            } else if (s2mm_b_sent > 0) {
                send_r = true;  // remaining beats after B sent
            }

            if (send_r && dut.m_axi_mm2s_r_ready) {
                dut.m_axi_mm2s_r_valid = 1;
                dut.m_axi_mm2s_r_data = 0x52440000 | mm2s_r_sent;
                dut.m_axi_mm2s_r_id = 0;
                dut.m_axi_mm2s_r_last = (mm2s_r_sent == mm2s_ar_len - 1) ? 1 : 0;
                mm2s_r_sent++;
                if (dut.m_axi_mm2s_r_last) mm2s_ar_pending = false;
            }
        }

        // ── S2MM B response: delayed, interleaved between R beats ───
        dut.m_axi_s2mm_b_valid = 0;
        if (s2mm_b_queued) {
            s2mm_b_delay--;
            if (s2mm_b_delay <= 0 && dut.m_axi_s2mm_b_ready) {
                dut.m_axi_s2mm_b_valid = 1;
                dut.m_axi_s2mm_b_id = 0;
                s2mm_b_queued = false;
                s2mm_b_sent++;
                printf("[cycle %3d] S2MM B response (interleaved between R beats)\n",
                       cycle_count);
            }
        }

        // ── MM2S stream output ──────────────────────────────────────
        if (dut.m_axis_mm2s_tvalid && dut.m_axis_mm2s_tready) {
            printf("[cycle %3d] MM2S Stream: data=0x%08x tlast=%d\n",
                   cycle_count, dut.m_axis_mm2s_tdata, dut.m_axis_mm2s_tlast);
            mm2s_stream_beats++;
        }

        tick();

        // Track interrupts
        if (dut.mm2s_introut && !mm2s_intr_seen) {
            printf("[cycle %3d] MM2S INTERRUPT\n", cycle_count);
            mm2s_intr_seen = true;
        }
        if (dut.s2mm_introut && !s2mm_intr_seen) {
            printf("[cycle %3d] S2MM INTERRUPT\n", cycle_count);
            s2mm_intr_seen = true;
        }

        if (mm2s_intr_seen && s2mm_intr_seen) {
            printf("\n[cycle %3d] Both channels complete.\n", cycle_count);
            break;
        }
    }

    printf("\n=== Results ===\n");
    printf("MM2S: AR=%d, R beats=%d, Stream out=%d, interrupt=%s\n",
           mm2s_ar_seen, mm2s_r_sent, mm2s_stream_beats,
           mm2s_intr_seen ? "YES" : "NO");
    printf("S2MM: AW=%d, W beats=%d, B=%d, Stream in=%d, interrupt=%s\n",
           s2mm_aw_seen, s2mm_w_beats, s2mm_b_sent, s2mm_stream_fed,
           s2mm_intr_seen ? "YES" : "NO");

    assert(mm2s_ar_seen >= 1);
    assert(mm2s_r_sent == 4);
    assert(mm2s_stream_beats == 4);
    assert(mm2s_intr_seen);

    assert(s2mm_aw_seen >= 1);
    assert(s2mm_w_beats == 4);
    assert(s2mm_b_sent == 1);
    assert(s2mm_stream_fed == 4);
    assert(s2mm_intr_seen);

    printf("\nPASS: AxiDmaTop — concurrent MM2S read + S2MM write with interleaved OOO completions\n");
    return 0;
}
