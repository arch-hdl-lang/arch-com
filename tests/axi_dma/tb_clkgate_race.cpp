// Targeted clock gating race condition tests for AXI DMA.
// Tests:
//   1. Wake-up from gated idle (deadlock test)
//   2. Back-to-back transfers (clock re-gate/ungate)
//   3. Simultaneous channel start
//   4. Rapid single-beat transfers (back pressure on gate timing)
//   5. Enable glitch immunity (enable changes mid-transfer)
//   6. Reset during gated state

#include "VAxiDmaTop.h"
#include <cstdio>
#include <cstdlib>
#include <cstring>

static VAxiDmaTop dut;
static int cycle_count = 0;
static int test_num = 0;

static void tick() {
    dut.clk = 0; dut.eval();
    dut.clk = 1; dut.eval();
    cycle_count++;
}

static void reset_dut() {
    dut.rst = 1;
    dut.s_axil_aw_valid = 0; dut.s_axil_w_valid = 0;
    dut.s_axil_b_ready = 1;  dut.s_axil_ar_valid = 0;
    dut.s_axil_r_ready = 1;
    dut.m_axi_mm2s_ar_ready = 0; dut.m_axi_mm2s_r_valid = 0;
    dut.m_axi_s2mm_aw_ready = 0; dut.m_axi_s2mm_w_ready = 0;
    dut.m_axi_s2mm_b_valid = 0;
    dut.m_axis_mm2s_tready = 1;
    dut.s_axis_s2mm_tvalid = 0;
    dut.m_axi_mm2s_sg_ar_ready = 0; dut.m_axi_mm2s_sg_r_valid = 0;
    dut.m_axi_mm2s_sg_aw_ready = 0; dut.m_axi_mm2s_sg_w_ready = 0;
    dut.m_axi_mm2s_sg_b_valid = 0;
    dut.m_axi_s2mm_sg_ar_ready = 0; dut.m_axi_s2mm_sg_r_valid = 0;
    dut.m_axi_s2mm_sg_aw_ready = 0; dut.m_axi_s2mm_sg_w_ready = 0;
    dut.m_axi_s2mm_sg_b_valid = 0;
    tick(); tick(); tick();
    dut.rst = 0;
    tick();
    cycle_count = 0;
}

static void fail(const char* msg) {
    printf("FAIL Test %d: %s (cycle %d)\n", test_num, msg, cycle_count);
    exit(1);
}

// AXI-Lite write
static void axil_write(uint32_t addr, uint32_t data) {
    dut.s_axil_aw_addr = addr; dut.s_axil_aw_valid = 1;
    dut.s_axil_w_data = data; dut.s_axil_w_strb = 0xF; dut.s_axil_w_valid = 1;
    for (int i = 0; i < 30; i++) {
        tick();
        if (dut.s_axil_aw_ready && dut.s_axil_w_ready) {
            tick();
            dut.s_axil_aw_valid = 0; dut.s_axil_w_valid = 0;
            // wait for B
            for (int j = 0; j < 10; j++) { tick(); if (dut.s_axil_b_valid) return; }
            return;
        }
    }
    fail("axil_write timeout");
}

// AXI-Lite read — single loop handles ar_ready and r_valid in any order/same cycle
static uint32_t axil_read(uint32_t addr) {
    dut.s_axil_ar_addr = addr; dut.s_axil_ar_valid = 1;
    dut.s_axil_r_ready = 1;
    for (int i = 0; i < 40; i++) {
        tick();
        if (dut.s_axil_ar_ready) dut.s_axil_ar_valid = 0;
        if (dut.s_axil_r_valid)  return dut.s_axil_r_data;
    }
    fail("axil_read timeout"); return 0;
}

// Run a single MM2S transfer: N beats from src_addr, return 0 if done within timeout
static int run_mm2s(uint32_t src_addr, int n_beats, uint32_t* mem, uint32_t* unused=nullptr, int timeout=200) {
    // Pre-load memory
    for (int i = 0; i < n_beats; i++) mem[src_addr/4 + i] = 0xA0000000 | (src_addr + i*4);

    // Write DMACR.RS=1
    axil_write(0x00, 1);
    // SA
    axil_write(0x18, src_addr);
    // LENGTH triggers start
    axil_write(0x28, n_beats * 4);

    // Drive AXI4 read slave: accept AR, send R beats
    for (int t = 0; t < timeout; t++) {
        // AR phase
        if (dut.m_axi_mm2s_ar_valid) {
            dut.m_axi_mm2s_ar_ready = 1;
            tick();
            dut.m_axi_mm2s_ar_ready = 0;
            // Send R beats.
            // Check r_ready BEFORE tick: if 1, handshake fires during that tick.
            // After the last beat the FSM moves to Done (r_ready→0 post-posedge),
            // so checking r_ready AFTER tick would always fail on the last beat.
            for (int b = 0; b < n_beats; b++) {
                dut.m_axi_mm2s_r_valid = 1;
                dut.m_axi_mm2s_r_data = mem[src_addr/4 + b];
                dut.m_axi_mm2s_r_last = (b == n_beats-1) ? 1 : 0;
                bool accepted = false;
                for (int w = 0; w < 20; w++) {
                    if (dut.m_axi_mm2s_r_ready) { tick(); accepted = true; break; }
                    tick();
                }
                if (!accepted) fail("r_ready timeout");
            }
            dut.m_axi_mm2s_r_valid = 0; dut.m_axi_mm2s_r_last = 0;
        }
        // Check done via DMASR.IOC_Irq
        uint32_t sr = axil_read(0x04);
        if (sr & (1 << 12)) {
            axil_write(0x04, 1 << 12); // W1C
            return 0; // success
        }
        tick();
    }
    return -1; // timeout
}

// Run a single S2MM transfer: N beats to dst_addr
static int run_s2mm(uint32_t dst_addr, int n_beats, uint32_t* mem, uint32_t* received, int timeout=200) {
    axil_write(0x30, 1);       // S2MM DMACR.RS=1
    axil_write(0x48, dst_addr); // DA
    axil_write(0x58, n_beats * 4); // LENGTH

    int beats_sent = 0;
    for (int t = 0; t < timeout; t++) {
        // Drive stream source
        if (beats_sent < n_beats && dut.s_axis_s2mm_tready) {
            dut.s_axis_s2mm_tvalid = 1;
            dut.s_axis_s2mm_tdata = 0xB0000000 | (dst_addr + beats_sent*4);
            dut.s_axis_s2mm_tlast = (beats_sent == n_beats-1) ? 1 : 0;
            if (received) received[beats_sent] = dut.s_axis_s2mm_tdata;
            beats_sent++;
        } else {
            dut.s_axis_s2mm_tvalid = 0;
        }
        // AW/W/B
        if (dut.m_axi_s2mm_aw_valid) dut.m_axi_s2mm_aw_ready = 1;
        if (dut.m_axi_s2mm_w_valid)  dut.m_axi_s2mm_w_ready = 1;
        if (dut.m_axi_s2mm_b_ready)  dut.m_axi_s2mm_b_valid = 1;
        tick();
        dut.m_axi_s2mm_aw_ready = 0; dut.m_axi_s2mm_w_ready = 0; dut.m_axi_s2mm_b_valid = 0;

        uint32_t sr = axil_read(0x34);
        if (sr & (1 << 12)) {
            axil_write(0x34, 1 << 12);
            return 0;
        }
    }
    return -1;
}

// ── Test 1: Wake-up from idle (deadlock test) ─────────────────────────────────
// After reset both channels are halted → clocks gated.
// A single MM2S transfer must complete — tests that start correctly unblocks the gate.
static void test1_wakeup_from_idle() {
    test_num = 1;
    reset_dut();

    // Verify both channels are halted after reset
    uint32_t sr = axil_read(0x04);
    if (!(sr & 1)) fail("MM2S should be halted after reset");
    sr = axil_read(0x34);
    if (!(sr & 1)) fail("S2MM should be halted after reset");

    uint32_t mem[4096] = {};
    if (run_mm2s(0x100, 4, mem) != 0)
        fail("MM2S 4-beat transfer timed out (deadlock on wake-up from gated idle)");

    printf("PASS Test 1: Wake-up from gated idle\n");
}

// ── Test 2: Back-to-back transfers ───────────────────────────────────────────
// After first transfer completes (channel goes halted → clock re-gated),
// immediately start a second. Tests re-wake-up after re-gating.
static void test2_back_to_back() {
    test_num = 2;
    reset_dut();

    uint32_t mem[4096] = {};

    if (run_mm2s(0x200, 2, mem) != 0) fail("Transfer 1 timed out");

    // Channel is now halted (re-gated). Start again immediately.
    if (run_mm2s(0x300, 2, mem) != 0) fail("Transfer 2 timed out (clock failed to re-wake)");

    // Third, different address
    if (run_mm2s(0x400, 1, mem) != 0) fail("Transfer 3 timed out");

    printf("PASS Test 2: Back-to-back MM2S transfers\n");
}

// ── Test 3: Rapid single-beat transfers ──────────────────────────────────────
// 8 consecutive 1-beat MM2S transfers. Each one forces clock gate→ungate→gate.
static void test3_rapid_single_beat() {
    test_num = 3;
    reset_dut();

    uint32_t mem[4096] = {};
    for (int i = 0; i < 8; i++) {
        if (run_mm2s(0x1000 + i*4, 1, mem, mem, 100) != 0) {
            printf("FAIL Test 3: transfer %d timed out\n", i);
            exit(1);
        }
    }
    printf("PASS Test 3: 8 rapid single-beat MM2S transfers\n");
}

// ── Test 4: Simultaneous MM2S + S2MM ─────────────────────────────────────────
// Start both channels at the same time. Both clocks ungate simultaneously.
static void test4_simultaneous_channels() {
    test_num = 4;
    reset_dut();

    uint32_t mem[4096] = {};
    for (int i = 0; i < 4; i++) mem[0x500/4 + i] = 0xC0000000 | i;

    // Configure both channels
    axil_write(0x00, 1);       // MM2S DMACR.RS=1
    axil_write(0x18, 0x500);   // MM2S SA
    axil_write(0x30, 1);       // S2MM DMACR.RS=1
    axil_write(0x48, 0x600);   // S2MM DA
    axil_write(0x28, 16);      // MM2S LENGTH (starts MM2S)
    axil_write(0x58, 16);      // S2MM LENGTH (starts S2MM)

    bool mm2s_done = false, s2mm_done = false;
    int beats_sent = 0;

    for (int t = 0; t < 400; t++) {
        // Drive MM2S AXI4 read
        if (!mm2s_done && dut.m_axi_mm2s_ar_valid) {
            dut.m_axi_mm2s_ar_ready = 1;
            tick();
            dut.m_axi_mm2s_ar_ready = 0;
            for (int b = 0; b < 4; b++) {
                dut.m_axi_mm2s_r_valid = 1;
                dut.m_axi_mm2s_r_data = mem[0x500/4 + b];
                dut.m_axi_mm2s_r_last = (b == 3) ? 1 : 0;
                for (int w = 0; w < 20; w++) { if (dut.m_axi_mm2s_r_ready) { tick(); break; } tick(); }
            }
            dut.m_axi_mm2s_r_valid = 0; dut.m_axi_mm2s_r_last = 0;
        }
        // Drive S2MM stream
        if (!s2mm_done && beats_sent < 4 && dut.s_axis_s2mm_tready) {
            dut.s_axis_s2mm_tvalid = 1;
            dut.s_axis_s2mm_tdata = 0xD0000000 | beats_sent;
            dut.s_axis_s2mm_tlast = (beats_sent == 3) ? 1 : 0;
            beats_sent++;
        } else {
            dut.s_axis_s2mm_tvalid = 0;
        }
        // S2MM AXI4 write
        if (dut.m_axi_s2mm_aw_valid) dut.m_axi_s2mm_aw_ready = 1;
        if (dut.m_axi_s2mm_w_valid)  dut.m_axi_s2mm_w_ready = 1;
        if (dut.m_axi_s2mm_b_ready)  dut.m_axi_s2mm_b_valid = 1;
        tick();
        dut.m_axi_s2mm_aw_ready = 0; dut.m_axi_s2mm_w_ready = 0; dut.m_axi_s2mm_b_valid = 0;

        if (!mm2s_done && (axil_read(0x04) & (1<<12))) {
            axil_write(0x04, 1<<12); mm2s_done = true;
        }
        if (!s2mm_done && (axil_read(0x34) & (1<<12))) {
            axil_write(0x34, 1<<12); s2mm_done = true;
        }
        if (mm2s_done && s2mm_done) break;
        tick();
    }
    if (!mm2s_done) fail("MM2S timed out in simultaneous test");
    if (!s2mm_done) fail("S2MM timed out in simultaneous test");

    printf("PASS Test 4: Simultaneous MM2S + S2MM\n");
}

// ── Test 5: Enable glitch — start while one channel is mid-transfer ───────────
// MM2S is running (clock not gated). S2MM starts (its clock was gated).
// Verifies independent channel gating doesn't cross-interfere.
static void test5_independent_gating() {
    test_num = 5;
    reset_dut();

    uint32_t mem[4096] = {};
    for (int i = 0; i < 8; i++) mem[0x700/4 + i] = 0xE0000000 | i;

    // Start MM2S (8 beats — long transfer)
    axil_write(0x00, 1);
    axil_write(0x18, 0x700);
    axil_write(0x28, 32); // 8 beats

    // While MM2S AR is pending, start S2MM too
    axil_write(0x30, 1);
    axil_write(0x48, 0x800);
    axil_write(0x58, 8); // 2 beats

    bool mm2s_done = false, s2mm_done = false;
    int mm2s_beats_sent = 0, s2mm_beats = 0;

    for (int t = 0; t < 500; t++) {
        // MM2S read slave
        if (!mm2s_done && dut.m_axi_mm2s_ar_valid && !dut.m_axi_mm2s_r_valid) {
            dut.m_axi_mm2s_ar_ready = 1; tick(); dut.m_axi_mm2s_ar_ready = 0;
            for (int b = 0; b < 8; b++) {
                dut.m_axi_mm2s_r_valid = 1;
                dut.m_axi_mm2s_r_data = mem[0x700/4 + b];
                dut.m_axi_mm2s_r_last = (b==7) ? 1 : 0;
                for (int w = 0; w < 20; w++) { if (dut.m_axi_mm2s_r_ready) { tick(); break; } tick(); }
            }
            dut.m_axi_mm2s_r_valid = 0; dut.m_axi_mm2s_r_last = 0;
        }
        // S2MM stream
        if (!s2mm_done && s2mm_beats < 2 && dut.s_axis_s2mm_tready) {
            dut.s_axis_s2mm_tvalid = 1;
            dut.s_axis_s2mm_tdata = 0xF0000000 | s2mm_beats;
            dut.s_axis_s2mm_tlast = (s2mm_beats==1) ? 1 : 0;
            s2mm_beats++;
        } else { dut.s_axis_s2mm_tvalid = 0; }
        // S2MM write
        if (dut.m_axi_s2mm_aw_valid) dut.m_axi_s2mm_aw_ready = 1;
        if (dut.m_axi_s2mm_w_valid)  dut.m_axi_s2mm_w_ready = 1;
        if (dut.m_axi_s2mm_b_ready)  dut.m_axi_s2mm_b_valid = 1;
        tick();
        dut.m_axi_s2mm_aw_ready = 0; dut.m_axi_s2mm_w_ready = 0; dut.m_axi_s2mm_b_valid = 0;

        if (!mm2s_done && (axil_read(0x04) & (1<<12))) {
            axil_write(0x04, 1<<12); mm2s_done = true;
        }
        if (!s2mm_done && (axil_read(0x34) & (1<<12))) {
            axil_write(0x34, 1<<12); s2mm_done = true;
        }
        if (mm2s_done && s2mm_done) break;
        tick();
    }
    if (!mm2s_done) fail("MM2S timed out in independent gating test");
    if (!s2mm_done) fail("S2MM timed out in independent gating test");

    printf("PASS Test 5: Independent channel gating\n");
}

// ── Test 6: Reset while clock gated ──────────────────────────────────────────
// Apply reset while DMA is idle (clock gated). Verify design comes back clean.
static void test6_reset_while_gated() {
    test_num = 6;
    reset_dut();

    // Both channels idle (gated). Apply another reset mid-idle.
    dut.rst = 1; tick(); tick(); dut.rst = 0; tick();

    // Now start a transfer — must work after re-reset
    uint32_t mem[4096] = {};
    if (run_mm2s(0xA00, 2, mem) != 0)
        fail("MM2S failed after reset-while-gated");

    printf("PASS Test 6: Reset while clock gated\n");
}

// ── Test 7: DMASR.Halted reflects real state ──────────────────────────────────
// After a transfer completes, Halted bit should return to 1.
// Tests that the clock re-gates AND DMASR is readable.
static void test7_halted_flag() {
    test_num = 7;
    reset_dut();

    uint32_t sr = axil_read(0x04);
    if (!(sr & 1)) fail("MM2S Halted should be 1 after reset");

    uint32_t mem[4096] = {};
    if (run_mm2s(0xB00, 2, mem) != 0) fail("transfer failed");

    // After done, FSM returns to Idle → Halted=1
    for (int i = 0; i < 20; i++) { tick(); }
    sr = axil_read(0x04);
    if (!(sr & 1)) fail("MM2S Halted should be 1 after transfer completes");

    printf("PASS Test 7: Halted flag and re-gate after transfer\n");
}

int main() {
    printf("Running clock gate race condition tests...\n");
    test1_wakeup_from_idle();
    test2_back_to_back();
    test3_rapid_single_beat();
    test4_simultaneous_channels();
    test5_independent_gating();
    test6_reset_while_gated();
    test7_halted_flag();
    printf("ALL PASS\n");
    return 0;
}
