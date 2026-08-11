// ARCH sim testbench for e203_core_top — E203 core integration.
//
// e203_core_top wires four large sub-units together: e203_ifu_top (itself
// ifetch + ift2icb + minidec + litebpu), e203_exu_top (nine sub-instances),
// e203_lsu and e203_biu. Everything below the ICB ports — TCMs, CLINT,
// peripherals — is external and attaches to the exported buses (see
// e203_soc_top). Each sub-unit has its own testbench, so this one checks the
// wrapper's own job: reset state of the exported buses, and that an ICB command
// raised inside the core comes out on the bus the region indications select.
//
// Tests: reset state; the IFU fetch reaching the ITCM ICB port; the same fetch
// routed through the BIU to the MEM ICB port when it falls outside the ITCM
// region; the four peripheral region comparators (ppi/clint/plic/fio) diverting
// the fetch out of MEM space and the BIU refusing an instruction fetch to
// peripheral space; the mem_icb_enable gate; ICB command backpressure freezing
// the PC; and the NICE ICB slave port.
//
// NOTE: this replaces two stale testbenches — e203_core_top_tb.cpp and
// e203_core_top_vltor_tb.cpp — that predate the PR #843 rewiring of these
// fixtures against the real ICB fabric and have not compiled since. The
// _vltor_ variant was a Verilator-targeted duplicate of the same scenarios and
// is deleted rather than ported; `arch sim` is the harness these fixtures run
// under.
//
// Run with:
//   arch sim tests/e203/e203_core_top.arch tests/e203/e203_ifu_top.arch \
//            tests/e203/e203_ifu_ifetch.arch tests/e203/e203_ifu_ift2icb.arch \
//            tests/e203/e203_ifu_minidec.arch tests/e203/e203_ifu_litebpu.arch \
//            tests/e203/e203_exu_top.arch tests/e203/e203_exu_decode.arch \
//            tests/e203/e203_exu_disp.arch tests/e203/e203_exu_oitf.arch \
//            tests/e203/e203_exu_alu.arch tests/e203/e203_exu_longpwbck.arch \
//            tests/e203/e203_exu_wbck.arch tests/e203/e203_exu_commit.arch \
//            tests/e203/e203_exu_csr.arch tests/e203/e203_exu_regfile.arch \
//            tests/e203/e203_lsu.arch tests/e203/e203_biu.arch \
//            --tb tests/e203/e203_core_top_tb.cpp
//
// ── KNOWN ISSUE 1 (arch#800): pc_rtvec is dead inside e203_ifu_ifetch, so the
// core boots from 0x0 rather than the reset vector. Test 2 pins that; every
// address expectation below is written against the 0x0 boot address.
//
// ── KNOWN ISSUE 2: the fetch pipeline deadlocks on the first ICB response, so
// no instruction ever reaches the EXU and the LSU/BIU data path is unreachable
// from inside the core. The cause is entirely inside e203_ifu_top — the
// bridge's `ifu_req_ready = icb_cmd_ready & ~buf_full` versus ifetch's
// `ifu_rsp2ir_ready = ... & ifu_req_ready ...`, so once the bridge's one-entry
// response buffer fills nothing can drain it. It is described in full in
// e203_ifu_top_tb.cpp (KNOWN ISSUE 1) and reported separately. Test 8 pins the
// core-level consequence: exu_active never rises and the LSU buses stay silent.
// Because of it, the only ICB traffic this tb can exercise is instruction
// fetch — which is still enough to cover every region comparator in the BIU,
// since the IFU and LSU share the same arbiter and splitter.
//
// ── KNOWN ISSUE 3: e203_exu_decode and e203_exu_alu disagree on the dec_info
// bus layout, so instructions are misclassified once dispatched. Described in
// full in e203_exu_top_tb.cpp (KNOWN ISSUE 1); unreachable here behind KNOWN
// ISSUE 2, noted so it is not rediscovered.

#include "Ve203_core_top.h"
#include <cstdio>
#include <cstdint>

static int fail_count = 0;

#define CHECK(cond, fmt, ...) do { \
    if (!(cond)) { \
        printf("  FAIL: " fmt "\n", ##__VA_ARGS__); \
        fail_count++; \
    } \
} while(0)

static Ve203_core_top* dut;

// The sim emitter runs a fixed two comb passes per eval(). The chains here are
// four sub-units deep (ifetch -> ift2icb -> biu -> splitter), so settle
// explicitly before sampling combinational outputs.
static void settle() { for (int i = 0; i < 5; i++) dut->eval(); }

static void tick() {
    dut->clk = 0; settle();
    dut->clk = 1; settle();
}

// Address-map configuration. Only the top 16 bits of each *_region_indic are
// compared, so these are distinct 64K-aligned regions; `itcm` decides whether a
// fetch from 0x0000_xxxx goes to the ITCM port or out through the BIU.
struct Cfg {
    uint32_t itcm, dtcm, ppi, clint, plic, fio;
    uint8_t ppi_en, clint_en, plic_en, fio_en, mem_en;
};

// The default map keeps the ITCM over the boot address and parks every
// peripheral somewhere the boot fetch cannot reach.
static Cfg default_cfg() {
    Cfg c;
    c.itcm  = 0x00000000;
    c.dtcm  = 0x10000000;
    c.ppi   = 0x20000000;
    c.clint = 0x30000000;
    c.plic  = 0x40000000;
    c.fio   = 0x50000000;
    c.ppi_en = 1; c.clint_en = 1; c.plic_en = 1; c.fio_en = 1; c.mem_en = 1;
    return c;
}

// Drive every input to a defined value and hold reset for 3 ticks.
static void reset(const Cfg& c) {
    dut->rst_n = 0;
    dut->clk = 0;
    dut->test_mode = 0;
    dut->pc_rtvec = 0x80000000;
    dut->core_mhartid = 0;
    dut->dbg_irq_r = 0;
    dut->lcl_irq_r = 0;
    dut->evt_r = 0;
    dut->ext_irq_r = 0;
    dut->sft_irq_r = 0;
    dut->tmr_irq_r = 0;
    dut->dcsr_r = 0;
    dut->dpc_r = 0;
    dut->dscratch_r = 0;
    dut->dbg_mode = 0;
    dut->dbg_halt_r = 0;
    dut->dbg_step_r = 0;
    dut->dbg_ebreakm_r = 0;
    dut->dbg_stopcycle = 0;

    dut->itcm_region_indic = c.itcm;
    dut->ifu2itcm_holdup = 0;
    dut->ifu2itcm_icb_cmd_ready = 1;
    dut->ifu2itcm_icb_rsp_valid = 0;
    dut->ifu2itcm_icb_rsp_err = 0;
    dut->ifu2itcm_icb_rsp_rdata = 0;

    dut->lsu2itcm_icb_cmd_ready = 1;
    dut->lsu2itcm_icb_rsp_valid = 0;
    dut->lsu2itcm_icb_rsp_err = 0;
    dut->lsu2itcm_icb_rsp_excl_ok = 0;
    dut->lsu2itcm_icb_rsp_rdata = 0;

    dut->dtcm_region_indic = c.dtcm;
    dut->lsu2dtcm_icb_cmd_ready = 1;
    dut->lsu2dtcm_icb_rsp_valid = 0;
    dut->lsu2dtcm_icb_rsp_err = 0;
    dut->lsu2dtcm_icb_rsp_excl_ok = 0;
    dut->lsu2dtcm_icb_rsp_rdata = 0;

    dut->ppi_region_indic = c.ppi;   dut->ppi_icb_enable = c.ppi_en;
    dut->ppi_icb_cmd_ready = 1;   dut->ppi_icb_rsp_valid = 0;
    dut->ppi_icb_rsp_err = 0;     dut->ppi_icb_rsp_excl_ok = 0;   dut->ppi_icb_rsp_rdata = 0;

    dut->clint_region_indic = c.clint; dut->clint_icb_enable = c.clint_en;
    dut->clint_icb_cmd_ready = 1; dut->clint_icb_rsp_valid = 0;
    dut->clint_icb_rsp_err = 0;   dut->clint_icb_rsp_excl_ok = 0; dut->clint_icb_rsp_rdata = 0;

    dut->plic_region_indic = c.plic; dut->plic_icb_enable = c.plic_en;
    dut->plic_icb_cmd_ready = 1;  dut->plic_icb_rsp_valid = 0;
    dut->plic_icb_rsp_err = 0;    dut->plic_icb_rsp_excl_ok = 0;  dut->plic_icb_rsp_rdata = 0;

    dut->fio_region_indic = c.fio; dut->fio_icb_enable = c.fio_en;
    dut->fio_icb_cmd_ready = 1;   dut->fio_icb_rsp_valid = 0;
    dut->fio_icb_rsp_err = 0;     dut->fio_icb_rsp_excl_ok = 0;   dut->fio_icb_rsp_rdata = 0;

    dut->mem_icb_enable = c.mem_en;
    dut->mem_icb_cmd_ready = 1;   dut->mem_icb_rsp_valid = 0;
    dut->mem_icb_rsp_err = 0;     dut->mem_icb_rsp_excl_ok = 0;   dut->mem_icb_rsp_rdata = 0;

    dut->nice_mem_holdup = 0;
    dut->nice_req_ready = 1;
    dut->nice_rsp_multicyc_valid = 0;
    dut->nice_rsp_multicyc_dat = 0;
    dut->nice_rsp_multicyc_err = 0;
    dut->nice_icb_cmd_valid = 0;
    dut->nice_icb_cmd_addr = 0;
    dut->nice_icb_cmd_read = 1;
    dut->nice_icb_cmd_wdata = 0;
    dut->nice_icb_cmd_size = 2;
    dut->nice_icb_rsp_ready = 1;

    for (int i = 0; i < 3; i++) tick();
    dut->rst_n = 1;
    settle();
}

// No peripheral bus should ever see an instruction fetch.
static void check_peripherals_quiet(const char* where) {
    CHECK(dut->ppi_icb_cmd_valid == 0, "%s: ppi_icb_cmd_valid should be 0, got %d",
          where, dut->ppi_icb_cmd_valid);
    CHECK(dut->clint_icb_cmd_valid == 0, "%s: clint_icb_cmd_valid should be 0, got %d",
          where, dut->clint_icb_cmd_valid);
    CHECK(dut->plic_icb_cmd_valid == 0, "%s: plic_icb_cmd_valid should be 0, got %d",
          where, dut->plic_icb_cmd_valid);
    CHECK(dut->fio_icb_cmd_valid == 0, "%s: fio_icb_cmd_valid should be 0, got %d",
          where, dut->fio_icb_cmd_valid);
}

// The LSU only issues on a dispatched load/store, which KNOWN ISSUE 2 prevents.
static void check_lsu_quiet(const char* where) {
    CHECK(dut->lsu2itcm_icb_cmd_valid == 0, "%s: lsu2itcm_icb_cmd_valid should be 0, got %d",
          where, dut->lsu2itcm_icb_cmd_valid);
    CHECK(dut->lsu2dtcm_icb_cmd_valid == 0, "%s: lsu2dtcm_icb_cmd_valid should be 0, got %d",
          where, dut->lsu2dtcm_icb_cmd_valid);
}

// Point one peripheral region at the boot address and confirm the BIU refuses
// to forward the instruction fetch there.
static void fetch_into_peripheral(const char* name, uint32_t Cfg::*field) {
    Cfg c = default_cfg();
    c.itcm = 0x80000000;        // push the fetch out through the BIU
    c.*field = 0x00000000;      // ...and into this peripheral's region
    reset(c);
    CHECK(dut->mem_icb_cmd_valid == 0, "%s: the fetch must not reach MEM, got mem cmd_valid %d",
          name, dut->mem_icb_cmd_valid);
    for (int i = 0; i < 3; i++) {
        tick(); settle();
        check_peripherals_quiet(name);
        CHECK(dut->mem_icb_cmd_valid == 0, "%s: still no MEM command expected (cycle %d)", name, i);
    }
}

int main() {
    dut = new Ve203_core_top;

    // ── Test 1: Reset state ──────────────────────────────────────────
    printf("Test 1: Reset state\n");
    reset(default_cfg());
    CHECK(dut->inspect_pc == 0x0, "inspect_pc should be 0 after reset, got 0x%08x", dut->inspect_pc);
    CHECK(dut->ifu_active == 1, "ifu_active is hardwired true, got %d", dut->ifu_active);
    CHECK(dut->exu_active == 0, "exu_active should be 0 with nothing dispatched, got %d", dut->exu_active);
    CHECK(dut->lsu_active == 0, "lsu_active should be 0 after reset, got %d", dut->lsu_active);
    CHECK(dut->biu_active == 0, "biu_active should be 0 with the fetch going to the ITCM, got %d",
          dut->biu_active);
    CHECK(dut->core_wfi == 0, "core_wfi should be 0 after reset, got %d", dut->core_wfi);
    CHECK(dut->tm_stop == 0, "tm_stop should be 0 after reset, got %d", dut->tm_stop);
    CHECK(dut->core_cgstop == 0, "core_cgstop should be 0 after reset, got %d", dut->core_cgstop);
    CHECK(dut->tcm_cgstop == 0, "tcm_cgstop should be 0 after reset, got %d", dut->tcm_cgstop);
    CHECK(dut->cmt_dpc_ena == 0, "cmt_dpc_ena should be 0 after reset, got %d", dut->cmt_dpc_ena);
    CHECK(dut->cmt_dcause_ena == 0, "cmt_dcause_ena should be 0 after reset, got %d", dut->cmt_dcause_ena);
    CHECK(dut->wr_dcsr_ena == 0, "wr_dcsr_ena should be 0 after reset, got %d", dut->wr_dcsr_ena);
    CHECK(dut->wr_dpc_ena == 0, "wr_dpc_ena should be 0 after reset, got %d", dut->wr_dpc_ena);
    CHECK(dut->wr_dscratch_ena == 0, "wr_dscratch_ena should be 0 after reset, got %d", dut->wr_dscratch_ena);
    CHECK(dut->nice_req_valid == 0, "nice_req_valid should be 0 after reset, got %d", dut->nice_req_valid);
    CHECK(dut->mem_icb_cmd_valid == 0, "mem_icb_cmd_valid should be 0 after reset, got %d",
          dut->mem_icb_cmd_valid);
    check_peripherals_quiet("reset");
    check_lsu_quiet("reset");

    // ── Test 2: pc_rtvec is dead (KNOWN ISSUE 1 / arch#800) ──────────
    printf("Test 2: pc_rtvec reset vector (KNOWN ISSUE 1)\n");
    CHECK(dut->inspect_pc == 0x0,
          "KNOWN ISSUE arch#800: the core boots from 0x0, not pc_rtvec 0x80000000; got 0x%08x",
          dut->inspect_pc);
    CHECK(dut->ifu2itcm_icb_cmd_addr == 0x0002,
          "the boot fetch address should be 0x0002, not derived from pc_rtvec; got 0x%04x",
          dut->ifu2itcm_icb_cmd_addr);

    // ── Test 3: Boot fetch reaches the ITCM ICB port ─────────────────
    // ifetch -> ift2icb -> the core's exported ITCM fetch bus.
    printf("Test 3: Boot fetch on the ITCM ICB port\n");
    CHECK(dut->ifu2itcm_icb_cmd_valid == 1, "ifu2itcm_icb_cmd_valid should be 1 for an ITCM-region pc, got %d",
          dut->ifu2itcm_icb_cmd_valid);
    CHECK(dut->ifu2itcm_icb_rsp_ready == 1, "ifu2itcm_icb_rsp_ready should be 1 with an empty buffer, got %d",
          dut->ifu2itcm_icb_rsp_ready);
    CHECK(dut->biu_active == 0, "the BIU must stay idle for an ITCM-region fetch, got biu_active %d",
          dut->biu_active);

    // ── Test 4: Fetch outside the ITCM routes through the BIU to MEM ─
    // ifetch -> ift2icb -> biu arbiter -> region splitter -> the MEM ICB port.
    printf("Test 4: Fetch routed through the BIU to the MEM ICB port\n");
    {
        Cfg c = default_cfg();
        c.itcm = 0x80000000;
        reset(c);
        CHECK(dut->ifu2itcm_icb_cmd_valid == 0, "no ITCM command expected for a non-ITCM pc, got %d",
              dut->ifu2itcm_icb_cmd_valid);
        CHECK(dut->biu_active == 1, "biu_active should rise once the BIU sees the fetch, got %d",
              dut->biu_active);
        tick(); settle();
        CHECK(dut->mem_icb_cmd_valid == 1, "the fetch should emerge on the MEM ICB port, got %d",
              dut->mem_icb_cmd_valid);
        CHECK(dut->mem_icb_cmd_addr == 0x00000002, "mem_icb_cmd_addr should be the fetch pc 0x2, got 0x%08x",
              dut->mem_icb_cmd_addr);
        CHECK(dut->mem_icb_cmd_read == 1, "an instruction fetch must be a read, got mem_icb_cmd_read %d",
              dut->mem_icb_cmd_read);
        check_peripherals_quiet("mem route");
        check_lsu_quiet("mem route");
    }

    // ── Test 5: Peripheral region comparators divert the fetch ───────
    // Each of the four region comparators must pull the address out of MEM
    // space; the BIU then refuses the fetch outright rather than forwarding an
    // instruction fetch to a peripheral (its ifu_to_peri path).
    printf("Test 5: Peripheral region comparators (ppi/clint/plic/fio)\n");
    fetch_into_peripheral("ppi",   &Cfg::ppi);
    fetch_into_peripheral("clint", &Cfg::clint);
    fetch_into_peripheral("plic",  &Cfg::plic);
    fetch_into_peripheral("fio",   &Cfg::fio);

    // ── Test 6: mem_icb_enable gates the MEM port ────────────────────
    printf("Test 6: mem_icb_enable gate\n");
    {
        Cfg c = default_cfg();
        c.itcm = 0x80000000;
        c.mem_en = 0;
        reset(c);
        for (int i = 0; i < 3; i++) {
            tick(); settle();
            CHECK(dut->mem_icb_cmd_valid == 0,
                  "no MEM command may be issued with mem_icb_enable low (cycle %d), got %d",
                  i, dut->mem_icb_cmd_valid);
            check_peripherals_quiet("mem disabled");
        }
    }

    // ── Test 7: ITCM command backpressure freezes the fetch ──────────
    printf("Test 7: ITCM ICB command backpressure\n");
    reset(default_cfg());
    dut->ifu2itcm_icb_cmd_ready = 0;
    settle();
    for (int i = 0; i < 3; i++) {
        tick(); settle();
        CHECK(dut->ifu2itcm_icb_cmd_valid == 1, "cmd_valid must stay asserted under backpressure (cycle %d)", i);
        CHECK(dut->ifu2itcm_icb_cmd_addr == 0x0002, "cmd_addr must stay stable under backpressure (cycle %d)", i);
        CHECK(dut->inspect_pc == 0x0, "the pc must not advance while stalled (cycle %d), got 0x%08x",
              i, dut->inspect_pc);
    }
    dut->ifu2itcm_icb_cmd_ready = 1;
    settle();
    tick(); settle();
    CHECK(dut->inspect_pc == 0x2, "the pc should advance once the command handshakes, got 0x%08x",
          dut->inspect_pc);

    // ── Test 8: Fetch pipeline lockup (KNOWN ISSUE 2) ────────────────
    // Serve one ITCM response and the core stops fetching forever; the EXU
    // never receives an instruction, so the LSU never issues.
    printf("Test 8: Fetch pipeline lockup (KNOWN ISSUE 2)\n");
    reset(default_cfg());
    dut->ifu2itcm_icb_rsp_rdata = ((uint64_t)0x002081B3ull << 32) | 0x002081B3ull;
    settle();
    tick(); settle();                        // command handshake
    dut->ifu2itcm_icb_rsp_valid = 1;
    settle();
    tick();
    dut->ifu2itcm_icb_rsp_valid = 0;
    settle();
    CHECK(dut->ifu2itcm_icb_rsp_ready == 0, "the bridge buffer should be holding the response, got rsp_ready %d",
          dut->ifu2itcm_icb_rsp_ready);
    for (int i = 0; i < 8; i++) {
        tick(); settle();
        CHECK(dut->exu_active == 0,
              "KNOWN ISSUE 2: no instruction reaches the EXU, exu_active stays 0 (cycle %d), got %d",
              i, dut->exu_active);
        CHECK(dut->ifu2itcm_icb_cmd_valid == 0,
              "KNOWN ISSUE 2: fetching never resumes (cycle %d), got cmd_valid %d",
              i, dut->ifu2itcm_icb_cmd_valid);
        CHECK(dut->inspect_pc == 0x2, "the pc stays frozen at 0x2 while locked (cycle %d), got 0x%08x",
              i, dut->inspect_pc);
        check_lsu_quiet("lockup");
        check_peripherals_quiet("lockup");
    }

    // ── Test 9: NICE ICB slave port ──────────────────────────────────
    // The NICE coprocessor's memory-access port is routed into the LSU, but the
    // LSU fixture does not arbitrate it in: nice_icb_cmd_ready is never
    // asserted, so a NICE memory request is never accepted. Pinned as observed
    // behavior rather than a fabricated expectation.
    printf("Test 9: NICE ICB slave port (inert in this fixture)\n");
    reset(default_cfg());
    CHECK(dut->nice_icb_cmd_ready == 0, "nice_icb_cmd_ready should be 0 when idle, got %d",
          dut->nice_icb_cmd_ready);
    dut->nice_icb_cmd_valid = 1;
    dut->nice_icb_cmd_addr = 0x10000040;
    settle();
    for (int i = 0; i < 3; i++) {
        tick(); settle();
        CHECK(dut->nice_icb_cmd_ready == 0,
              "KNOWN: the LSU fixture never accepts a NICE ICB command (cycle %d), got ready %d",
              i, dut->nice_icb_cmd_ready);
        CHECK(dut->nice_icb_rsp_valid == 0,
              "no NICE ICB response may appear for an unaccepted command (cycle %d), got %d",
              i, dut->nice_icb_rsp_valid);
    }

    printf("\n=== e203_core_top: %d failure(s) ===\n", fail_count);
    return fail_count ? 1 : 0;
}
