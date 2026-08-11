// ARCH sim testbench for e203_ifu_ifetch — E203 instruction fetch unit.
// Tests: reset state, sequential RV32/RVC fetch, single-outstanding-request
// flow control, IR delivery to decode, bus-error capture, pipeline-flush
// redirect, halt handshake, and downstream backpressure.
//
// NOTE: this replaces a stale tb (VIfuIfetch.h, req_valid/req_addr/redirect
// naming, a hand-rolled Idle/WaitGnt/WaitRsp/Abort FSM) that targeted an
// earlier, simplified revision of this module. That revision was replaced when
// the e203 fixtures were rewritten against the real E203 RTL, which renamed the
// construct to `e203_ifu_ifetch` and moved to the ifu_req_*/ifu_rsp_*/ifu_o_*
// port surface plus minidec + litebpu submodules. The old tb has not compiled
// since. Ported to the current class name (Ve203_ifu_ifetch) and protocol.
//
// Run with:
//   arch sim tests/e203/e203_ifu_ifetch.arch tests/e203/e203_ifu_minidec.arch \
//            tests/e203/e203_ifu_litebpu.arch --tb tests/e203/e203_ifu_ifetch_tb.cpp
//
// BOOT BEHAVIOUR — `pc_rtvec` is the reset vector. `reset_flag_r` resets to
// true (mirroring the reference `sirv_gnrl_dffrs`, a DFF that *sets* on
// reset), which arms `reset_req_r`; that both holds fetch off while reset is
// asserted and selects `pc_rtvec` for the first fetch. Arming costs one cycle
// after release, so `reset()` below ticks once and the first offered fetch is
// the reset vector, not `pc_r + incr`. Test 2 covers this directly.
// Before arch#800 the register reset to false, so both effects were dead and
// the core booted from 0x0 whatever `pc_rtvec` was driven to.

#include "Ve203_ifu_ifetch.h"
#include <cstdio>
#include <cstdint>

static int fail_count = 0;

#define CHECK(cond, fmt, ...) do { \
    if (!(cond)) { \
        printf("  FAIL: " fmt "\n", ##__VA_ARGS__); \
        fail_count++; \
    } \
} while(0)

static Ve203_ifu_ifetch* dut;

// Reset vector used by `reset()`. Zero keeps the sequential-fetch tests below
// on the same PCs they used before the reset vector worked; Test 2 drives a
// non-zero vector explicitly to prove it is honoured.
static const uint32_t BOOT_PC = 0x00000000u;

// RV32 `add x3, x1, x2` — rs1=1, rs2=2, bits[1:0]=11 so minidec sees rv32.
static const uint32_t RV32_ADD_X3_X1_X2 = 0x002081B3u;
// RVC `c.li x10, 0` — bits[1:0]=01, so minidec sees a 16-bit instruction.
static const uint32_t RVC_LI_X10_0 = 0x00004501u;

static void tick() {
    dut->clk = 0; dut->eval();
    dut->clk = 1; dut->eval();
}

static void reset() {
    dut->rst_n = 0;
    dut->pc_rtvec = BOOT_PC;
    dut->ifu_req_ready = 1;
    dut->ifu_rsp_valid = 0;
    dut->ifu_rsp_err = 0;
    dut->ifu_rsp_instr = 0;
    dut->ifu_o_ready = 1;
    dut->pipe_flush_req = 0;
    dut->pipe_flush_add_op1 = 0;
    dut->pipe_flush_add_op2 = 0;
    dut->pipe_flush_pc = 0;
    dut->ifu_halt_req = 0;
    dut->oitf_empty = 1;
    dut->rf2ifu_x1 = 0;
    dut->rf2ifu_rs1 = 0;
    dut->dec2ifu_rs1en = 0;
    dut->dec2ifu_rden = 0;
    dut->dec2ifu_rdidx = 0;
    dut->dec2ifu_mulhsu = 0;
    dut->dec2ifu_div = 0;
    dut->dec2ifu_rem = 0;
    dut->dec2ifu_divu = 0;
    dut->dec2ifu_remu = 0;
    for (int i = 0; i < 3; i++) tick();
    dut->rst_n = 1;
    dut->eval();
    // Arming `reset_req_r` costs one cycle after release; the fetch offered
    // after it is the reset vector, so the first request every test sees is
    // BOOT_PC rather than pc_r + incr.
    tick();
    dut->eval();
}

// Complete one fetch: the request is already being offered, so handshake it,
// then hand back `instr`. Leaves the response deasserted and the IR holding
// the delivered instruction.
static void deliver(uint32_t instr, uint8_t err) {
    dut->ifu_rsp_instr = instr;
    dut->eval();
    tick();                       // request handshake (ifu_req_ready is held high)
    dut->ifu_rsp_valid = 1;
    dut->ifu_rsp_err = err;
    dut->eval();
    tick();                       // response handshake -> IR loads
    dut->ifu_rsp_valid = 0;
    dut->ifu_rsp_err = 0;
    dut->eval();
}

int main() {
    dut = new Ve203_ifu_ifetch;

    // ── Test 1: Reset state ──────────────────────────────────────────
    printf("Test 1: Reset state\n");
    reset();
    CHECK(dut->ifu_o_valid == 0, "o_valid should be 0 after reset, got %d", dut->ifu_o_valid);
    CHECK(dut->ifu_o_pc_vld == 0, "o_pc_vld should be 0 after reset, got %d", dut->ifu_o_pc_vld);
    CHECK(dut->inspect_pc == 0, "pc should be 0 after reset, got 0x%08x", dut->inspect_pc);
    CHECK(dut->ifu_halt_ack == 0, "halt_ack should be 0 after reset, got %d", dut->ifu_halt_ack);
    // pipe_flush_ack is an unconditional constant in this design.
    CHECK(dut->pipe_flush_ack == 1, "pipe_flush_ack should be 1, got %d", dut->pipe_flush_ack);
    // A fetch is offered immediately: nothing is outstanding and halt is low.
    CHECK(dut->ifu_req_valid == 1, "req_valid should be 1 after reset, got %d", dut->ifu_req_valid);

    // ── Test 2: Boot fetch targets the reset vector, then PC += 4 ────
    printf("Test 2: Boot fetch at the reset vector, then sequential RV32\n");
    reset();
    dut->ifu_rsp_instr = RV32_ADD_X3_X1_X2;
    dut->eval();
    // The first fetch after reset is the reset vector, so it is *not*
    // sequential — `ifu_req_seq` excludes it via `~ifu_reset_req`. Before
    // arch#800 this fetch did not exist and the unit started at pc_r + incr.
    CHECK(dut->ifu_req_seq == 0, "the boot fetch should be non-sequential, got req_seq %d", dut->ifu_req_seq);
    CHECK(dut->ifu_req_seq_rv32 == 1, "req_seq_rv32 should be 1 for an RV32 instr, got %d", dut->ifu_req_seq_rv32);
    CHECK(dut->ifu_req_pc == BOOT_PC, "boot req_pc should be the reset vector 0x%08x, got 0x%08x",
          BOOT_PC, dut->ifu_req_pc);
    tick();                       // boot request handshake
    dut->eval();
    CHECK(dut->inspect_pc == BOOT_PC, "pc should land on the reset vector 0x%08x, got 0x%08x",
          BOOT_PC, dut->inspect_pc);
    CHECK(dut->ifu_req_seq == 1, "the fetch after boot should be sequential, got req_seq %d", dut->ifu_req_seq);
    CHECK(dut->ifu_req_pc == BOOT_PC + 4, "next req_pc should be 0x%08x (RV32 +4), got 0x%08x",
          BOOT_PC + 4, dut->ifu_req_pc);

    // ── Test 3: Single outstanding request ───────────────────────────
    // After a request handshake the unit must not offer another fetch until
    // the outstanding response has been taken.
    printf("Test 3: Single outstanding request\n");
    CHECK(dut->ifu_req_valid == 0, "req_valid should drop while a fetch is outstanding, got %d", dut->ifu_req_valid);
    for (int i = 0; i < 3; i++) {
        tick(); dut->eval();
        CHECK(dut->ifu_req_valid == 0, "req_valid must stay 0 with no response (cycle %d)", i);
        CHECK(dut->inspect_pc == BOOT_PC, "pc must not advance while stalled, got 0x%08x", dut->inspect_pc);
    }

    // ── Test 4: Response delivers the instruction to decode ──────────
    printf("Test 4: IR delivery to decode\n");
    dut->ifu_rsp_valid = 1;
    dut->eval();
    CHECK(dut->ifu_rsp_ready == 1, "rsp_ready should be 1 when the IR slot is free");
    tick();                       // response handshake -> IR loads
    dut->ifu_rsp_valid = 0;
    dut->eval();
    CHECK(dut->ifu_o_valid == 1, "o_valid should be 1 after a response, got %d", dut->ifu_o_valid);
    CHECK(dut->ifu_o_ir == RV32_ADD_X3_X1_X2, "o_ir should be 0x%08x, got 0x%08x",
          RV32_ADD_X3_X1_X2, dut->ifu_o_ir);
    CHECK(dut->ifu_o_pc == BOOT_PC, "o_pc should be the fetched pc 0x%08x, got 0x%08x", BOOT_PC, dut->ifu_o_pc);
    CHECK(dut->ifu_o_pc_vld == 1, "o_pc_vld should be 1, got %d", dut->ifu_o_pc_vld);
    CHECK(dut->ifu_o_rs1idx == 1, "o_rs1idx should be 1 (add x3,x1,x2), got %d", dut->ifu_o_rs1idx);
    CHECK(dut->ifu_o_rs2idx == 2, "o_rs2idx should be 2 (add x3,x1,x2), got %d", dut->ifu_o_rs2idx);
    CHECK(dut->ifu_o_buserr == 0, "o_buserr should be 0 for a clean fetch, got %d", dut->ifu_o_buserr);
    CHECK(dut->ifu_o_misalgn == 0, "o_misalgn is tied low in this design, got %d", dut->ifu_o_misalgn);

    // ── Test 5: Downstream backpressure holds the IR ─────────────────
    printf("Test 5: Downstream backpressure\n");
    reset();
    deliver(RV32_ADD_X3_X1_X2, 0);
    dut->ifu_o_ready = 0;
    dut->eval();
    CHECK(dut->ifu_o_valid == 1, "o_valid should be held while o_ready is low");
    for (int i = 0; i < 3; i++) {
        tick(); dut->eval();
        CHECK(dut->ifu_o_valid == 1, "o_valid must stay asserted under backpressure (cycle %d)", i);
        CHECK(dut->ifu_o_ir == RV32_ADD_X3_X1_X2, "o_ir must stay stable under backpressure (cycle %d)", i);
    }
    dut->ifu_o_ready = 1;
    dut->eval();
    tick(); dut->eval();
    CHECK(dut->ifu_o_valid == 0, "o_valid should clear once decode accepts, got %d", dut->ifu_o_valid);

    // ── Test 6: RVC instruction advances PC by 2 ─────────────────────
    printf("Test 6: RVC fetch\n");
    reset();
    dut->ifu_rsp_instr = RVC_LI_X10_0;
    dut->eval();
    CHECK(dut->ifu_req_seq_rv32 == 0, "req_seq_rv32 should be 0 for a 16-bit instr, got %d", dut->ifu_req_seq_rv32);
    // The boot fetch goes to the reset vector regardless of instruction width;
    // the 2-byte increment shows on the fetch after it.
    CHECK(dut->ifu_req_pc == BOOT_PC, "boot req_pc should be the reset vector 0x%08x, got 0x%08x",
          BOOT_PC, dut->ifu_req_pc);
    tick(); dut->eval();           // boot request handshake
    CHECK(dut->inspect_pc == BOOT_PC, "pc should land on the reset vector 0x%08x, got 0x%08x",
          BOOT_PC, dut->inspect_pc);
    CHECK(dut->ifu_req_pc == BOOT_PC + 2, "RVC next req_pc should be 0x%08x (pc + 2), got 0x%08x",
          BOOT_PC + 2, dut->ifu_req_pc);

    // ── Test 7: Bus error is captured with the instruction ───────────
    printf("Test 7: Bus error capture\n");
    reset();
    deliver(RV32_ADD_X3_X1_X2, 1);
    CHECK(dut->ifu_o_valid == 1, "o_valid should be 1 after an errored response, got %d", dut->ifu_o_valid);
    CHECK(dut->ifu_o_buserr == 1, "o_buserr should be 1 after ifu_rsp_err, got %d", dut->ifu_o_buserr);

    // ── Test 8: Pipeline flush redirects the PC ──────────────────────
    printf("Test 8: Pipeline flush redirect\n");
    reset();
    deliver(RV32_ADD_X3_X1_X2, 0);
    dut->pipe_flush_req = 1;
    dut->pipe_flush_add_op1 = 0x1234;
    dut->pipe_flush_add_op2 = 0;
    dut->eval();
    CHECK(dut->pipe_flush_ack == 1, "pipe_flush_ack should be 1, got %d", dut->pipe_flush_ack);
    CHECK(dut->ifu_req_pc == 0x1234, "flush should steer req_pc to 0x1234, got 0x%08x", dut->ifu_req_pc);
    CHECK(dut->ifu_req_seq == 0, "req_seq should be 0 on a flush redirect, got %d", dut->ifu_req_seq);
    tick();
    dut->pipe_flush_req = 0;
    dut->eval();
    CHECK(dut->inspect_pc == 0x1234, "pc should be 0x1234 after the flush, got 0x%08x", dut->inspect_pc);

    // op1 + op2 are summed, and bit 0 is forced low (2-byte alignment).
    reset();
    deliver(RV32_ADD_X3_X1_X2, 0);
    dut->pipe_flush_req = 1;
    dut->pipe_flush_add_op1 = 0x2000;
    dut->pipe_flush_add_op2 = 0x11;
    dut->eval();
    CHECK(dut->ifu_req_pc == 0x2010, "flush target should be (0x2000+0x11) & ~1 = 0x2010, got 0x%08x",
          dut->ifu_req_pc);
    dut->pipe_flush_req = 0;

    // ── Test 9: Halt handshake ───────────────────────────────────────
    printf("Test 9: Halt handshake\n");
    reset();
    deliver(RV32_ADD_X3_X1_X2, 0);
    // A fetch is still outstanding here: the response handshake in deliver()
    // frees the slot and issues the next request in the same cycle. Raising
    // halt_req suppresses that reissue, so the outstanding fetch can drain.
    dut->ifu_halt_req = 1;
    dut->eval();
    CHECK(dut->ifu_req_valid == 0, "req_valid should be suppressed while halted, got %d", dut->ifu_req_valid);
    // halt is only acknowledged once nothing is outstanding, which happens on
    // the edge that takes the final response.
    dut->ifu_rsp_valid = 1;
    dut->eval();
    tick();
    dut->ifu_rsp_valid = 0;
    dut->eval();
    CHECK(dut->ifu_halt_ack == 1, "halt_ack should assert once no fetch is outstanding, got %d",
          dut->ifu_halt_ack);
    CHECK(dut->ifu_req_valid == 0, "req_valid should stay 0 while halted, got %d", dut->ifu_req_valid);
    dut->ifu_halt_req = 0;
    dut->eval();
    tick(); dut->eval();
    CHECK(dut->ifu_halt_ack == 0, "halt_ack should clear when halt_req drops, got %d", dut->ifu_halt_ack);

    // ── Test 10: A non-zero reset vector is honoured ─────────────────
    // The regression this file exists to hold: before arch#800 `pc_rtvec` was
    // dead — `reset_flag_r` reset to false, so `reset_req_r` never armed and
    // the boot fetch came from pc_r + incr no matter what the vector said.
    // Driving a vector far from 0 makes that unmistakable.
    printf("Test 10: Non-zero reset vector\n");
    reset();
    dut->pc_rtvec = 0x80000000;
    // Present an RV32 instruction so the post-boot increment is 4; the width
    // comes from whatever minidec sees on ifu_rsp_instr, and 0 decodes as RVC.
    dut->ifu_rsp_instr = RV32_ADD_X3_X1_X2;
    dut->rst_n = 0;
    for (int i = 0; i < 3; i++) tick();
    dut->rst_n = 1;
    dut->eval();
    CHECK(dut->ifu_req_valid == 0, "fetch should be held off until the reset request arms, got %d",
          dut->ifu_req_valid);
    tick();
    dut->eval();
    CHECK(dut->ifu_req_valid == 1, "a boot fetch should be offered once armed, got %d", dut->ifu_req_valid);
    CHECK(dut->ifu_req_pc == 0x80000000u, "boot req_pc should be pc_rtvec 0x80000000, got 0x%08x",
          dut->ifu_req_pc);
    tick();
    dut->eval();
    CHECK(dut->inspect_pc == 0x80000000u, "pc should land on the reset vector, got 0x%08x", dut->inspect_pc);
    CHECK(dut->ifu_req_pc == 0x80000004u, "next req_pc should be 0x80000004, got 0x%08x", dut->ifu_req_pc);

    printf("\n=== e203_ifu_ifetch: %d failure(s) ===\n", fail_count);
    return fail_count ? 1 : 0;
}
