// ARCH sim testbench for e203_exu_commit — E203 commit unit.
// Tests: clean-commit strobes (instret, nonflush), the exception-cause
// priority chain (IFU misalign > IFU buserr > illegal > ebreak > load
// misalign/buserr > store misalign/buserr > ecall U/M), trap side effects
// (epc/cause/status/badaddr enables, mtvec flush target), mret (status
// restore, epc flush target, exception-beats-mret priority), dret (dpc
// target), branch-mispredict-class flushes with PC+4/PC+2 targets, the
// flush_req set/ack-clear register, WFI flag set/wake (irq-enable gating)
// and halt requests, amo_wait commit gating, debug-mode dpc/dcause strobes,
// and the long-pipe exception port grant.
//
// NOTE: this replaces a stale tb (VExuCommit.h) that targeted an earlier
// revision of the fixture before the e203 fixtures were rewritten against the
// real E203 RTL, which renamed the construct to `e203_exu_commit`. The old tb
// (and its e203_exu_commit_vltor_tb.cpp Verilator twin, deleted with this
// rewrite) has not compiled since. Ported to the current class name
// (Ve203_exu_commit).
//
// Run with:
//   arch sim tests/e203/e203_exu_commit.arch --tb tests/e203/e203_exu_commit_tb.cpp

#include "Ve203_exu_commit.h"
#include <cstdio>
#include <cstdint>

static int fail_count = 0;

#define CHECK(cond, fmt, ...) do { \
    if (!(cond)) { \
        printf("  FAIL: " fmt "\n", ##__VA_ARGS__); \
        fail_count++; \
    } \
} while(0)

static Ve203_exu_commit* dut;

static void tick() {
    dut->clk = 0; dut->eval();
    dut->clk = 1; dut->eval();
}

static void clear_cmt_flags() {
    dut->alu_cmt_i_valid = 0;
    dut->alu_cmt_i_pc = 0; dut->alu_cmt_i_instr = 0; dut->alu_cmt_i_pc_vld = 0;
    dut->alu_cmt_i_imm = 0; dut->alu_cmt_i_rv32 = 1;
    dut->alu_cmt_i_bjp = 0; dut->alu_cmt_i_wfi = 0; dut->alu_cmt_i_fencei = 0;
    dut->alu_cmt_i_mret = 0; dut->alu_cmt_i_dret = 0;
    dut->alu_cmt_i_ecall = 0; dut->alu_cmt_i_ebreak = 0;
    dut->alu_cmt_i_ifu_misalgn = 0; dut->alu_cmt_i_ifu_buserr = 0; dut->alu_cmt_i_ifu_ilegl = 0;
    dut->alu_cmt_i_bjp_prdt = 0; dut->alu_cmt_i_bjp_rslv = 0;
    dut->alu_cmt_i_misalgn = 0; dut->alu_cmt_i_ld = 0; dut->alu_cmt_i_stamo = 0;
    dut->alu_cmt_i_buserr = 0; dut->alu_cmt_i_badaddr = 0;
}

static void reset() {
    dut->rst_n = 0;
    dut->amo_wait = 0;
    dut->wfi_halt_ifu_ack = 0; dut->wfi_halt_exu_ack = 0;
    dut->dbg_irq_r = 0; dut->lcl_irq_r = 0; dut->ext_irq_r = 0;
    dut->sft_irq_r = 0; dut->tmr_irq_r = 0; dut->evt_r = 0;
    dut->status_mie_r = 0; dut->mtie_r = 0; dut->msie_r = 0; dut->meie_r = 0;
    clear_cmt_flags();
    dut->csr_epc_r = 0; dut->csr_dpc_r = 0; dut->csr_mtvec_r = 0;
    dut->dbg_mode = 0; dut->dbg_halt_r = 0; dut->dbg_step_r = 0; dut->dbg_ebreakm_r = 0;
    dut->oitf_empty = 1;
    dut->u_mode = 0; dut->s_mode = 0; dut->h_mode = 0; dut->m_mode = 1;
    dut->longp_excp_i_valid = 0; dut->longp_excp_i_ld = 0; dut->longp_excp_i_st = 0;
    dut->longp_excp_i_buserr = 0; dut->longp_excp_i_badaddr = 0;
    dut->longp_excp_i_insterr = 0; dut->longp_excp_i_pc = 0;
    dut->pipe_flush_ack = 0;
    for (int i = 0; i < 3; i++) tick();
    dut->rst_n = 1;
    dut->eval();
}

int main() {
    dut = new Ve203_exu_commit;

    // ── Test 1: Clean commit ─────────────────────────────────────────
    printf("Test 1: Clean commit\n");
    reset();
    dut->alu_cmt_i_valid = 1;
    dut->alu_cmt_i_pc = 0x1000;
    dut->eval();
    CHECK(dut->alu_cmt_i_ready == 1, "commit always ready without AMO wait, got %d", dut->alu_cmt_i_ready);
    CHECK(dut->cmt_instret_ena == 1, "clean commit counts instret, got %d", dut->cmt_instret_ena);
    CHECK(dut->nonflush_cmt_ena == 1, "clean commit is nonflush, got %d", dut->nonflush_cmt_ena);
    CHECK(dut->commit_trap == 0 && dut->commit_mret == 0, "no trap/mret, got %d/%d",
          dut->commit_trap, dut->commit_mret);
    CHECK(dut->flush_pulse == 0, "no flush pulse, got %d", dut->flush_pulse);
    CHECK(dut->cmt_epc_ena == 0 && dut->cmt_cause_ena == 0 && dut->cmt_status_ena == 0,
          "no CSR strobes, got %d/%d/%d", dut->cmt_epc_ena, dut->cmt_cause_ena, dut->cmt_status_ena);
    // amo_wait gates the whole commit.
    dut->amo_wait = 1;
    dut->eval();
    CHECK(dut->alu_cmt_i_ready == 0, "AMO wait blocks ready, got %d", dut->alu_cmt_i_ready);
    CHECK(dut->cmt_instret_ena == 0, "AMO wait blocks instret, got %d", dut->cmt_instret_ena);
    dut->amo_wait = 0;

    // ── Test 2: Exception-cause priority chain ───────────────────────
    printf("Test 2: Exception causes\n");
    struct { const char* name; void (*set)(Ve203_exu_commit*); uint32_t cause; } cases[] = {
        { "ifu_misalgn", [](Ve203_exu_commit* d){ d->alu_cmt_i_ifu_misalgn = 1; }, 0 },
        { "ifu_buserr",  [](Ve203_exu_commit* d){ d->alu_cmt_i_ifu_buserr = 1; }, 1 },
        { "ifu_ilegl",   [](Ve203_exu_commit* d){ d->alu_cmt_i_ifu_ilegl = 1; }, 2 },
        { "ebreak",      [](Ve203_exu_commit* d){ d->alu_cmt_i_ebreak = 1; }, 3 },
        { "ld misalgn",  [](Ve203_exu_commit* d){ d->alu_cmt_i_misalgn = 1; d->alu_cmt_i_ld = 1; }, 4 },
        { "ld buserr",   [](Ve203_exu_commit* d){ d->alu_cmt_i_buserr = 1; d->alu_cmt_i_ld = 1; }, 5 },
        { "st misalgn",  [](Ve203_exu_commit* d){ d->alu_cmt_i_misalgn = 1; d->alu_cmt_i_stamo = 1; }, 6 },
        { "st buserr",   [](Ve203_exu_commit* d){ d->alu_cmt_i_buserr = 1; d->alu_cmt_i_stamo = 1; }, 7 },
        { "ecall M",     [](Ve203_exu_commit* d){ d->alu_cmt_i_ecall = 1; }, 11 },
    };
    for (auto& c : cases) {
        reset();
        clear_cmt_flags();
        dut->alu_cmt_i_valid = 1;
        dut->alu_cmt_i_pc = 0x2000;
        c.set(dut);
        dut->eval();
        CHECK(dut->commit_trap == 1, "%s should trap, got %d", c.name, dut->commit_trap);
        CHECK(dut->cmt_cause == c.cause, "%s cause should be %u, got %u", c.name, c.cause, dut->cmt_cause);
        CHECK(dut->cmt_epc_ena == 1, "%s captures epc, got %d", c.name, dut->cmt_epc_ena);
        CHECK(dut->cmt_epc == 0x2000, "%s epc should be the pc, got 0x%08x", c.name, dut->cmt_epc);
        CHECK(dut->cmt_instret_ena == 0, "%s must not count instret, got %d", c.name, dut->cmt_instret_ena);
    }
    // ecall from U mode has cause 8.
    reset();
    clear_cmt_flags();
    dut->alu_cmt_i_valid = 1;
    dut->alu_cmt_i_ecall = 1;
    dut->m_mode = 0; dut->u_mode = 1;
    dut->eval();
    CHECK(dut->cmt_cause == 8, "ecall from U mode is cause 8, got %u", dut->cmt_cause);
    // Priority: IFU misalign beats ebreak.
    reset();
    clear_cmt_flags();
    dut->alu_cmt_i_valid = 1;
    dut->alu_cmt_i_ifu_misalgn = 1;
    dut->alu_cmt_i_ebreak = 1;
    dut->eval();
    CHECK(dut->cmt_cause == 0, "ifu_misalgn beats ebreak in the chain, got %u", dut->cmt_cause);

    // ── Test 3: Trap side effects and mtvec redirect ─────────────────
    printf("Test 3: Trap redirect\n");
    reset();
    dut->csr_mtvec_r = 0x80000040u;
    dut->alu_cmt_i_valid = 1;
    dut->alu_cmt_i_pc = 0x1004;
    dut->alu_cmt_i_buserr = 1;
    dut->alu_cmt_i_ld = 1;
    dut->alu_cmt_i_badaddr = 0xBAD0'0000u & 0xFFFFFFFFu;
    dut->eval();
    CHECK(dut->excp_active == 1, "excp_active mirrors trap, got %d", dut->excp_active);
    CHECK(dut->cmt_status_ena == 1, "trap stacks mstatus, got %d", dut->cmt_status_ena);
    CHECK(dut->cmt_badaddr_ena == 1, "access fault latches badaddr, got %d", dut->cmt_badaddr_ena);
    CHECK(dut->cmt_badaddr == 0xBAD00000u, "badaddr passes through, got 0x%08x", dut->cmt_badaddr);
    CHECK(dut->flush_pulse == 1, "trap pulses a flush, got %d", dut->flush_pulse);
    CHECK(dut->pipe_flush_add_op1 == 0x80000040u && dut->pipe_flush_add_op2 == 0,
          "trap redirects to mtvec, got 0x%08x + 0x%08x", dut->pipe_flush_add_op1, dut->pipe_flush_add_op2);
    CHECK(dut->pipe_flush_pc == 0x80000040u, "flush pc is mtvec, got 0x%08x", dut->pipe_flush_pc);
    // An IFU fault (no ld/stamo) must not latch badaddr.
    clear_cmt_flags();
    dut->alu_cmt_i_valid = 1;
    dut->alu_cmt_i_ifu_ilegl = 1;
    dut->eval();
    CHECK(dut->cmt_badaddr_ena == 0, "illegal-instr trap has no badaddr, got %d", dut->cmt_badaddr_ena);

    // ── Test 4: mret / dret ──────────────────────────────────────────
    printf("Test 4: mret / dret\n");
    reset();
    dut->csr_epc_r = 0x1008;
    dut->alu_cmt_i_valid = 1;
    dut->alu_cmt_i_mret = 1;
    dut->eval();
    CHECK(dut->commit_mret == 1, "mret commits, got %d", dut->commit_mret);
    CHECK(dut->cmt_mret_ena == 1 && dut->cmt_status_ena == 1, "mret restores status, got %d/%d",
          dut->cmt_mret_ena, dut->cmt_status_ena);
    CHECK(dut->pipe_flush_add_op1 == 0x1008 && dut->pipe_flush_pc == 0x1008,
          "mret redirects to epc, got 0x%08x", dut->pipe_flush_add_op1);
    CHECK(dut->cmt_instret_ena == 1, "mret still counts instret, got %d", dut->cmt_instret_ena);
    // Exception on the same beat beats mret.
    dut->alu_cmt_i_ifu_ilegl = 1;
    dut->eval();
    CHECK(dut->commit_mret == 0 && dut->commit_trap == 1, "exception must beat mret, got mret=%d trap=%d",
          dut->commit_mret, dut->commit_trap);
    // dret redirects to dpc.
    clear_cmt_flags();
    dut->alu_cmt_i_valid = 1;
    dut->alu_cmt_i_dret = 1;
    dut->csr_dpc_r = 0x3000;
    dut->eval();
    CHECK(dut->pipe_flush_add_op1 == 0x3000 && dut->pipe_flush_pc == 0x3000,
          "dret redirects to dpc, got 0x%08x", dut->pipe_flush_add_op1);
    CHECK(dut->flush_pulse == 1, "dret pulses a flush, got %d", dut->flush_pulse);

    // ── Test 5: Branch flush targets (PC+4 / PC+2) ───────────────────
    printf("Test 5: Branch flush targets\n");
    reset();
    dut->alu_cmt_i_valid = 1;
    dut->alu_cmt_i_bjp = 1;
    dut->alu_cmt_i_pc = 0x2000;
    dut->alu_cmt_i_rv32 = 1;
    dut->eval();
    CHECK(dut->flush_pulse == 1, "bjp commit pulses a flush, got %d", dut->flush_pulse);
    CHECK(dut->nonflush_cmt_ena == 0, "bjp commit is not nonflush, got %d", dut->nonflush_cmt_ena);
    CHECK(dut->pipe_flush_add_op1 == 0x2000 && dut->pipe_flush_add_op2 == 4,
          "rv32 branch flush target is pc+4, got 0x%08x+0x%x",
          dut->pipe_flush_add_op1, dut->pipe_flush_add_op2);
    dut->alu_cmt_i_rv32 = 0;
    dut->eval();
    CHECK(dut->pipe_flush_add_op2 == 2, "rvc branch flush target is pc+2, got 0x%x",
          dut->pipe_flush_add_op2);

    // ── Test 6: flush_req register set/clear ─────────────────────────
    printf("Test 6: flush_req lifecycle\n");
    reset();
    CHECK(dut->flush_req == 0, "no flush_req after reset, got %d", dut->flush_req);
    dut->alu_cmt_i_valid = 1;
    dut->alu_cmt_i_bjp = 1;
    tick();                          // flush_req_r sets
    clear_cmt_flags();
    dut->eval();
    CHECK(dut->flush_req == 1, "flush_req latches, got %d", dut->flush_req);
    CHECK(dut->pipe_flush_req == 1, "pipe_flush_req mirrors it, got %d", dut->pipe_flush_req);
    for (int i = 0; i < 2; i++) tick();
    CHECK(dut->flush_req == 1, "flush_req holds until acked, got %d", dut->flush_req);
    dut->pipe_flush_ack = 1;
    tick();
    dut->pipe_flush_ack = 0;
    dut->eval();
    CHECK(dut->flush_req == 0, "flush_req clears on ack, got %d", dut->flush_req);

    // ── Test 7: WFI sleep and wake ───────────────────────────────────
    printf("Test 7: WFI\n");
    reset();
    dut->alu_cmt_i_valid = 1;
    dut->alu_cmt_i_wfi = 1;
    tick();                          // wfi_flag_r sets
    clear_cmt_flags();
    dut->eval();
    CHECK(dut->core_wfi == 1, "core sleeps after wfi, got %d", dut->core_wfi);
    CHECK(dut->wfi_halt_ifu_req == 1 && dut->wfi_halt_exu_req == 1,
          "wfi requests both halts, got %d/%d", dut->wfi_halt_ifu_req, dut->wfi_halt_exu_req);
    // A masked interrupt must NOT wake the core (meie=0).
    dut->ext_irq_r = 1;
    tick();
    CHECK(dut->core_wfi == 1, "masked ext irq must not wake, got %d", dut->core_wfi);
    // Enable the irq path: wake.
    dut->meie_r = 1;
    dut->status_mie_r = 1;
    tick();
    CHECK(dut->core_wfi == 0, "enabled ext irq wakes the core, got %d", dut->core_wfi);
    CHECK(dut->wfi_halt_ifu_req == 0, "halt requests drop on wake, got %d", dut->wfi_halt_ifu_req);
    // A wfi committed while an irq is already pending never sleeps.
    dut->alu_cmt_i_valid = 1;
    dut->alu_cmt_i_wfi = 1;
    tick();
    clear_cmt_flags();
    dut->eval();
    CHECK(dut->core_wfi == 0, "wfi with a pending irq does not sleep, got %d", dut->core_wfi);

    // ── Test 8: Debug mode and long-pipe grant ───────────────────────
    printf("Test 8: Debug / longp grant\n");
    reset();
    dut->dbg_mode = 1;
    dut->alu_cmt_i_valid = 1;
    dut->alu_cmt_i_ebreak = 1;
    dut->alu_cmt_i_pc = 0x4000;
    dut->eval();
    CHECK(dut->cmt_dpc_ena == 1, "debug-mode trap strobes dpc, got %d", dut->cmt_dpc_ena);
    CHECK(dut->cmt_dpc == 0x4000, "dpc carries the pc, got 0x%08x", dut->cmt_dpc);
    CHECK(dut->cmt_dcause_ena == 1, "debug-mode trap strobes dcause, got %d", dut->cmt_dcause_ena);
    dut->dbg_mode = 0;
    dut->eval();
    CHECK(dut->cmt_dpc_ena == 0, "no dpc strobe outside debug mode, got %d", dut->cmt_dpc_ena);
    // Long-pipe exception port is granted only when the ALU channel is idle.
    CHECK(dut->longp_excp_i_ready == 0, "longp waits while the ALU commits, got %d",
          dut->longp_excp_i_ready);
    dut->alu_cmt_i_valid = 0;
    dut->eval();
    CHECK(dut->longp_excp_i_ready == 1, "longp granted when the ALU is idle, got %d",
          dut->longp_excp_i_ready);

    printf("\n=== e203_exu_commit: %d failure(s) ===\n", fail_count);
    return fail_count ? 1 : 0;
}
