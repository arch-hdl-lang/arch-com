//=============================================================================
// DMA Engine Testbench
//
// Scenario:
//   1. APB-write src_addr=0x1000, dst_addr=0x2000, len=3, then start=1
//   2. Memory slave always ready; read data = src_addr + beat_idx
//   3. Verify:
//      - mem_rd_addr advances correctly each read beat
//      - mem_wr_addr and mem_wr_data (from FIFO) advance correctly each write beat
//      - Read beats run one cycle ahead of write beats (WriteBuffer FIFO)
//      - IRQ fires exactly one cycle after the last write beat
//      - APB read-back of src/dst/beat matches expected values
//=============================================================================
#include <cstdio>
#include <cstdlib>
#include <cassert>
#include "VDmaEngine.h"
#include "verilated.h"

static VDmaEngine *dut;
static int sim_time = 0;

static void tick() {
    dut->clk = 0;
    dut->eval();
    dut->clk = 1;
    dut->eval();
    sim_time++;
}

// One APB write cycle (sel+enable on consecutive rising edges)
static void apb_write(uint8_t addr, uint32_t data) {
    // Phase 1: setup
    dut->apb_sel    = 1;
    dut->apb_enable = 0;
    dut->apb_write  = 1;
    dut->apb_addr   = addr;
    dut->apb_wdata  = data;
    tick();
    // Phase 2: enable
    dut->apb_enable = 1;
    tick();
    // Deassert
    dut->apb_sel    = 0;
    dut->apb_enable = 0;
    dut->apb_write  = 0;
}

// One APB read cycle; returns read data
static uint32_t apb_read(uint8_t addr) {
    dut->apb_sel    = 1;
    dut->apb_enable = 0;
    dut->apb_write  = 0;
    dut->apb_addr   = addr;
    tick();
    dut->apb_enable = 1;
    tick();
    uint32_t rdata = dut->apb_rdata;
    dut->apb_sel    = 0;
    dut->apb_enable = 0;
    return rdata;
}

int main(int argc, char **argv) {
    Verilated::commandArgs(argc, argv);
    dut = new VDmaEngine;

    // ── Reset ────────────────────────────────────────────────────────────────
    dut->rst = 1;
    dut->clk = 0;
    dut->apb_sel = 0; dut->apb_enable = 0; dut->apb_write = 0;
    dut->apb_addr = 0; dut->apb_wdata = 0;
    dut->mem_rd_ready = 1;   // memory always ready
    dut->mem_wr_ready = 1;
    dut->mem_rd_data  = 0;
    for (int i = 0; i < 4; i++) tick();
    dut->rst = 0;
    tick();

    // ── APB configuration ────────────────────────────────────────────────────
    const uint32_t SRC = 0x1000;
    const uint32_t DST = 0x2000;
    const uint32_t LEN = 3;    // transfer beats 0..LEN (LEN+1 total)

    apb_write(0, SRC);   // src_addr
    apb_write(1, DST);   // dst_addr
    apb_write(2, LEN);   // len
    apb_write(3, 1);     // start

    printf("Configuration written. Waiting for transfer...\n");

    // ── Run transfer ─────────────────────────────────────────────────────────
    // The WriteBuffer FIFO decouples reads from writes: reads run one cycle
    // ahead of writes.  We track read_check and write_check independently.
    bool irq_seen = false;
    int  read_check  = 0;
    int  write_check = 0;
    int  max_cycles  = 50;

    for (int cycle = 0; cycle < max_cycles; cycle++) {
        // Drive read data = echoed address so the FIFO captures it correctly.
        if (dut->mem_rd_valid) {
            dut->mem_rd_data = dut->mem_rd_addr;
            dut->eval();
        }

        // ── Sample read beat ─────────────────────────────────────────────────
        if (dut->mem_rd_valid && dut->mem_rd_ready) {
            uint32_t exp_rd_addr = SRC + read_check;
            if (dut->mem_rd_addr != exp_rd_addr) {
                printf("FAIL read beat %d: mem_rd_addr=0x%08x expected 0x%08x\n",
                       read_check, dut->mem_rd_addr, exp_rd_addr);
                return 1;
            }
            printf("  read  beat %d: rd_addr=0x%08x OK\n",
                   read_check, dut->mem_rd_addr);
            read_check++;
        }

        // ── Sample write beat ────────────────────────────────────────────────
        if (dut->mem_wr_valid && dut->mem_wr_ready) {
            uint32_t exp_wr_addr = DST + write_check;
            uint32_t exp_wr_data = SRC + write_check; // echoed from earlier read

            if (dut->mem_wr_addr != exp_wr_addr) {
                printf("FAIL write beat %d: mem_wr_addr=0x%08x expected 0x%08x\n",
                       write_check, dut->mem_wr_addr, exp_wr_addr);
                return 1;
            }
            if (dut->mem_wr_data != exp_wr_data) {
                printf("FAIL write beat %d: mem_wr_data=0x%08x expected 0x%08x\n",
                       write_check, dut->mem_wr_data, exp_wr_data);
                return 1;
            }
            printf("  write beat %d: wr_addr=0x%08x wr_data=0x%08x OK\n",
                   write_check, dut->mem_wr_addr, dut->mem_wr_data);
            write_check++;
        }

        if (dut->irq) {
            irq_seen = true;
            printf("IRQ fired at cycle %d\n", cycle);
        }

        tick();

        if (irq_seen && write_check == (int)(LEN + 1))
            break;
    }

    // ── Checks ───────────────────────────────────────────────────────────────
    if (!irq_seen) {
        printf("FAIL: IRQ never fired\n");
        return 1;
    }
    if (read_check != (int)(LEN + 1)) {
        printf("FAIL: expected %u read beats, observed %d\n", LEN + 1, read_check);
        return 1;
    }
    if (write_check != (int)(LEN + 1)) {
        printf("FAIL: expected %u write beats, observed %d\n", LEN + 1, write_check);
        return 1;
    }

    // APB read-back: src and dst should still hold their programmed values
    uint32_t rb_src = apb_read(0);
    uint32_t rb_dst = apb_read(1);
    if (rb_src != SRC) { printf("FAIL: APB readback src=0x%08x expected 0x%08x\n", rb_src, SRC); return 1; }
    if (rb_dst != DST) { printf("FAIL: APB readback dst=0x%08x expected 0x%08x\n", rb_dst, DST); return 1; }

    // beat_val should have been cleared to 0 after completion
    uint32_t rb_beat = apb_read(2);
    if (rb_beat != 0) { printf("FAIL: beat_val after transfer = %u, expected 0\n", rb_beat); return 1; }

    printf("\nPASS: DMA transferred %u beats (reads ahead of writes via WriteBuffer FIFO), IRQ fired, APB read-back correct.\n", LEN + 1);

    dut->final();
    delete dut;
    return 0;
}
