// ARCH sim testbench for e203_ifu_top — E203 instruction-fetch unit top.
//
// e203_ifu_top is a pure integration wrapper: it instantiates e203_ifu_ifetch
// (PC generation + fetch state machine, itself wrapping e203_ifu_minidec and
// e203_ifu_litebpu) and e203_ifu_ift2icb (the fetch-to-ICB bridge), and wires
// the two together through the ifu_req_*/ifu_rsp_* wire bundle. So what this tb
// is responsible for is the *wiring*, not the leaf behavior — the leaves have
// their own testbenches (e203_ifu_ifetch_tb.cpp, e203_ifu_minidec_tb.cpp,
// e203_ifu_litebpu_tb.cpp).
//
// Tests: reset state of every top-level output; ITCM-vs-BIU region decode and
// the 16-bit ITCM address truncation; ICB command backpressure freezing the PC;
// an ITCM/BIU ICB response landing in the bridge's response buffer; the
// pipe-flush redirect reaching the ICB command address through both instances;
// and the halt handshake.
//
// NOTE: this replaces a stale tb that predates the PR #843 rewiring of these
// fixtures against the real ICB fabric and has not compiled since.
//
// Run with:
//   arch sim tests/e203/e203_ifu_top.arch tests/e203/e203_ifu_ifetch.arch \
//            tests/e203/e203_ifu_minidec.arch tests/e203/e203_ifu_litebpu.arch \
//            tests/e203/e203_ifu_ift2icb.arch tests/e203/e203_exu_decode.arch \
//            --tb tests/e203/e203_ifu_top_tb.cpp
//
// ── KNOWN ISSUE 1: the fetch pipeline deadlocks on the first ICB response, so
// no instruction ever reaches the ifu_o_* channel. e203_ifu_ift2icb computes
//     ifu_req_ready = (…icb_cmd_ready) & ~buf_full     (buf_full == rsp_valid_r)
// while e203_ifu_ifetch computes (matching the reference RTL exactly)
//     ifu_rsp2ir_ready = pipe_flush_req_real ? 1 : (ifu_ir_i_ready & ifu_req_ready & ~bpu_wait)
// and drives ifu_rsp_ready from it. Once the bridge's 1-entry response buffer
// fills, ifu_req_ready drops, which drops ifu_rsp_ready, which prevents
// rsp_drain from ever clearing the buffer — a permanent lock, independent of
// stimulus. The bridge's `~buf_full` term was introduced to break the
// same-cycle algebraic loop of arch#781 (see the long comment in
// e203_ifu_ift2icb.arch); the real design instead breaks that loop with a
// sirv_gnrl_bypbuf whose request-ready is not gated on buffer occupancy. So
// the tests below verify the fetch path up to and including the response
// buffer, and Test 7 pins the deadlock as observed behavior rather than
// pretending an instruction gets delivered. Reported separately.
//
// ── KNOWN ISSUE 2: the ITCM 64→32 response half-select is off by one request.
// e203_ifu_ift2icb selects with `fetch_pc[2]`, but fetch_pc is the address of
// the *next* request, not of the outstanding one whose data is arriving. Test 5
// demonstrates it: the request goes out for 0x0002 (word 0 ⇒ rdata[31:0]) and
// the buffer captures rdata[63:32]. Asserted as-is, with this note.
//
// ── KNOWN ISSUE 3: e203_ifu_top ties ifetch's `ifu_rsp_err <- false`, so an
// ICB bus error captured by the bridge (ifu_rsp_err_w) is dropped at the
// instance boundary and can never reach ifu_o_buserr. Not separately checkable
// while KNOWN ISSUE 1 holds ifu_o_valid low; noted here so it is not
// rediscovered.
//
// ── KNOWN ISSUE 4 (pre-existing, arch#800): the pc_rtvec reset vector is dead
// in e203_ifu_ifetch, so the fetch unit boots from 0x0 rather than pc_rtvec.
// Every PC expectation below is written against the 0x0 boot address.
//
// Two further top-level inputs, itcm_nohold and ifu2itcm_holdup, are declared
// on e203_ifu_ift2icb but never read in its body; they are driven to 0 here and
// no check depends on them.

#include "Ve203_ifu_top.h"
#include <cstdio>
#include <cstdint>

static int fail_count = 0;

#define CHECK(cond, fmt, ...) do { \
    if (!(cond)) { \
        printf("  FAIL: " fmt "\n", ##__VA_ARGS__); \
        fail_count++; \
    } \
} while(0)

static Ve203_ifu_top* dut;

// RV32 `add x3, x1, x2` — bits[1:0]=11, so minidec sees a 32-bit instruction.
static const uint32_t RV32_ADD_X3_X1_X2 = 0x002081B3u;

// The sim emitter runs a fixed two comb passes per eval(), so a value that has
// to cross ifetch -> ift2icb -> back can be one pass stale in a hierarchy this
// deep. Settle explicitly before sampling combinational outputs.
static void settle() { dut->eval(); dut->eval(); dut->eval(); }

static void tick() {
    dut->clk = 0; settle();
    dut->clk = 1; settle();
}

// Drive every input to a defined value and hold reset for 3 ticks.
// `indic` selects which region the ITCM claims (compared on bits [31:16]).
static void reset(uint32_t indic) {
    dut->rst_n = 0;
    dut->itcm_nohold = 0;
    dut->pc_rtvec = 0x80000000;
    dut->ifu2itcm_holdup = 0;
    dut->itcm_region_indic = indic;
    dut->ifu2itcm_icb_cmd_ready = 1;
    dut->ifu2itcm_icb_rsp_valid = 0;
    dut->ifu2itcm_icb_rsp_err = 0;
    dut->ifu2itcm_icb_rsp_rdata = 0;
    dut->ifu2biu_icb_cmd_ready = 1;
    dut->ifu2biu_icb_rsp_valid = 0;
    dut->ifu2biu_icb_rsp_err = 0;
    dut->ifu2biu_icb_rsp_rdata = 0;
    dut->ifu_o_ready = 1;
    dut->pipe_flush_req = 0;
    dut->pipe_flush_add_op1 = 0;
    dut->pipe_flush_add_op2 = 0;
    dut->pipe_flush_pc = 0;
    dut->ifu_halt_req = 0;
    dut->oitf_empty = 1;
    dut->rf2ifu_x1 = 0;
    dut->rf2ifu_rs1 = 0;
    dut->dec2ifu_rden = 0;
    dut->dec2ifu_rs1en = 0;
    dut->dec2ifu_rdidx = 0;
    dut->dec2ifu_mulhsu = 0;
    dut->dec2ifu_div = 0;
    dut->dec2ifu_rem = 0;
    dut->dec2ifu_divu = 0;
    dut->dec2ifu_remu = 0;
    dut->clk = 0;
    for (int i = 0; i < 3; i++) tick();
    dut->rst_n = 1;
    settle();
}

int main() {
    dut = new Ve203_ifu_top;

    // ── Test 1: Reset state ──────────────────────────────────────────
    printf("Test 1: Reset state\n");
    reset(0x00000000);
    CHECK(dut->ifu_active == 1, "ifu_active is hardwired true, got %d", dut->ifu_active);
    CHECK(dut->inspect_pc == 0x0, "pc should be 0 after reset, got 0x%08x", dut->inspect_pc);
    CHECK(dut->ifu_o_valid == 0, "ifu_o_valid should be 0 after reset, got %d", dut->ifu_o_valid);
    CHECK(dut->ifu_o_pc_vld == 0, "ifu_o_pc_vld should be 0 after reset, got %d", dut->ifu_o_pc_vld);
    CHECK(dut->ifu_o_buserr == 0, "ifu_o_buserr should be 0 after reset, got %d", dut->ifu_o_buserr);
    CHECK(dut->ifu_halt_ack == 0, "ifu_halt_ack should be 0 after reset, got %d", dut->ifu_halt_ack);
    // pipe_flush_ack is an unconditional constant inside e203_ifu_ifetch.
    CHECK(dut->pipe_flush_ack == 1, "pipe_flush_ack should be 1, got %d", dut->pipe_flush_ack);
    // Both ICB response channels accept immediately: the bridge buffer is empty.
    CHECK(dut->ifu2itcm_icb_rsp_ready == 1, "itcm rsp_ready should be 1 with an empty buffer, got %d",
          dut->ifu2itcm_icb_rsp_ready);
    CHECK(dut->ifu2biu_icb_rsp_ready == 1, "biu rsp_ready should be 1 with an empty buffer, got %d",
          dut->ifu2biu_icb_rsp_ready);

    // ── Test 2: Fetch request reaches the ITCM ICB command channel ───
    // ifetch offers a fetch as soon as reset drops; ift2icb must route it and
    // present the address. rsp_instr is 0 at this point, so minidec reads a
    // 16-bit encoding and the sequential PC step is +2.
    printf("Test 2: ITCM region decode + command issue\n");
    CHECK(dut->ifu2itcm_icb_cmd_valid == 1, "itcm cmd_valid should be 1 for a pc in the ITCM region, got %d",
          dut->ifu2itcm_icb_cmd_valid);
    CHECK(dut->ifu2biu_icb_cmd_valid == 0, "biu cmd_valid should be 0 for a pc in the ITCM region, got %d",
          dut->ifu2biu_icb_cmd_valid);
    CHECK(dut->ifu2itcm_icb_cmd_addr == 0x0002, "itcm cmd_addr should be 0x0002, got 0x%04x",
          dut->ifu2itcm_icb_cmd_addr);

    // ── Test 3: BIU region decode ────────────────────────────────────
    // With the ITCM claiming 0x8000_xxxx, a pc of 0x0000_0002 falls outside it
    // and the same request must come out of the BIU port instead, un-truncated.
    printf("Test 3: BIU region decode\n");
    reset(0x80000000);
    CHECK(dut->ifu2biu_icb_cmd_valid == 1, "biu cmd_valid should be 1 for a pc outside the ITCM region, got %d",
          dut->ifu2biu_icb_cmd_valid);
    CHECK(dut->ifu2itcm_icb_cmd_valid == 0, "itcm cmd_valid should be 0 for a pc outside the ITCM region, got %d",
          dut->ifu2itcm_icb_cmd_valid);
    CHECK(dut->ifu2biu_icb_cmd_addr == 0x00000002, "biu cmd_addr should be the full 32-bit pc 0x00000002, got 0x%08x",
          dut->ifu2biu_icb_cmd_addr);

    // Redirect the PC into the ITCM region and the routing must flip back, with
    // the ITCM port seeing only the low 16 bits.
    dut->pipe_flush_req = 1;
    dut->pipe_flush_add_op1 = 0x80001234;
    dut->pipe_flush_add_op2 = 0;
    settle();
    CHECK(dut->ifu2itcm_icb_cmd_valid == 1, "routing should flip to ITCM once the pc enters its region, got %d",
          dut->ifu2itcm_icb_cmd_valid);
    CHECK(dut->ifu2biu_icb_cmd_valid == 0, "biu cmd_valid should drop once the pc enters the ITCM region, got %d",
          dut->ifu2biu_icb_cmd_valid);
    CHECK(dut->ifu2itcm_icb_cmd_addr == 0x1234, "itcm cmd_addr should be the truncated 0x1234, got 0x%04x",
          dut->ifu2itcm_icb_cmd_addr);
    dut->pipe_flush_req = 0;

    // ── Test 4: ICB command backpressure freezes the fetch ───────────
    printf("Test 4: ICB command backpressure\n");
    reset(0x00000000);
    dut->ifu2itcm_icb_cmd_ready = 0;
    settle();
    for (int i = 0; i < 3; i++) {
        tick(); settle();
        CHECK(dut->ifu2itcm_icb_cmd_valid == 1, "cmd_valid must stay asserted under backpressure (cycle %d)", i);
        CHECK(dut->ifu2itcm_icb_cmd_addr == 0x0002, "cmd_addr must stay stable under backpressure (cycle %d)", i);
        CHECK(dut->inspect_pc == 0x0, "pc must not advance while the command is stalled (cycle %d), got 0x%08x",
              i, dut->inspect_pc);
    }
    dut->ifu2itcm_icb_cmd_ready = 1;
    settle();
    tick(); settle();
    CHECK(dut->inspect_pc == 0x2, "pc should advance to 0x2 once the command handshakes, got 0x%08x",
          dut->inspect_pc);
    CHECK(dut->ifu2itcm_icb_cmd_valid == 0, "cmd_valid should drop with a fetch outstanding, got %d",
          dut->ifu2itcm_icb_cmd_valid);

    // ── Test 5: ITCM response lands in the bridge response buffer ────
    // The response buffer is the last stage this integration can actually reach
    // (see KNOWN ISSUE 1). ifu2itcm_icb_rsp_ready falling 1 -> 0 is the
    // port-visible proof the response was accepted; _let_ifu_rsp_instr_w is the
    // ifetch<->ift2icb wire carrying the captured instruction.
    printf("Test 5: ITCM response capture\n");
    reset(0x00000000);
    dut->ifu2itcm_icb_rsp_rdata = ((uint64_t)0xAAAAAAAAull << 32) | RV32_ADD_X3_X1_X2;
    settle();
    tick(); settle();                        // command handshake for addr 0x0002
    CHECK(dut->ifu2itcm_icb_rsp_ready == 1, "itcm rsp_ready should still be 1 before the response, got %d",
          dut->ifu2itcm_icb_rsp_ready);
    dut->ifu2itcm_icb_rsp_valid = 1;
    settle();
    tick();
    dut->ifu2itcm_icb_rsp_valid = 0;
    settle();
    CHECK(dut->ifu2itcm_icb_rsp_ready == 0, "itcm rsp_ready should drop once the buffer holds a response, got %d",
          dut->ifu2itcm_icb_rsp_ready);
    CHECK(dut->_let_ifu_rsp_valid_w == 1, "the bridge should present a valid response to ifetch, got %d",
          dut->_let_ifu_rsp_valid_w);
    // KNOWN ISSUE 2: the request was for 0x0002 (word 0), so the correct half is
    // rdata[31:0] == RV32_ADD_X3_X1_X2. The design selects on the *next* fetch
    // address (0x0004, bit 2 set) and captures rdata[63:32] instead.
    CHECK(dut->_let_ifu_rsp_instr_w == 0xAAAAAAAAu,
          "KNOWN ISSUE 2: half-select follows the next fetch_pc, expected 0xAAAAAAAA, got 0x%08x",
          dut->_let_ifu_rsp_instr_w);

    // ── Test 6: The fetch pipeline locks up on that response ─────────
    // KNOWN ISSUE 1. Pinned as observed behavior so a future fix flips this
    // test loudly instead of silently.
    printf("Test 6: Response-buffer lockup (KNOWN ISSUE 1)\n");
    for (int i = 0; i < 8; i++) {
        tick(); settle();
        CHECK(dut->ifu_o_valid == 0, "KNOWN ISSUE 1: ifu_o_valid stays 0 (cycle %d), got %d", i, dut->ifu_o_valid);
        CHECK(dut->ifu2itcm_icb_rsp_ready == 0, "KNOWN ISSUE 1: the buffer never drains (cycle %d)", i);
        CHECK(dut->ifu2itcm_icb_cmd_valid == 0, "KNOWN ISSUE 1: no new command is issued (cycle %d)", i);
        CHECK(dut->inspect_pc == 0x2, "pc stays frozen at 0x2 while locked (cycle %d), got 0x%08x",
              i, dut->inspect_pc);
    }

    // ── Test 7: pipe_flush drains the buffer and redirects the PC ────
    // pipe_flush_req forces ifetch's rsp2ir_ready high, which is the one escape
    // from the lockup above; the buffered response is discarded, not delivered.
    printf("Test 7: pipe_flush drain + redirect\n");
    dut->pipe_flush_req = 1;
    dut->pipe_flush_add_op1 = 0x00001234;
    dut->pipe_flush_add_op2 = 0;
    settle();
    CHECK(dut->pipe_flush_ack == 1, "pipe_flush_ack should be 1, got %d", dut->pipe_flush_ack);
    CHECK(dut->ifu2itcm_icb_cmd_addr == 0x1234, "flush should steer the ICB command to 0x1234, got 0x%04x",
          dut->ifu2itcm_icb_cmd_addr);
    tick();
    dut->pipe_flush_req = 0;
    settle();
    CHECK(dut->inspect_pc == 0x1234, "pc should be 0x1234 after the flush, got 0x%08x", dut->inspect_pc);
    CHECK(dut->ifu2itcm_icb_rsp_ready == 1, "the buffer should have drained on the flush, got rsp_ready %d",
          dut->ifu2itcm_icb_rsp_ready);
    CHECK(dut->ifu_o_valid == 0, "the flushed response must not be delivered to decode, got o_valid %d",
          dut->ifu_o_valid);
    CHECK(dut->ifu2itcm_icb_cmd_valid == 1, "fetching should resume from the redirect target, got %d",
          dut->ifu2itcm_icb_cmd_valid);

    // op1 + op2 are summed and bit 0 is forced low (2-byte alignment).
    reset(0x00000000);
    dut->pipe_flush_req = 1;
    dut->pipe_flush_add_op1 = 0x2000;
    dut->pipe_flush_add_op2 = 0x11;
    settle();
    CHECK(dut->ifu2itcm_icb_cmd_addr == 0x2010, "flush target should be (0x2000+0x11) & ~1 = 0x2010, got 0x%04x",
          dut->ifu2itcm_icb_cmd_addr);
    dut->pipe_flush_req = 0;

    // ── Test 8: BIU response capture ─────────────────────────────────
    // The BIU path is 32 bits wide, so the response is captured verbatim with
    // no half-select in the way.
    printf("Test 8: BIU response capture\n");
    reset(0x80000000);
    dut->ifu2biu_icb_rsp_rdata = 0xDEADBEEFu;
    settle();
    tick(); settle();                        // command handshake on the BIU port
    dut->ifu2biu_icb_rsp_valid = 1;
    settle();
    tick();
    dut->ifu2biu_icb_rsp_valid = 0;
    settle();
    CHECK(dut->ifu2biu_icb_rsp_ready == 0, "biu rsp_ready should drop once the buffer holds a response, got %d",
          dut->ifu2biu_icb_rsp_ready);
    CHECK(dut->_let_ifu_rsp_valid_w == 1, "the bridge should present a valid BIU response, got %d",
          dut->_let_ifu_rsp_valid_w);
    CHECK(dut->_let_ifu_rsp_instr_w == 0xDEADBEEFu, "biu rdata should reach the response buffer verbatim, got 0x%08x",
          dut->_let_ifu_rsp_instr_w);

    // ── Test 9: Halt handshake ───────────────────────────────────────
    printf("Test 9: Halt handshake\n");
    reset(0x00000000);
    CHECK(dut->ifu2itcm_icb_cmd_valid == 1, "a fetch should be offered before halting, got %d",
          dut->ifu2itcm_icb_cmd_valid);
    dut->ifu_halt_req = 1;
    settle();
    CHECK(dut->ifu2itcm_icb_cmd_valid == 0, "halt_req should suppress the ICB command, got %d",
          dut->ifu2itcm_icb_cmd_valid);
    CHECK(dut->ifu2biu_icb_cmd_valid == 0, "halt_req should suppress the BIU command too, got %d",
          dut->ifu2biu_icb_cmd_valid);
    tick(); settle();
    CHECK(dut->ifu_halt_ack == 1, "halt_ack should assert with nothing outstanding, got %d", dut->ifu_halt_ack);
    tick(); settle();
    CHECK(dut->ifu_halt_ack == 1, "halt_ack should stay asserted while halt_req holds, got %d", dut->ifu_halt_ack);
    CHECK(dut->inspect_pc == 0x0, "pc must not advance while halted, got 0x%08x", dut->inspect_pc);
    dut->ifu_halt_req = 0;
    settle();
    tick(); settle();
    CHECK(dut->ifu_halt_ack == 0, "halt_ack should clear when halt_req drops, got %d", dut->ifu_halt_ack);
    CHECK(dut->inspect_pc == 0x2, "fetching should resume after halt, pc should be 0x2, got 0x%08x",
          dut->inspect_pc);

    printf("\n=== e203_ifu_top: %d failure(s) ===\n", fail_count);
    return fail_count ? 1 : 0;
}
