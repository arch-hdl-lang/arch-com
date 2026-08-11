// ARCH sim testbench for e203_lsu_ctrl — E203 load-store unit controller.
// Tests: reset state, AGU ICB command routing to DTCM/ITCM/BIU by region
// indication (16-bit region compare, 16-bit TCM address truncation, full
// 32-bit BIU address), ready back-propagation from the selected target,
// response back-routing to AGU vs NICE, the write-back/commit channel
// (wdat/itag/err/buserr/badaddr/ld/st), AGU-over-NICE arbitration and the
// nice_mem_holdup override, and lsu_ctrl_active.
//
// NOTE: this replaces a stale tb (VLsuCtrl.h) that targeted an earlier,
// simplified revision of this fixture. The fixture was rewritten against the
// real E203 RTL and renamed to `e203_lsu_ctrl`; the old tb has not compiled
// since. The `_vltor_tb.cpp` Verilator twin was deleted at the same time (no
// harness path runs vltor TBs) and its coverage folds in here.
//
// Fixture note (asserted as-implemented, not a bug per se): the rsp_target_*
// tracking registers only update on a command fire and never self-clear when
// a response completes, so lsu_ctrl_active stays high after the first
// transaction and the response mux keeps selecting the last target until the
// next command fires.
//
// Sim note: this module has a 2-level backward comb chain (the routing comb
// block computes arb_cmd_ready from the region decode; the earlier arbiter
// comb block consumes it for agu/nice_icb_cmd_ready). The arch sim emitter
// runs a fixed two comb passes per eval() for leaf modules (documented in
// src/sim_codegen/mod.rs — deeper chains would need topo-sort emission), so
// one eval() is not enough to settle the ready path after a stimulus change.
// settle() below calls eval() twice, which is sufficient for this design.
//
// Run with:
//   arch sim tests/e203/e203_lsu_ctrl.arch --tb tests/e203/e203_lsu_ctrl_tb.cpp

#include "Ve203_lsu_ctrl.h"
#include <cstdio>
#include <cstdint>

static int fail_count = 0;

#define CHECK(cond, fmt, ...) do { \
    if (!(cond)) { \
        printf("  FAIL: " fmt "\n", ##__VA_ARGS__); \
        fail_count++; \
    } \
} while(0)

static Ve203_lsu_ctrl* dut;

// Region maps: ITCM at 0x1000_xxxx, DTCM at 0x9000_xxxx, all else -> BIU.
static const uint32_t ITCM_BASE = 0x10000000u;
static const uint32_t DTCM_BASE = 0x90000000u;

static void tick() {
    dut->clk = 0; dut->eval();
    dut->clk = 1; dut->eval();
}

// Settle combinational state after an input change (see sim note above).
static void settle() {
    dut->eval(); dut->eval();
}

static void clear_agu_cmd() {
    dut->agu_icb_cmd_valid = 0;
    dut->agu_icb_cmd_addr = 0;
    dut->agu_icb_cmd_read = 0;
    dut->agu_icb_cmd_wdata = 0;
    dut->agu_icb_cmd_wmask = 0;
    dut->agu_icb_cmd_lock = 0;
    dut->agu_icb_cmd_excl = 0;
    dut->agu_icb_cmd_size = 0;
    dut->agu_icb_cmd_back2agu = 0;
    dut->agu_icb_cmd_usign = 0;
    dut->agu_icb_cmd_itag = 0;
}

static void clear_nice_cmd() {
    dut->nice_mem_holdup = 0;
    dut->nice_icb_cmd_valid = 0;
    dut->nice_icb_cmd_addr = 0;
    dut->nice_icb_cmd_read = 0;
    dut->nice_icb_cmd_wdata = 0;
    dut->nice_icb_cmd_wmask = 0;
    dut->nice_icb_cmd_lock = 0;
    dut->nice_icb_cmd_excl = 0;
    dut->nice_icb_cmd_size = 0;
}

static void clear_slaves() {
    dut->dtcm_icb_cmd_ready = 0;
    dut->dtcm_icb_rsp_valid = 0;
    dut->dtcm_icb_rsp_err = 0;
    dut->dtcm_icb_rsp_excl_ok = 0;
    dut->dtcm_icb_rsp_rdata = 0;
    dut->itcm_icb_cmd_ready = 0;
    dut->itcm_icb_rsp_valid = 0;
    dut->itcm_icb_rsp_err = 0;
    dut->itcm_icb_rsp_excl_ok = 0;
    dut->itcm_icb_rsp_rdata = 0;
    dut->biu_icb_cmd_ready = 0;
    dut->biu_icb_rsp_valid = 0;
    dut->biu_icb_rsp_err = 0;
    dut->biu_icb_rsp_excl_ok = 0;
    dut->biu_icb_rsp_rdata = 0;
}

static void reset() {
    dut->rst_n = 0;
    dut->commit_mret = 0;
    dut->commit_trap = 0;
    dut->itcm_region_indic = ITCM_BASE;
    dut->dtcm_region_indic = DTCM_BASE;
    dut->lsu_o_ready = 1;
    dut->agu_icb_rsp_ready = 1;
    dut->nice_icb_rsp_ready = 1;
    clear_agu_cmd();
    clear_nice_cmd();
    clear_slaves();
    for (int i = 0; i < 3; i++) tick();
    dut->rst_n = 1;
    settle();
}

int main() {
    dut = new Ve203_lsu_ctrl;

    // ── Test 1: Reset / idle state ───────────────────────────────────
    printf("Test 1: Reset state\n");
    reset();
    CHECK(dut->dtcm_icb_cmd_valid == 0, "no dtcm cmd at idle, got %d", dut->dtcm_icb_cmd_valid);
    CHECK(dut->itcm_icb_cmd_valid == 0, "no itcm cmd at idle, got %d", dut->itcm_icb_cmd_valid);
    CHECK(dut->biu_icb_cmd_valid == 0, "no biu cmd at idle, got %d", dut->biu_icb_cmd_valid);
    CHECK(dut->agu_icb_rsp_valid == 0, "no agu rsp at idle, got %d", dut->agu_icb_rsp_valid);
    CHECK(dut->nice_icb_rsp_valid == 0, "no nice rsp at idle, got %d", dut->nice_icb_rsp_valid);
    CHECK(dut->lsu_o_valid == 0, "no writeback at idle, got %d", dut->lsu_o_valid);
    CHECK(dut->lsu_ctrl_active == 0, "lsu_ctrl inactive at idle, got %d", dut->lsu_ctrl_active);
    CHECK(dut->agu_icb_cmd_ready == 0, "agu not ready with no cmd/target, got %d",
          dut->agu_icb_cmd_ready);

    // ── Test 2: AGU read routed to DTCM ──────────────────────────────
    printf("Test 2: AGU read -> DTCM\n");
    dut->agu_icb_cmd_valid = 1;
    dut->agu_icb_cmd_addr = DTCM_BASE + 0x1234;
    dut->agu_icb_cmd_read = 1;
    dut->agu_icb_cmd_size = 2;
    dut->agu_icb_cmd_itag = 1;
    settle();
    CHECK(dut->dtcm_icb_cmd_valid == 1, "dtcm cmd should assert, got %d", dut->dtcm_icb_cmd_valid);
    CHECK(dut->itcm_icb_cmd_valid == 0 && dut->biu_icb_cmd_valid == 0,
          "itcm/biu must stay quiet, got %d/%d", dut->itcm_icb_cmd_valid, dut->biu_icb_cmd_valid);
    CHECK(dut->dtcm_icb_cmd_addr == 0x1234, "dtcm addr should truncate to 16 bits, got 0x%04x",
          dut->dtcm_icb_cmd_addr);
    CHECK(dut->dtcm_icb_cmd_read == 1, "read passes through, got %d", dut->dtcm_icb_cmd_read);
    CHECK(dut->dtcm_icb_cmd_size == 2, "size passes through, got %d", dut->dtcm_icb_cmd_size);
    CHECK(dut->lsu_ctrl_active == 1, "active while a cmd is pending, got %d", dut->lsu_ctrl_active);
    // Ready back-propagation: target not ready -> AGU not ready.
    CHECK(dut->agu_icb_cmd_ready == 0, "agu ready must track dtcm ready (0), got %d",
          dut->agu_icb_cmd_ready);
    dut->dtcm_icb_cmd_ready = 1;
    settle();
    CHECK(dut->agu_icb_cmd_ready == 1, "agu ready must track dtcm ready (1), got %d",
          dut->agu_icb_cmd_ready);
    tick();                             // command fires; target captured
    clear_agu_cmd();
    dut->dtcm_icb_cmd_ready = 0;
    settle();

    // Response phase: DTCM answers, response routes back to the AGU.
    dut->dtcm_icb_rsp_valid = 1;
    dut->dtcm_icb_rsp_rdata = 0xDEADBEEFu;
    settle();
    CHECK(dut->agu_icb_rsp_valid == 1, "agu rsp should assert, got %d", dut->agu_icb_rsp_valid);
    CHECK(dut->nice_icb_rsp_valid == 0, "nice rsp must stay quiet, got %d", dut->nice_icb_rsp_valid);
    CHECK(dut->agu_icb_rsp_rdata == 0xDEADBEEFu, "rdata routes to agu, got 0x%08x",
          dut->agu_icb_rsp_rdata);
    CHECK(dut->dtcm_icb_rsp_ready == 1, "dtcm rsp ready follows agu_icb_rsp_ready, got %d",
          dut->dtcm_icb_rsp_ready);
    CHECK(dut->itcm_icb_rsp_ready == 0 && dut->biu_icb_rsp_ready == 0,
          "other rsp readies stay low, got %d/%d", dut->itcm_icb_rsp_ready, dut->biu_icb_rsp_ready);
    // Write-back channel for the load.
    CHECK(dut->lsu_o_valid == 1, "writeback valid on agu rsp, got %d", dut->lsu_o_valid);
    CHECK(dut->lsu_o_wbck_wdat == 0xDEADBEEFu, "wbck data, got 0x%08x", dut->lsu_o_wbck_wdat);
    CHECK(dut->lsu_o_wbck_itag == 1, "wbck itag captured from cmd, got %d", dut->lsu_o_wbck_itag);
    CHECK(dut->lsu_o_wbck_err == 0, "no wbck err on clean rsp, got %d", dut->lsu_o_wbck_err);
    CHECK(dut->lsu_o_cmt_ld == 1 && dut->lsu_o_cmt_st == 0, "load commit flags, got ld=%d st=%d",
          dut->lsu_o_cmt_ld, dut->lsu_o_cmt_st);
    CHECK(dut->lsu_o_cmt_badaddr == DTCM_BASE + 0x1234, "badaddr holds cmd addr, got 0x%08x",
          dut->lsu_o_cmt_badaddr);
    dut->dtcm_icb_rsp_valid = 0;
    dut->dtcm_icb_rsp_rdata = 0;
    settle();

    // ── Test 3: AGU write routed to ITCM, error response ─────────────
    printf("Test 3: AGU write -> ITCM with bus error\n");
    reset();
    dut->agu_icb_cmd_valid = 1;
    dut->agu_icb_cmd_addr = ITCM_BASE + 0x0040;
    dut->agu_icb_cmd_read = 0;
    dut->agu_icb_cmd_wdata = 0xA5A5A5A5u;
    dut->agu_icb_cmd_wmask = 0xF;
    dut->agu_icb_cmd_size = 2;
    dut->itcm_icb_cmd_ready = 1;
    settle();
    CHECK(dut->itcm_icb_cmd_valid == 1, "itcm cmd should assert, got %d", dut->itcm_icb_cmd_valid);
    CHECK(dut->dtcm_icb_cmd_valid == 0 && dut->biu_icb_cmd_valid == 0,
          "dtcm/biu must stay quiet, got %d/%d", dut->dtcm_icb_cmd_valid, dut->biu_icb_cmd_valid);
    CHECK(dut->itcm_icb_cmd_addr == 0x0040, "itcm addr truncates, got 0x%04x", dut->itcm_icb_cmd_addr);
    CHECK(dut->itcm_icb_cmd_read == 0, "write passes through, got %d", dut->itcm_icb_cmd_read);
    CHECK(dut->itcm_icb_cmd_wdata == 0xA5A5A5A5u, "wdata passes through, got 0x%08x",
          dut->itcm_icb_cmd_wdata);
    CHECK(dut->itcm_icb_cmd_wmask == 0xF, "wmask passes through, got 0x%x", dut->itcm_icb_cmd_wmask);
    CHECK(dut->agu_icb_cmd_ready == 1, "agu ready follows itcm ready, got %d", dut->agu_icb_cmd_ready);
    tick();
    clear_agu_cmd();
    dut->itcm_icb_cmd_ready = 0;
    dut->itcm_icb_rsp_valid = 1;
    dut->itcm_icb_rsp_err = 1;          // bus error on the store
    settle();
    CHECK(dut->agu_icb_rsp_valid == 1, "agu rsp on itcm answer, got %d", dut->agu_icb_rsp_valid);
    CHECK(dut->agu_icb_rsp_err == 1, "err routes to agu, got %d", dut->agu_icb_rsp_err);
    CHECK(dut->itcm_icb_rsp_ready == 1, "itcm rsp ready, got %d", dut->itcm_icb_rsp_ready);
    CHECK(dut->lsu_o_wbck_err == 1 && dut->lsu_o_cmt_buserr == 1,
          "wbck/cmt error flags, got %d/%d", dut->lsu_o_wbck_err, dut->lsu_o_cmt_buserr);
    CHECK(dut->lsu_o_cmt_st == 1 && dut->lsu_o_cmt_ld == 0, "store commit flags, got st=%d ld=%d",
          dut->lsu_o_cmt_st, dut->lsu_o_cmt_ld);
    CHECK(dut->lsu_o_cmt_badaddr == ITCM_BASE + 0x0040, "badaddr for faulting store, got 0x%08x",
          dut->lsu_o_cmt_badaddr);
    dut->itcm_icb_rsp_valid = 0;
    dut->itcm_icb_rsp_err = 0;
    settle();

    // ── Test 4: Non-TCM address routed to BIU (full 32-bit addr) ─────
    printf("Test 4: AGU read -> BIU\n");
    reset();
    dut->agu_icb_cmd_valid = 1;
    dut->agu_icb_cmd_addr = 0x80001000u;    // matches neither region
    dut->agu_icb_cmd_read = 1;
    dut->agu_icb_cmd_excl = 1;
    dut->biu_icb_cmd_ready = 1;
    settle();
    CHECK(dut->biu_icb_cmd_valid == 1, "biu cmd should assert, got %d", dut->biu_icb_cmd_valid);
    CHECK(dut->dtcm_icb_cmd_valid == 0 && dut->itcm_icb_cmd_valid == 0,
          "tcm cmds must stay quiet, got %d/%d", dut->dtcm_icb_cmd_valid, dut->itcm_icb_cmd_valid);
    CHECK(dut->biu_icb_cmd_addr == 0x80001000u, "biu keeps the full address, got 0x%08x",
          dut->biu_icb_cmd_addr);
    CHECK(dut->biu_icb_cmd_excl == 1, "excl passes through, got %d", dut->biu_icb_cmd_excl);
    tick();
    clear_agu_cmd();
    dut->biu_icb_cmd_ready = 0;
    dut->biu_icb_rsp_valid = 1;
    dut->biu_icb_rsp_rdata = 0x12345678u;
    dut->biu_icb_rsp_excl_ok = 1;
    settle();
    CHECK(dut->agu_icb_rsp_valid == 1, "agu rsp on biu answer, got %d", dut->agu_icb_rsp_valid);
    CHECK(dut->agu_icb_rsp_rdata == 0x12345678u, "biu rdata routes to agu, got 0x%08x",
          dut->agu_icb_rsp_rdata);
    CHECK(dut->agu_icb_rsp_excl_ok == 1, "excl_ok routes to agu, got %d", dut->agu_icb_rsp_excl_ok);
    CHECK(dut->biu_icb_rsp_ready == 1, "biu rsp ready, got %d", dut->biu_icb_rsp_ready);
    dut->biu_icb_rsp_valid = 0;
    dut->biu_icb_rsp_excl_ok = 0;
    settle();

    // ── Test 5: AGU wins arbitration over NICE ───────────────────────
    printf("Test 5: AGU > NICE arbitration\n");
    reset();
    dut->agu_icb_cmd_valid = 1;
    dut->agu_icb_cmd_addr = DTCM_BASE + 0x100;
    dut->agu_icb_cmd_read = 1;
    dut->nice_icb_cmd_valid = 1;
    dut->nice_icb_cmd_addr = DTCM_BASE + 0x200;
    dut->nice_icb_cmd_read = 1;
    dut->dtcm_icb_cmd_ready = 1;
    settle();
    CHECK(dut->dtcm_icb_cmd_addr == 0x100, "AGU address must win, got 0x%04x", dut->dtcm_icb_cmd_addr);
    CHECK(dut->agu_icb_cmd_ready == 1, "AGU gets the grant, got %d", dut->agu_icb_cmd_ready);
    CHECK(dut->nice_icb_cmd_ready == 0, "NICE must be blocked, got %d", dut->nice_icb_cmd_ready);

    // ── Test 6: nice_mem_holdup overrides AGU priority ───────────────
    printf("Test 6: nice_mem_holdup override\n");
    dut->nice_mem_holdup = 1;
    settle();
    CHECK(dut->dtcm_icb_cmd_addr == 0x200, "NICE address must win under holdup, got 0x%04x",
          dut->dtcm_icb_cmd_addr);
    CHECK(dut->nice_icb_cmd_ready == 1, "NICE gets the grant under holdup, got %d",
          dut->nice_icb_cmd_ready);
    CHECK(dut->agu_icb_cmd_ready == 0, "AGU must be blocked under holdup, got %d",
          dut->agu_icb_cmd_ready);
    tick();                             // NICE command fires
    clear_agu_cmd();
    clear_nice_cmd();
    dut->dtcm_icb_cmd_ready = 0;
    // NICE response: routes to the NICE port only, no write-back.
    dut->dtcm_icb_rsp_valid = 1;
    dut->dtcm_icb_rsp_rdata = 0xCAFED00Du;
    dut->agu_icb_rsp_ready = 0;         // only NICE is listening
    settle();
    CHECK(dut->nice_icb_rsp_valid == 1, "nice rsp should assert, got %d", dut->nice_icb_rsp_valid);
    CHECK(dut->agu_icb_rsp_valid == 0, "agu rsp must stay quiet, got %d", dut->agu_icb_rsp_valid);
    CHECK(dut->nice_icb_rsp_rdata == 0xCAFED00Du, "rdata routes to nice, got 0x%08x",
          dut->nice_icb_rsp_rdata);
    CHECK(dut->dtcm_icb_rsp_ready == 1, "dtcm rsp ready follows nice_icb_rsp_ready, got %d",
          dut->dtcm_icb_rsp_ready);
    CHECK(dut->lsu_o_valid == 0, "NICE transactions produce no write-back, got %d", dut->lsu_o_valid);
    dut->dtcm_icb_rsp_valid = 0;
    dut->dtcm_icb_rsp_rdata = 0;
    dut->agu_icb_rsp_ready = 1;
    settle();

    // ── Test 7: NICE-only command is accepted without holdup ─────────
    printf("Test 7: NICE alone\n");
    reset();
    dut->nice_icb_cmd_valid = 1;
    dut->nice_icb_cmd_addr = ITCM_BASE + 0x80;
    dut->nice_icb_cmd_read = 1;
    dut->itcm_icb_cmd_ready = 1;
    settle();
    CHECK(dut->itcm_icb_cmd_valid == 1, "NICE cmd routes to itcm, got %d", dut->itcm_icb_cmd_valid);
    CHECK(dut->itcm_icb_cmd_addr == 0x80, "NICE addr truncates for itcm, got 0x%04x",
          dut->itcm_icb_cmd_addr);
    CHECK(dut->nice_icb_cmd_ready == 1, "NICE granted with no AGU contender, got %d",
          dut->nice_icb_cmd_ready);
    CHECK(dut->lsu_ctrl_active == 1, "active with NICE cmd pending, got %d", dut->lsu_ctrl_active);

    // ── Test 8: Fixture-pinned rsp_target persistence ────────────────
    printf("Test 8: target tracking persists after response\n");
    tick();                             // NICE->ITCM fires
    clear_nice_cmd();
    dut->itcm_icb_cmd_ready = 0;
    dut->itcm_icb_rsp_valid = 1;
    settle();
    CHECK(dut->nice_icb_rsp_valid == 1, "nice rsp asserts, got %d", dut->nice_icb_rsp_valid);
    dut->itcm_icb_rsp_valid = 0;
    tick();
    settle();
    // rsp_target_itcm only clears on the next cmd fire (see header note).
    CHECK(dut->lsu_ctrl_active == 1, "fixture: active stays high after rsp (target regs persist), got %d",
          dut->lsu_ctrl_active);

    printf("\n=== e203_lsu_ctrl: %d failure(s) ===\n", fail_count);
    return fail_count ? 1 : 0;
}
