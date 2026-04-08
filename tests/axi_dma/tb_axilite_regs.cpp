#include "VAxiLiteRegs.h"
#include <cstdio>
#include <cstdlib>

static VAxiLiteRegs dut;
static int cycle_count = 0;

static void tick() {
    dut.clk = 0; dut.eval();
    dut.clk = 1; dut.eval();
    cycle_count++;
}

static void reset() {
    dut.rst = 1;
    dut.axil_aw_addr = 0; dut.axil_aw_valid = 0;
    dut.axil_w_data = 0; dut.axil_w_strb = 0xF; dut.axil_w_valid = 0;
    dut.axil_b_ready = 1;
    dut.axil_ar_addr = 0; dut.axil_ar_valid = 0;
    dut.axil_r_ready = 1;
    dut.mm2s_done = 0; dut.mm2s_halted = 1; dut.mm2s_idle = 1;
    dut.s2mm_done = 0; dut.s2mm_halted = 1; dut.s2mm_idle = 1;
    dut.mm2s_sg_done = 0; dut.s2mm_sg_done = 0;
    dut.mm2s_sg_active = 0; dut.s2mm_sg_active = 0;
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

// AXI4-Lite write: drive AW+W simultaneously, wait for B
static void axil_write(uint32_t addr, uint32_t data) {
    dut.axil_aw_addr = addr;
    dut.axil_aw_valid = 1;
    dut.axil_w_data = data;
    dut.axil_w_strb = 0xF;
    dut.axil_w_valid = 1;
    dut.axil_b_ready = 1;
    // Wait for awready + wready
    for (int i = 0; i < 10; i++) {
        tick();
        if (dut.axil_b_valid) break;
    }
    dut.axil_aw_valid = 0;
    dut.axil_w_valid = 0;
    // Consume B response
    if (!dut.axil_b_valid) tick();
    tick(); // clear bvalid
}

// AXI4-Lite read: drive AR, wait for R
static uint32_t axil_read(uint32_t addr) {
    dut.axil_ar_addr = addr;
    dut.axil_ar_valid = 1;
    dut.axil_r_ready = 1;
    for (int i = 0; i < 10; i++) {
        tick();
        if (dut.axil_r_valid) break;
    }
    dut.axil_ar_valid = 0;
    uint32_t val = dut.axil_r_data;
    tick(); // consume R
    return val;
}

// Test 1: Write and read MM2S registers
static void test_mm2s_register_rw() {
    reset();

    // Write MM2S_DMACR (0x00): RS=1
    axil_write(0x00, 0x0001);
    uint32_t dmacr = axil_read(0x00);
    ASSERT_EQ(dmacr & 1, 1, "MM2S_DMACR.RS");

    // Write MM2S_SA (0x18)
    axil_write(0x18, 0xDEAD1000);
    uint32_t sa = axil_read(0x18);
    ASSERT_EQ(sa, 0xDEAD1000u, "MM2S_SA readback");

    // Write MM2S_LENGTH (0x28) = 16 bytes
    axil_write(0x28, 16);
    uint32_t len = axil_read(0x28);
    ASSERT_EQ(len, 16u, "MM2S_LENGTH readback");

    printf("Test 1 PASS: MM2S register R/W\n");
}

// Test 2: MM2S start pulse on LENGTH write when RS=1
static void test_mm2s_start_pulse() {
    reset();

    // Set RS=1
    axil_write(0x00, 0x0001);
    // Write SA
    axil_write(0x18, 0x1000);
    // Write LENGTH — should trigger mm2s_start pulse
    dut.axil_aw_addr = 0x28;
    dut.axil_aw_valid = 1;
    dut.axil_w_data = 16; // 16 bytes = 4 beats
    dut.axil_w_strb = 0xF;
    dut.axil_w_valid = 1;
    dut.axil_b_ready = 1;

    // Tick until bvalid, watch for start pulse
    int start_seen = 0;
    for (int i = 0; i < 10; i++) {
        tick();
        if (dut.mm2s_start) start_seen = 1;
        if (dut.axil_b_valid) break;
    }
    dut.axil_aw_valid = 0;
    dut.axil_w_valid = 0;

    // Check start pulse + outputs
    ASSERT_EQ(start_seen, 1, "mm2s_start pulsed");
    ASSERT_EQ(dut.mm2s_src_addr, 0x1000u, "mm2s_src_addr");
    ASSERT_EQ(dut.mm2s_num_beats, 4u, "mm2s_num_beats (16/4=4)");

    // start should clear after 1 cycle
    tick();
    ASSERT_EQ(dut.mm2s_start, 0, "mm2s_start clears");

    printf("Test 2 PASS: MM2S start pulse\n");
}

// Test 3: DMASR status bits + W1C
static void test_dmasr_status_and_w1c() {
    reset();

    // DMASR should reflect halted=1, idle=1
    uint32_t sr = axil_read(0x04);
    ASSERT_EQ(sr & 1, 1, "DMASR.Halted when halted=1");
    ASSERT_EQ((sr >> 1) & 1, 1, "DMASR.Idle when idle=1");

    // Simulate: FSM running
    dut.mm2s_halted = 0;
    dut.mm2s_idle = 0;
    tick();
    sr = axil_read(0x04);
    ASSERT_EQ(sr & 1, 0, "DMASR.Halted cleared");
    ASSERT_EQ((sr >> 1) & 1, 0, "DMASR.Idle cleared");

    // Simulate: done pulse → IOC_Irq should set
    dut.mm2s_done = 1;
    tick();
    dut.mm2s_done = 0;
    tick();
    sr = axil_read(0x04);
    ASSERT_EQ((sr >> 12) & 1, 1, "DMASR.IOC_Irq set after done");

    // W1C: write 1 to bit 12 to clear IOC_Irq
    axil_write(0x04, 1 << 12);
    sr = axil_read(0x04);
    ASSERT_EQ((sr >> 12) & 1, 0, "DMASR.IOC_Irq cleared by W1C");

    printf("Test 3 PASS: DMASR status + W1C\n");
}

// Test 4: Interrupt output
static void test_interrupt() {
    reset();

    // Enable IOC interrupt: DMACR bit 12
    axil_write(0x00, (1 << 12) | 1); // IOC_IrqEn + RS

    // No interrupt yet
    dut.eval();
    ASSERT_EQ(dut.mm2s_introut, 0, "no interrupt before done");

    // Trigger done
    dut.mm2s_done = 1;
    tick();
    dut.mm2s_done = 0;
    tick();
    dut.eval();
    ASSERT_EQ(dut.mm2s_introut, 1, "interrupt after done");

    // Clear IOC_Irq
    axil_write(0x04, 1 << 12);
    dut.eval();
    ASSERT_EQ(dut.mm2s_introut, 0, "interrupt clears after W1C");

    printf("Test 4 PASS: interrupt generation\n");
}

// Test 5: S2MM registers
static void test_s2mm_registers() {
    reset();

    axil_write(0x30, 0x0001); // S2MM DMACR RS=1
    axil_write(0x48, 0xBEEF2000); // DA
    axil_write(0x58, 32); // LENGTH = 32 bytes = 8 beats

    uint32_t dmacr = axil_read(0x30);
    ASSERT_EQ(dmacr & 1, 1, "S2MM_DMACR.RS");

    uint32_t da = axil_read(0x48);
    ASSERT_EQ(da, 0xBEEF2000u, "S2MM_DA readback");

    ASSERT_EQ(dut.s2mm_dst_addr, 0xBEEF2000u, "s2mm_dst_addr");
    ASSERT_EQ(dut.s2mm_num_beats, 8u, "s2mm_num_beats (32/4=8)");

    printf("Test 5 PASS: S2MM registers\n");
}

// Test 6: SG registers — CURDESC/TAILDESC R/W + sg_start pulse
static void test_sg_registers() {
    reset();

    // Write DMACR.RS=1
    axil_write(0x00, 0x0001);

    // Write CURDESC (0x08)
    axil_write(0x08, 0x1000);
    ASSERT_EQ(axil_read(0x08), 0x1000u, "MM2S_CURDESC readback");

    // Write TAILDESC (0x10) — should trigger mm2s_sg_start
    int sg_start_seen = 0;
    dut.axil_aw_addr = 0x10; dut.axil_aw_valid = 1;
    dut.axil_w_data = 0x1030; dut.axil_w_strb = 0xF; dut.axil_w_valid = 1;
    for (int i = 0; i < 10; i++) {
        tick();
        if (dut.mm2s_sg_start) sg_start_seen = 1;
        if (dut.axil_b_valid) break;
    }
    dut.axil_aw_valid = 0; dut.axil_w_valid = 0;
    tick();

    ASSERT_EQ(sg_start_seen, 1, "mm2s_sg_start pulsed on TAILDESC write");
    ASSERT_EQ(dut.mm2s_curdesc_o, 0x1000u, "mm2s_curdesc_o");
    ASSERT_EQ(dut.mm2s_taildesc_o, 0x1030u, "mm2s_taildesc_o");
    ASSERT_EQ(axil_read(0x10), 0x1030u, "MM2S_TAILDESC readback");

    // SG done should set IOC_Irq (with IRQ enabled, SG active)
    axil_write(0x00, (1 << 12) | 1); // IOC_IrqEn + RS
    dut.mm2s_sg_active = 1;
    dut.mm2s_sg_done = 1; tick(); dut.mm2s_sg_done = 0; tick();
    dut.eval();
    ASSERT_EQ(dut.mm2s_introut, 1, "interrupt after sg_done");

    printf("Test 6 PASS: SG registers\n");
}

int main() {
    test_mm2s_register_rw();
    test_mm2s_start_pulse();
    test_dmasr_status_and_w1c();
    test_interrupt();
    test_s2mm_registers();
    test_sg_registers();
    printf("PASS\n");
    return 0;
}
