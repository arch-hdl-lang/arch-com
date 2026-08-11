// ARCH sim testbench for e203_exu_csr — E203 machine-mode CSR file.
// Tests: read/write of every implemented CSR (mstatus/mie/mtvec/mscratch/
// mepc/mcause/mtval/mcycle{,h}/minstret{,h}), the read-only mip composed
// from the irq inputs, mhartid, unknown-CSR reads-as-zero, trap-entry side
// effects (MPIE<-MIE, MIE<-0 on cmt_status_ena; mepc/mcause/mtval capture),
// mret restore (MIE<-MPIE, MPIE<-1), the mie-derived interrupt-enable
// outputs, mcycle free-run + minstret commit-gated counting, debug-CSR write
// strobes and read passthrough, and the constant machine-mode/control pins.
//
// NOTE: this replaces a stale tb (VExuCsr.h) that targeted an earlier
// revision of the fixture before the e203 fixtures were rewritten against the
// real E203 RTL, which renamed the construct to `e203_exu_csr`. The old tb
// has not compiled since. Ported to the current class name (Ve203_exu_csr).
//
// Run with:
//   arch sim tests/e203/e203_exu_csr.arch --tb tests/e203/e203_exu_csr_tb.cpp

#include "Ve203_exu_csr.h"
#include <cstdio>
#include <cstdint>

static int fail_count = 0;

#define CHECK(cond, fmt, ...) do { \
    if (!(cond)) { \
        printf("  FAIL: " fmt "\n", ##__VA_ARGS__); \
        fail_count++; \
    } \
} while(0)

static Ve203_exu_csr* dut;

// CSR addresses
enum : uint16_t {
    MSTATUS = 0x300, MIE = 0x304, MTVEC = 0x305, MSCRATCH = 0x340,
    MEPC = 0x341, MCAUSE = 0x342, MTVAL = 0x343, MIP = 0x344,
    MCYCLE = 0xB00, MCYCLEH = 0xB80, MINSTRET = 0xB02, MINSTRETH = 0xB82,
    MVENDORID = 0xF11, MHARTID = 0xF14,
    DCSR = 0x7B0, DPC = 0x7B1, DSCRATCH = 0x7B2,
};

static void tick() {
    dut->clk = 0; dut->eval();
    dut->clk = 1; dut->eval();
}

static void reset() {
    dut->rst_n = 0;
    dut->clk_aon = 0;
    dut->nonflush_cmt_ena = 0;
    dut->csr_ena = 0; dut->csr_wr_en = 0; dut->csr_rd_en = 0;
    dut->csr_idx = 0;
    dut->wbck_csr_dat = 0;
    dut->core_mhartid = 0;
    dut->ext_irq_r = 0; dut->sft_irq_r = 0; dut->tmr_irq_r = 0;
    dut->dcsr_r = 0; dut->dpc_r = 0; dut->dscratch_r = 0;
    dut->dbg_mode = 0; dut->dbg_stopcycle = 0;
    dut->cmt_badaddr = 0; dut->cmt_badaddr_ena = 0;
    dut->cmt_epc = 0; dut->cmt_epc_ena = 0;
    dut->cmt_cause = 0; dut->cmt_cause_ena = 0;
    dut->cmt_status_ena = 0; dut->cmt_instret_ena = 0; dut->cmt_mret_ena = 0;
    for (int i = 0; i < 3; i++) tick();
    dut->rst_n = 1;
    dut->eval();
}

// Write a CSR through the access port (one commit cycle).
static void csr_write(uint16_t idx, uint32_t dat) {
    dut->csr_ena = 1;
    dut->csr_wr_en = 1;
    dut->csr_idx = idx;
    dut->wbck_csr_dat = dat;
    dut->eval();
    tick();
    dut->csr_ena = 0;
    dut->csr_wr_en = 0;
    dut->eval();
}

// Combinational CSR read.
static uint32_t csr_read(uint16_t idx) {
    dut->csr_idx = idx;
    dut->csr_rd_en = 1;
    dut->eval();
    return dut->read_csr_dat;
}

int main() {
    dut = new Ve203_exu_csr;

    // ── Test 1: Reset state and constants ────────────────────────────
    printf("Test 1: Reset state\n");
    reset();
    CHECK(csr_read(MSTATUS) == 0, "mstatus should reset to 0, got 0x%08x", dut->read_csr_dat);
    CHECK(csr_read(MIE) == 0, "mie should reset to 0, got 0x%08x", dut->read_csr_dat);
    CHECK(csr_read(MTVEC) == 0, "mtvec should reset to 0, got 0x%08x", dut->read_csr_dat);
    CHECK(dut->m_mode == 1 && dut->s_mode == 0 && dut->h_mode == 0 && dut->u_mode == 0,
          "E203 is machine-mode only, got m%d s%d h%d u%d", dut->m_mode, dut->s_mode, dut->h_mode, dut->u_mode);
    CHECK(dut->csr_access_ilgl == 0, "access-illegal is tied low, got %d", dut->csr_access_ilgl);
    CHECK(dut->status_mie_r == 0, "global MIE should be 0 after reset, got %d", dut->status_mie_r);

    // ── Test 2: Read/write the plain CSRs ────────────────────────────
    printf("Test 2: CSR read/write\n");
    reset();
    csr_write(MSCRATCH, 0xDEADBEEFu);
    CHECK(csr_read(MSCRATCH) == 0xDEADBEEFu, "mscratch readback wrong, got 0x%08x", dut->read_csr_dat);
    csr_write(MTVEC, 0x80000100u);
    CHECK(csr_read(MTVEC) == 0x80000100u, "mtvec readback wrong, got 0x%08x", dut->read_csr_dat);
    CHECK(dut->csr_mtvec_r == 0x80000100u, "csr_mtvec_r output should track, got 0x%08x", dut->csr_mtvec_r);
    csr_write(MEPC, 0x400);
    CHECK(csr_read(MEPC) == 0x400, "mepc readback wrong, got 0x%08x", dut->read_csr_dat);
    CHECK(dut->csr_epc_r == 0x400, "csr_epc_r output should track, got 0x%08x", dut->csr_epc_r);
    csr_write(MCAUSE, 0x8000000Bu);
    CHECK(csr_read(MCAUSE) == 0x8000000Bu, "mcause readback wrong, got 0x%08x", dut->read_csr_dat);
    csr_write(MTVAL, 0x1234);
    CHECK(csr_read(MTVAL) == 0x1234, "mtval readback wrong, got 0x%08x", dut->read_csr_dat);
    // A write without csr_ena must be ignored.
    dut->csr_wr_en = 1; dut->csr_idx = MSCRATCH; dut->wbck_csr_dat = 0x11111111u;
    tick();
    dut->csr_wr_en = 0;
    CHECK(csr_read(MSCRATCH) == 0xDEADBEEFu, "write without csr_ena must be ignored, got 0x%08x",
          dut->read_csr_dat);
    // Unknown CSR reads as zero.
    CHECK(csr_read(0x306) == 0, "unimplemented CSR should read 0, got 0x%08x", dut->read_csr_dat);
    CHECK(csr_read(MVENDORID) == 0, "mvendorid reads 0, got 0x%08x", dut->read_csr_dat);

    // ── Test 3: mip composition and mhartid ──────────────────────────
    printf("Test 3: mip / mhartid\n");
    reset();
    CHECK(csr_read(MIP) == 0, "mip should be 0 with no irqs, got 0x%08x", dut->read_csr_dat);
    dut->ext_irq_r = 1;
    CHECK(csr_read(MIP) == (1u << 11), "ext irq should set MEIP (bit 11), got 0x%08x", dut->read_csr_dat);
    dut->tmr_irq_r = 1;
    CHECK(csr_read(MIP) == ((1u << 11) | (1u << 7)), "tmr irq should add MTIP (bit 7), got 0x%08x",
          dut->read_csr_dat);
    dut->sft_irq_r = 1;
    CHECK(csr_read(MIP) == ((1u << 11) | (1u << 7) | (1u << 3)),
          "sft irq should add MSIP (bit 3), got 0x%08x", dut->read_csr_dat);
    dut->ext_irq_r = 0; dut->tmr_irq_r = 0; dut->sft_irq_r = 0;
    dut->core_mhartid = 1;
    CHECK(csr_read(MHARTID) == 1, "mhartid should follow core_mhartid, got 0x%08x", dut->read_csr_dat);

    // ── Test 4: mie-derived interrupt enables ────────────────────────
    printf("Test 4: Interrupt enables\n");
    reset();
    csr_write(MIE, (1u << 11) | (1u << 7) | (1u << 3));   // MEIE | MTIE | MSIE
    CHECK(dut->meie_r == 1, "meie should be mie[11], got %d", dut->meie_r);
    CHECK(dut->mtie_r == 1, "mtie should be mie[7], got %d", dut->mtie_r);
    CHECK(dut->msie_r == 1, "msie should be mie[3], got %d", dut->msie_r);
    csr_write(MIE, 1u << 7);
    CHECK(dut->meie_r == 0 && dut->mtie_r == 1 && dut->msie_r == 0,
          "only mtie should remain, got e%d t%d s%d", dut->meie_r, dut->mtie_r, dut->msie_r);
    csr_write(MSTATUS, 1u << 3);                          // global MIE
    CHECK(dut->status_mie_r == 1, "status_mie_r should be mstatus[3], got %d", dut->status_mie_r);

    // ── Test 5: Trap entry and mret ──────────────────────────────────
    printf("Test 5: Trap entry / mret\n");
    reset();
    csr_write(MSTATUS, 1u << 3);                          // MIE=1, MPIE=0
    // Trap: capture epc/cause/tval, stack MIE into MPIE, clear MIE.
    dut->cmt_epc = 0x1004; dut->cmt_epc_ena = 1;
    dut->cmt_cause = 0x0000000Bu; dut->cmt_cause_ena = 1;   // ecall from M
    dut->cmt_badaddr = 0x80001000u; dut->cmt_badaddr_ena = 1;
    dut->cmt_status_ena = 1;
    tick();
    dut->cmt_epc_ena = 0; dut->cmt_cause_ena = 0; dut->cmt_badaddr_ena = 0;
    dut->cmt_status_ena = 0;
    dut->eval();
    CHECK(csr_read(MEPC) == 0x1004, "trap should capture mepc, got 0x%08x", dut->read_csr_dat);
    CHECK(csr_read(MCAUSE) == 0xB, "trap should capture mcause, got 0x%08x", dut->read_csr_dat);
    CHECK(csr_read(MTVAL) == 0x80001000u, "trap should capture mtval, got 0x%08x", dut->read_csr_dat);
    CHECK(csr_read(MSTATUS) == (1u << 7), "trap: MPIE<-MIE(1), MIE<-0 => 0x80, got 0x%08x",
          dut->read_csr_dat);
    CHECK(dut->status_mie_r == 0, "global MIE disabled inside the trap, got %d", dut->status_mie_r);
    // mret: MIE <- MPIE, MPIE <- 1.
    dut->cmt_mret_ena = 1;
    tick();
    dut->cmt_mret_ena = 0;
    dut->eval();
    CHECK(csr_read(MSTATUS) == ((1u << 7) | (1u << 3)), "mret: MIE<-MPIE(1), MPIE<-1 => 0x88, got 0x%08x",
          dut->read_csr_dat);
    CHECK(dut->status_mie_r == 1, "global MIE restored after mret, got %d", dut->status_mie_r);
    // Trap with MIE=0 stacks a zero MPIE.
    csr_write(MSTATUS, 0);
    dut->cmt_status_ena = 1;
    tick();
    dut->cmt_status_ena = 0;
    dut->eval();
    CHECK(csr_read(MSTATUS) == 0, "trap from MIE=0 leaves mstatus 0, got 0x%08x", dut->read_csr_dat);

    // ── Test 6: Counters ─────────────────────────────────────────────
    printf("Test 6: mcycle / minstret\n");
    reset();
    csr_write(MCYCLE, 100);
    uint32_t c0 = csr_read(MCYCLE);
    tick(); tick(); tick();
    uint32_t c1 = csr_read(MCYCLE);
    CHECK(c1 == c0 + 3, "mcycle should free-run (+3), got %u -> %u", c0, c1);
    // mcycle_lo rollover carries into mcycle_hi. Write the hi word first —
    // mcycle free-runs during every write cycle, so the lo=all-ones write
    // must be the last one before the rollover tick.
    csr_write(MCYCLEH, 0);
    csr_write(MCYCLE, 0xFFFFFFFFu);
    tick();
    CHECK(csr_read(MCYCLE) == 0, "mcycle_lo should roll over, got 0x%08x", dut->read_csr_dat);
    CHECK(csr_read(MCYCLEH) == 1, "rollover should carry into mcycle_hi, got 0x%08x", dut->read_csr_dat);
    // minstret only counts committed instructions.
    csr_write(MINSTRET, 0);
    tick(); tick();
    CHECK(csr_read(MINSTRET) == 0, "minstret must not free-run, got 0x%08x", dut->read_csr_dat);
    dut->cmt_instret_ena = 1;
    tick(); tick();
    dut->cmt_instret_ena = 0;
    CHECK(csr_read(MINSTRET) == 2, "minstret should count 2 commits, got 0x%08x", dut->read_csr_dat);
    // minstret_lo rollover carries into minstret_hi.
    csr_write(MINSTRET, 0xFFFFFFFFu);
    csr_write(MINSTRETH, 7);
    dut->cmt_instret_ena = 1;
    tick();
    dut->cmt_instret_ena = 0;
    CHECK(csr_read(MINSTRET) == 0, "minstret_lo should roll over, got 0x%08x", dut->read_csr_dat);
    CHECK(csr_read(MINSTRETH) == 8, "carry into minstret_hi, got 0x%08x", dut->read_csr_dat);

    // ── Test 7: Debug CSRs ───────────────────────────────────────────
    printf("Test 7: Debug CSRs\n");
    reset();
    dut->dcsr_r = 0x40000003u;
    dut->dpc_r = 0x2000;
    dut->dscratch_r = 0x5A5A;
    CHECK(csr_read(DCSR) == 0x40000003u, "dcsr read should pass dcsr_r, got 0x%08x", dut->read_csr_dat);
    CHECK(csr_read(DPC) == 0x2000, "dpc read should pass dpc_r, got 0x%08x", dut->read_csr_dat);
    CHECK(csr_read(DSCRATCH) == 0x5A5A, "dscratch read should pass dscratch_r, got 0x%08x",
          dut->read_csr_dat);
    CHECK(dut->csr_dpc_r == 0x2000, "csr_dpc_r output should track dpc_r, got 0x%08x", dut->csr_dpc_r);
    // Write strobes are combinational and idx-decoded.
    dut->csr_ena = 1; dut->csr_wr_en = 1; dut->csr_idx = DCSR; dut->wbck_csr_dat = 0x77;
    dut->eval();
    CHECK(dut->wr_dcsr_ena == 1, "dcsr write strobe should assert, got %d", dut->wr_dcsr_ena);
    CHECK(dut->wr_dpc_ena == 0 && dut->wr_dscratch_ena == 0, "other strobes stay low, got %d/%d",
          dut->wr_dpc_ena, dut->wr_dscratch_ena);
    CHECK(dut->wr_csr_nxt == 0x77, "wr_csr_nxt should carry the write data, got 0x%08x", dut->wr_csr_nxt);
    dut->csr_idx = DPC;
    dut->eval();
    CHECK(dut->wr_dpc_ena == 1 && dut->wr_dcsr_ena == 0, "dpc strobe should assert, got %d/%d",
          dut->wr_dpc_ena, dut->wr_dcsr_ena);
    dut->csr_idx = DSCRATCH;
    dut->eval();
    CHECK(dut->wr_dscratch_ena == 1, "dscratch strobe should assert, got %d", dut->wr_dscratch_ena);
    dut->csr_ena = 0; dut->csr_wr_en = 0;
    dut->eval();
    CHECK(dut->wr_dscratch_ena == 0, "strobe drops without ena, got %d", dut->wr_dscratch_ena);

    // ── Test 8: Clock-gate control pins ──────────────────────────────
    printf("Test 8: Control pins\n");
    reset();
    CHECK(dut->tm_stop == 0 && dut->core_cgstop == 0 && dut->tcm_cgstop == 0,
          "cg stops low without dbg_stopcycle, got %d/%d/%d", dut->tm_stop, dut->core_cgstop, dut->tcm_cgstop);
    dut->dbg_stopcycle = 1;
    dut->eval();
    CHECK(dut->tm_stop == 1 && dut->core_cgstop == 1 && dut->tcm_cgstop == 1,
          "cg stops follow dbg_stopcycle, got %d/%d/%d", dut->tm_stop, dut->core_cgstop, dut->tcm_cgstop);
    CHECK(dut->nice_xs_off == 0 && dut->itcm_nohold == 0 && dut->mdv_nob2b == 0,
          "tied-low controls, got %d/%d/%d", dut->nice_xs_off, dut->itcm_nohold, dut->mdv_nob2b);

    printf("\n=== e203_exu_csr: %d failure(s) ===\n", fail_count);
    return fail_count ? 1 : 0;
}
