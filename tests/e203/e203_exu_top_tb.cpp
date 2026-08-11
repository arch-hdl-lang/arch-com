// ARCH sim testbench for e203_exu_top — E203 execution-unit top.
//
// e203_exu_top is an integration wrapper over nine sub-instances:
//   decode -> disp -> {oitf, alu} -> longpwbck -> wbck -> commit, plus csr and
//   regfile. Each of those has its own leaf testbench, so this tb checks what
//   only the wrapper is responsible for: that a value driven at a top-level
//   input actually traverses the instances it is supposed to and comes back out
//   at the right top-level output, that the handshakes are connected in the
//   right direction, and that the glue `comb` block at the bottom of the file
//   routes what it claims to route.
//
// Tests: reset state; decode fan-out to the IFU feedback bus; the
// i_valid/i_ready and exu_active/oitf_empty glue; a full LSU-writeback ->
// longpwbck -> wbck -> regfile -> AGU address round trip across four
// sub-instances; AGU ICB command issue, address formation and backpressure;
// the IFU-side exception inputs reaching commit; and the pipe-flush request /
// acknowledge handshake with its payload.
//
// NOTE: this replaces a stale tb that predates the PR #843 rewiring of these
// fixtures against the real ICB fabric and has not compiled since.
//
// Run with:
//   arch sim tests/e203/e203_exu_top.arch tests/e203/e203_exu_decode.arch \
//            tests/e203/e203_exu_disp.arch tests/e203/e203_exu_oitf.arch \
//            tests/e203/e203_exu_alu.arch tests/e203/e203_exu_longpwbck.arch \
//            tests/e203/e203_exu_wbck.arch tests/e203/e203_exu_commit.arch \
//            tests/e203/e203_exu_csr.arch tests/e203/e203_exu_regfile.arch \
//            --tb tests/e203/e203_exu_top_tb.cpp
//
// ── KNOWN ISSUE 1: e203_exu_decode and e203_exu_alu disagree on the layout of
// the `dec_info` / `i_info` bus, so every instruction is misclassified once it
// crosses that instance boundary. decode builds info as
//     info = <3-bit group id>          // 0=ALU 1=AGU 2=BJP 3=CSR 4=MULDIV
//          | (rv32 ? 8 : 0)            // bit 3
//          | <group-relative sub-op bits, from 0x10 up>
// (see grp_alu/grp_agu/... and info_base in e203_exu_decode.arch), while
// e203_exu_alu decodes it as a per-class bitmap
//     bit0=is_alu bit1=is_bjp bit2=is_csr bit3=is_agu bit4=is_nice bit5=is_mret
//     bit6=is_dret bit7=is_ecall bit8=is_ebreak bit9=is_fencei bit10=is_wfi
//     bit11=is_rv32 bit12=bjp_prdt, sub-ops from bit 13
// (see is_alu/is_bjp/... in e203_exu_alu.arch). The two are incompatible.
// Concretely, every RV32 instruction sets info bit 3, which the ALU reads as
// is_agu, so *every* RV32 instruction issues an AGU ICB command; `sw` sets
// group bit 0 plus 0x20, which the ALU reads as is_alu + is_mret, so a store
// raises commit_mret; `div` sets group 4 plus 0x100, which the ALU reads as
// is_csr + is_ebreak, so a divide raises commit_trap. Because of this the
// wrapper cannot execute instructions correctly, and this tb makes no
// instruction-semantics claims — it tests wiring only. Test 8 pins two of the
// misclassifications as observed behavior so a fix flips them loudly.
// Reported separately.
//
// ── KNOWN ISSUE 2 (pre-existing): e203_exu_alu_muldiv MULH/REM (arch#876) and
// e203_exu_decode SRAI (arch#877). No check below depends on either.
//
// Note on decode outputs at i_ir == 0: an all-zero instruction word is illegal,
// and e203_exu_decode still emits non-zero rs1/rs2/rd indices for it (they are
// raw instruction-field slices). dec2ifu_* are pure combinational functions of
// i_ir, so Test 1 checks only the wrapper's registered/idle outputs, not those.

#include "Ve203_exu_top.h"
#include <cstdio>
#include <cstdint>

static int fail_count = 0;

#define CHECK(cond, fmt, ...) do { \
    if (!(cond)) { \
        printf("  FAIL: " fmt "\n", ##__VA_ARGS__); \
        fail_count++; \
    } \
} while(0)

static Ve203_exu_top* dut;

// RV32 encodings used below.
static const uint32_t IR_DIV_X4_X1_X2    = 0x0220C233u;  // div    x4, x1, x2
static const uint32_t IR_DIVU_X4_X1_X2   = 0x0220D233u;  // divu   x4, x1, x2
static const uint32_t IR_REM_X4_X1_X2    = 0x0220E233u;  // rem    x4, x1, x2
static const uint32_t IR_REMU_X4_X1_X2   = 0x0220F233u;  // remu   x4, x1, x2
static const uint32_t IR_MULHSU_X4_X1_X2 = 0x0220A233u;  // mulhsu x4, x1, x2
static const uint32_t IR_LW_X5_291_X1    = 0x1230A283u;  // lw     x5, 0x123(x1)
static const uint32_t IR_LW_X1_291_X0    = 0x12302083u;  // lw     x1, 0x123(x0)
static const uint32_t IR_SW_X2_256_X1    = 0x1020A023u;  // sw     x2, 0x100(x1)
static const uint32_t IR_ADDI_X1_X0_5    = 0x00500093u;  // addi   x1, x0, 5
static const uint32_t IR_ILLEGAL         = 0xFFFFFFFFu;

// The sim emitter runs a fixed two comb passes per eval(). Chains here cross up
// to five sub-instances (decode -> disp -> alu -> commit -> top glue), so
// settle explicitly before sampling combinational outputs.
static void settle() { for (int i = 0; i < 4; i++) dut->eval(); }

static void tick() {
    dut->clk = 0; dut->clk_aon = 0; settle();
    dut->clk = 1; dut->clk_aon = 1; settle();
}

// Drive every input to a defined value and hold reset for 3 ticks.
// `flush_ack` selects whether the pipe-flush acknowledge is tied high; commit
// clears its flush request whenever pipe_flush_ack is asserted, so Test 7 needs
// it low to observe the request at all.
static void reset(uint8_t flush_ack) {
    dut->rst_n = 0;
    dut->clk = 0;
    dut->clk_aon = 0;
    dut->i_valid = 0;
    dut->i_ir = 0;
    dut->i_pc = 0;
    dut->i_pc_vld = 0;
    dut->i_misalgn = 0;
    dut->i_buserr = 0;
    dut->i_prdt_taken = 0;
    dut->i_muldiv_b2b = 0;
    dut->i_rs1idx = 0;
    dut->i_rs2idx = 0;
    dut->pipe_flush_ack = flush_ack;
    dut->lsu_o_valid = 0;
    dut->lsu_o_wbck_wdat = 0;
    dut->lsu_o_wbck_itag = 0;
    dut->lsu_o_wbck_err = 0;
    dut->lsu_o_cmt_ld = 0;
    dut->lsu_o_cmt_st = 0;
    dut->lsu_o_cmt_badaddr = 0;
    dut->lsu_o_cmt_buserr = 0;
    dut->agu_icb_cmd_ready = 1;
    dut->agu_icb_rsp_valid = 0;
    dut->agu_icb_rsp_rdata = 0;
    dut->agu_icb_rsp_err = 0;
    dut->agu_icb_rsp_excl_ok = 0;
    dut->dbg_mode = 0;
    dut->dbg_halt_r = 0;
    dut->dbg_step_r = 0;
    dut->dbg_ebreakm_r = 0;
    dut->dbg_stopcycle = 0;
    dut->dbg_irq_r = 0;
    dut->lcl_irq_r = 0;
    dut->evt_r = 0;
    dut->ext_irq_r = 0;
    dut->sft_irq_r = 0;
    dut->tmr_irq_r = 0;
    dut->dcsr_r = 0;
    dut->dpc_r = 0;
    dut->dscratch_r = 0;
    dut->wfi_halt_ifu_ack = 0;
    dut->core_mhartid = 0;
    dut->nice_req_ready = 1;
    dut->nice_rsp_multicyc_valid = 0;
    dut->nice_rsp_multicyc_dat = 0;
    dut->nice_rsp_multicyc_err = 0;
    dut->test_mode = 0;
    for (int i = 0; i < 3; i++) tick();
    dut->rst_n = 1;
    settle();
}

// Offer an instruction on the IFU input channel.
static void present(uint32_t ir, uint32_t pc, uint8_t rs1idx, uint8_t rs2idx) {
    dut->i_valid = 1;
    dut->i_ir = ir;
    dut->i_pc = pc;
    dut->i_pc_vld = 1;
    dut->i_rs1idx = rs1idx;
    dut->i_rs2idx = rs2idx;
    settle();
}

int main() {
    dut = new Ve203_exu_top;

    // ── Test 1: Reset state ──────────────────────────────────────────
    printf("Test 1: Reset state\n");
    reset(1);
    CHECK(dut->oitf_empty == 1, "oitf_empty should be 1 after reset, got %d", dut->oitf_empty);
    CHECK(dut->i_ready == 1, "i_ready should be 1 with an idle dispatch stage, got %d", dut->i_ready);
    CHECK(dut->exu_active == 0, "exu_active should be 0 when idle, got %d", dut->exu_active);
    CHECK(dut->excp_active == 0, "excp_active should be 0 after reset, got %d", dut->excp_active);
    CHECK(dut->commit_trap == 0, "commit_trap should be 0 after reset, got %d", dut->commit_trap);
    CHECK(dut->commit_mret == 0, "commit_mret should be 0 after reset, got %d", dut->commit_mret);
    CHECK(dut->core_wfi == 0, "core_wfi should be 0 after reset, got %d", dut->core_wfi);
    CHECK(dut->wfi_halt_ifu_req == 0, "wfi_halt_ifu_req should be 0 after reset, got %d", dut->wfi_halt_ifu_req);
    CHECK(dut->pipe_flush_req == 0, "pipe_flush_req should be 0 after reset, got %d", dut->pipe_flush_req);
    CHECK(dut->agu_icb_cmd_valid == 0, "agu_icb_cmd_valid should be 0 with nothing dispatched, got %d",
          dut->agu_icb_cmd_valid);
    CHECK(dut->nice_req_valid == 0, "nice_req_valid should be 0 with nothing dispatched, got %d",
          dut->nice_req_valid);
    CHECK(dut->agu_icb_rsp_ready == 1, "agu_icb_rsp_ready should be 1 after reset, got %d",
          dut->agu_icb_rsp_ready);
    CHECK(dut->rf2ifu_x1 == 0, "rf2ifu_x1 should be 0 out of reset, got 0x%08x", dut->rf2ifu_x1);
    CHECK(dut->cmt_dpc_ena == 0, "cmt_dpc_ena should be 0 after reset, got %d", dut->cmt_dpc_ena);
    CHECK(dut->cmt_dcause_ena == 0, "cmt_dcause_ena should be 0 after reset, got %d", dut->cmt_dcause_ena);
    // CSR-sourced control outputs are all clear out of reset.
    CHECK(dut->tm_stop == 0, "tm_stop should be 0 after reset, got %d", dut->tm_stop);
    CHECK(dut->itcm_nohold == 0, "itcm_nohold should be 0 after reset, got %d", dut->itcm_nohold);
    CHECK(dut->core_cgstop == 0, "core_cgstop should be 0 after reset, got %d", dut->core_cgstop);
    CHECK(dut->tcm_cgstop == 0, "tcm_cgstop should be 0 after reset, got %d", dut->tcm_cgstop);

    // ── Test 2: Decode fan-out to the IFU feedback bus ───────────────
    // i_ir -> dec instance -> the dec2ifu_* top-level outputs (which the IFU
    // uses for its back-to-back muldiv and rs1 bypass logic). These are direct
    // decode outputs, so they are unaffected by KNOWN ISSUE 1.
    printf("Test 2: Decode fan-out to dec2ifu_*\n");
    reset(1);
    present(IR_DIV_X4_X1_X2, 0x2000, 1, 2);
    CHECK(dut->dec2ifu_div == 1, "dec2ifu_div should be 1 for div, got %d", dut->dec2ifu_div);
    CHECK(dut->dec2ifu_divu == 0, "dec2ifu_divu should be 0 for div, got %d", dut->dec2ifu_divu);
    CHECK(dut->dec2ifu_rem == 0, "dec2ifu_rem should be 0 for div, got %d", dut->dec2ifu_rem);
    CHECK(dut->dec2ifu_rdidx == 4, "dec2ifu_rdidx should be 4 for div x4, got %d", dut->dec2ifu_rdidx);
    CHECK(dut->dec2ifu_rden == 1, "dec2ifu_rden should be 1 for div x4, got %d", dut->dec2ifu_rden);
    CHECK(dut->dec2ifu_rs1en == 1, "dec2ifu_rs1en should be 1 for div, got %d", dut->dec2ifu_rs1en);

    present(IR_DIVU_X4_X1_X2, 0x2000, 1, 2);
    CHECK(dut->dec2ifu_divu == 1, "dec2ifu_divu should be 1 for divu, got %d", dut->dec2ifu_divu);
    CHECK(dut->dec2ifu_div == 0, "dec2ifu_div should be 0 for divu, got %d", dut->dec2ifu_div);

    present(IR_REM_X4_X1_X2, 0x2000, 1, 2);
    CHECK(dut->dec2ifu_rem == 1, "dec2ifu_rem should be 1 for rem, got %d", dut->dec2ifu_rem);
    CHECK(dut->dec2ifu_remu == 0, "dec2ifu_remu should be 0 for rem, got %d", dut->dec2ifu_remu);

    present(IR_REMU_X4_X1_X2, 0x2000, 1, 2);
    CHECK(dut->dec2ifu_remu == 1, "dec2ifu_remu should be 1 for remu, got %d", dut->dec2ifu_remu);
    CHECK(dut->dec2ifu_rem == 0, "dec2ifu_rem should be 0 for remu, got %d", dut->dec2ifu_rem);

    present(IR_MULHSU_X4_X1_X2, 0x2000, 1, 2);
    CHECK(dut->dec2ifu_mulhsu == 1, "dec2ifu_mulhsu should be 1 for mulhsu, got %d", dut->dec2ifu_mulhsu);
    CHECK(dut->dec2ifu_div == 0, "dec2ifu_div should be 0 for mulhsu, got %d", dut->dec2ifu_div);

    // A store writes no register, so the rd-enable feedback must drop.
    present(IR_SW_X2_256_X1, 0x3000, 1, 2);
    CHECK(dut->dec2ifu_rden == 0, "dec2ifu_rden should be 0 for a store, got %d", dut->dec2ifu_rden);
    CHECK(dut->dec2ifu_rs1en == 1, "dec2ifu_rs1en should be 1 for a store (base register), got %d",
          dut->dec2ifu_rs1en);

    // ── Test 3: i_valid / exu_active / oitf_empty glue ───────────────
    // exu_active = (~oitf_empty) | i_valid | excp_active, straight from the
    // wrapper's glue comb block.
    printf("Test 3: exu_active / oitf_empty glue\n");
    reset(1);
    CHECK(dut->exu_active == 0, "exu_active should be 0 when idle, got %d", dut->exu_active);
    dut->i_valid = 1; settle();
    CHECK(dut->exu_active == 1, "exu_active should follow i_valid, got %d", dut->exu_active);
    dut->i_valid = 0; settle();
    CHECK(dut->exu_active == 0, "exu_active should drop with i_valid, got %d", dut->exu_active);

    // Dispatching a register-writing instruction allocates an OITF entry, which
    // both clears oitf_empty and (nothing retiring it) drops i_ready.
    present(IR_LW_X5_291_X1, 0x3000, 1, 0);
    CHECK(dut->oitf_empty == 1, "oitf_empty should still be 1 before the dispatch edge, got %d",
          dut->oitf_empty);
    CHECK(dut->i_ready == 1, "i_ready should be 1 before the dispatch edge, got %d", dut->i_ready);
    tick(); settle();
    dut->i_valid = 0; settle();
    CHECK(dut->oitf_empty == 0, "oitf_empty should clear once an entry is allocated, got %d",
          dut->oitf_empty);
    CHECK(dut->exu_active == 1, "exu_active should stay 1 while the OITF is occupied, got %d",
          dut->exu_active);
    CHECK(dut->i_ready == 0, "i_ready should drop while the OITF entry is unretired, got %d",
          dut->i_ready);

    // ── Test 4: AGU ICB command address formation ────────────────────
    // The immediate has to travel decode -> disp -> alu -> agu adder to reach
    // the ICB command address. With x1 == 0 the address is the immediate.
    printf("Test 4: AGU ICB command address\n");
    reset(1);
    present(IR_LW_X5_291_X1, 0x3000, 1, 0);
    CHECK(dut->agu_icb_cmd_valid == 1, "agu_icb_cmd_valid should assert for a load, got %d",
          dut->agu_icb_cmd_valid);
    CHECK(dut->agu_icb_cmd_addr == 0x123, "agu addr should be x1(0) + 0x123, got 0x%08x",
          dut->agu_icb_cmd_addr);
    present(IR_SW_X2_256_X1, 0x3000, 1, 2);
    CHECK(dut->agu_icb_cmd_addr == 0x100, "agu addr should be x1(0) + 0x100 for the store, got 0x%08x",
          dut->agu_icb_cmd_addr);

    // Backpressure: with cmd_ready low the command must be held, not dropped.
    reset(1);
    dut->agu_icb_cmd_ready = 0;
    present(IR_LW_X5_291_X1, 0x3000, 1, 0);
    for (int i = 0; i < 3; i++) {
        tick(); settle();
        CHECK(dut->agu_icb_cmd_valid == 1, "agu cmd_valid must stay asserted under backpressure (cycle %d)", i);
        CHECK(dut->agu_icb_cmd_addr == 0x123, "agu cmd_addr must stay stable under backpressure (cycle %d)", i);
    }
    dut->agu_icb_cmd_ready = 1;
    dut->i_valid = 0;
    settle();

    // ── Test 5: LSU writeback -> longpwbck -> wbck -> regfile -> AGU ─
    // The longest wiring chain in the wrapper, and the one no leaf tb can
    // cover. A load is dispatched so the OITF allocates an entry recording
    // rd == x1; the LSU then returns a result tagged with that entry, and the
    // value has to travel longpwbck -> wbck -> regfile and reappear on
    // rf2ifu_x1. It is then re-read as the AGU base register for a later load.
    printf("Test 5: LSU writeback round trip through the regfile\n");
    reset(1);
    CHECK(dut->lsu_o_ready == 0, "lsu_o_ready should be 0 with no writeback offered, got %d",
          dut->lsu_o_ready);
    // lw x1, 0x123(x0) — allocates OITF entry 0 with rd == x1.
    present(IR_LW_X1_291_X0, 0x3000, 0, 0);
    tick(); settle();
    dut->i_valid = 0; settle();
    CHECK(dut->oitf_empty == 0, "the load should have allocated an OITF entry, got oitf_empty %d",
          dut->oitf_empty);

    dut->lsu_o_valid = 1;
    dut->lsu_o_wbck_wdat = 0xCAFEBABEu;
    dut->lsu_o_wbck_itag = 0;
    settle();
    CHECK(dut->lsu_o_ready == 1, "lsu_o_ready should assert for an offered writeback, got %d",
          dut->lsu_o_ready);
    tick(); settle();
    dut->lsu_o_valid = 0;
    settle();
    CHECK(dut->rf2ifu_x1 == 0xCAFEBABEu,
          "the LSU writeback should land in x1 and appear on rf2ifu_x1, got 0x%08x", dut->rf2ifu_x1);
    CHECK(dut->oitf_empty == 1, "the tagged writeback should retire the OITF entry, got oitf_empty %d",
          dut->oitf_empty);
    CHECK(dut->i_ready == 1, "i_ready should recover once the OITF drains, got %d", dut->i_ready);

    // Now read x1 back as the load base: rf2ifu_rs1 mirrors the regfile read
    // port, and the AGU address must be x1 + imm.
    present(IR_LW_X5_291_X1, 0x3000, 1, 0);
    CHECK(dut->rf2ifu_rs1 == 0xCAFEBABEu, "rf2ifu_rs1 should mirror the rs1 read port, got 0x%08x",
          dut->rf2ifu_rs1);
    CHECK(dut->agu_icb_cmd_addr == 0xCAFEBBE1u, "agu addr should be 0xCAFEBABE + 0x123, got 0x%08x",
          dut->agu_icb_cmd_addr);
    // The same rs1 value must also reach the NICE request payload.
    CHECK(dut->nice_req_inst == IR_LW_X5_291_X1, "nice_req_inst should carry i_ir, got 0x%08x",
          dut->nice_req_inst);
    CHECK(dut->nice_req_rs1 == 0xCAFEBABEu, "nice_req_rs1 should carry the rs1 read data, got 0x%08x",
          dut->nice_req_rs1);
    dut->i_valid = 0; settle();

    // ── Test 6: IFU exception inputs reach commit ────────────────────
    // i_buserr / i_misalgn / an illegal encoding each have to cross
    // decode -> disp -> alu -> commit to raise commit_trap and excp_active.
    printf("Test 6: IFU exception inputs reach commit\n");
    reset(1);
    present(IR_ILLEGAL, 0x6000, 0, 0);
    CHECK(dut->commit_trap == 1, "an illegal instruction should raise commit_trap, got %d", dut->commit_trap);
    CHECK(dut->excp_active == 1, "an illegal instruction should raise excp_active, got %d", dut->excp_active);

    reset(1);
    dut->i_buserr = 1;
    present(IR_ADDI_X1_X0_5, 0x7000, 0, 0);
    CHECK(dut->commit_trap == 1, "i_buserr should raise commit_trap, got %d", dut->commit_trap);
    CHECK(dut->excp_active == 1, "i_buserr should raise excp_active, got %d", dut->excp_active);

    reset(1);
    dut->i_misalgn = 1;
    present(IR_ADDI_X1_X0_5, 0x7000, 0, 0);
    CHECK(dut->commit_trap == 1, "i_misalgn should raise commit_trap, got %d", dut->commit_trap);
    CHECK(dut->excp_active == 1, "i_misalgn should raise excp_active, got %d", dut->excp_active);

    // ── Test 7: pipe_flush request / acknowledge handshake ───────────
    // commit clears its flush register whenever pipe_flush_ack is high, so the
    // request is only observable with the acknowledge held low.
    printf("Test 7: pipe_flush handshake\n");
    reset(0);
    present(IR_ILLEGAL, 0x6000, 0, 0);
    CHECK(dut->pipe_flush_req == 0, "pipe_flush_req is registered, still 0 in the commit cycle, got %d",
          dut->pipe_flush_req);
    tick(); settle();
    dut->i_valid = 0; settle();
    CHECK(dut->pipe_flush_req == 1, "pipe_flush_req should assert after the exception commits, got %d",
          dut->pipe_flush_req);
    CHECK(dut->pipe_flush_add_op1 == 0x6000, "pipe_flush_add_op1 should carry i_pc 0x6000, got 0x%08x",
          dut->pipe_flush_add_op1);
    CHECK(dut->pipe_flush_pc == 0x6000, "pipe_flush_pc should carry i_pc 0x6000, got 0x%08x",
          dut->pipe_flush_pc);
    // The request latches until it is acknowledged.
    for (int i = 0; i < 3; i++) {
        tick(); settle();
        CHECK(dut->pipe_flush_req == 1, "pipe_flush_req must hold until acknowledged (cycle %d)", i);
    }
    dut->pipe_flush_ack = 1;
    settle();
    tick(); settle();
    CHECK(dut->pipe_flush_req == 0, "pipe_flush_req should clear on the acknowledge, got %d",
          dut->pipe_flush_req);

    // ── Test 8: dec_info encoding mismatch (KNOWN ISSUE 1) ───────────
    // Pinned as observed behavior. If the encodings are ever reconciled these
    // two checks fail, which is the intended signal.
    printf("Test 8: dec_info misclassification (KNOWN ISSUE 1)\n");
    reset(1);
    present(IR_SW_X2_256_X1, 0x3000, 1, 2);
    CHECK(dut->commit_mret == 1,
          "KNOWN ISSUE 1: a store's info sets bit 5, which the ALU reads as is_mret; expected 1, got %d",
          dut->commit_mret);
    reset(1);
    present(IR_DIV_X4_X1_X2, 0x2000, 1, 2);
    CHECK(dut->commit_trap == 1,
          "KNOWN ISSUE 1: a divide's info sets bit 8, which the ALU reads as is_ebreak; expected 1, got %d",
          dut->commit_trap);
    // Every RV32 instruction sets info bit 3, which the ALU reads as is_agu.
    CHECK(dut->agu_icb_cmd_valid == 1,
          "KNOWN ISSUE 1: a divide spuriously issues an AGU ICB command; expected 1, got %d",
          dut->agu_icb_cmd_valid);

    printf("\n=== e203_exu_top: %d failure(s) ===\n", fail_count);
    return fail_count ? 1 : 0;
}
