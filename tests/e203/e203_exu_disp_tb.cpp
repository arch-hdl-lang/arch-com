// ARCH sim testbench for e203_exu_disp — E203 dispatch unit.
// Tests: clean dispatch pass-through (operands, info, imm, pc, faults, itag),
// x0-source zeroing, OITF RAW/WAW hazard stalls, CSR and fence/fencei
// serialization against a non-empty OITF, long-pipe (AGU-group) gating on
// OITF readiness plus OITF allocation strobes and payload, WFI halt request
// blocking dispatch and the halt-ack condition, and downstream ALU
// backpressure.
//
// NOTE: this replaces a stale tb (VExuDisp.h) that targeted an earlier
// revision of the fixture before the e203 fixtures were rewritten against the
// real E203 RTL, which renamed the construct to `e203_exu_disp`. The old tb
// has not compiled since. Ported to the current class name (Ve203_exu_disp).
//
// The module is purely combinational: drive inputs, eval(), check.
//
// Run with:
//   arch sim tests/e203/e203_exu_disp.arch --tb tests/e203/e203_exu_disp_tb.cpp

#include "Ve203_exu_disp.h"
#include <cstdio>
#include <cstdint>

static int fail_count = 0;

#define CHECK(cond, fmt, ...) do { \
    if (!(cond)) { \
        printf("  FAIL: " fmt "\n", ##__VA_ARGS__); \
        fail_count++; \
    } \
} while(0)

static Ve203_exu_disp* dut;

// disp_i_info group codes (bits [2:0], matching e203_exu_decode)
enum : uint32_t { GRP_ALU = 0, GRP_AGU = 1, GRP_BJP = 2, GRP_CSR = 3, GRP_MDV = 4 };

static void clear_inputs() {
    dut->clk = 0;
    dut->rst_n = 1;
    dut->wfi_halt_exu_req = 0;
    dut->oitf_empty = 1;
    dut->amo_wait = 0;
    dut->disp_i_valid = 0;
    dut->disp_i_rs1x0 = 0; dut->disp_i_rs2x0 = 0;
    dut->disp_i_rs1en = 0; dut->disp_i_rs2en = 0;
    dut->disp_i_rs1idx = 0; dut->disp_i_rs2idx = 0;
    dut->disp_i_rs1 = 0; dut->disp_i_rs2 = 0;
    dut->disp_i_rdwen = 0; dut->disp_i_rdidx = 0;
    dut->disp_i_info = 0; dut->disp_i_imm = 0; dut->disp_i_pc = 0;
    dut->disp_i_misalgn = 0; dut->disp_i_buserr = 0; dut->disp_i_ilegl = 0;
    dut->disp_o_alu_ready = 1;
    dut->disp_o_alu_longpipe = 0;
    dut->oitfrd_match_disprs1 = 0; dut->oitfrd_match_disprs2 = 0;
    dut->oitfrd_match_disprs3 = 0; dut->oitfrd_match_disprd = 0;
    dut->disp_oitf_ptr = 0;
    dut->disp_oitf_ready = 1;
    dut->eval();
}

// Present a valid dispatch beat of the given info group.
static void present(uint32_t grp, uint32_t extra_info = 0) {
    dut->disp_i_valid = 1;
    dut->disp_i_info = grp | extra_info | 0x8;   // rv32 flag bit 3
    dut->eval();
}

int main() {
    dut = new Ve203_exu_disp;

    // ── Test 1: Clean pass-through dispatch ──────────────────────────
    printf("Test 1: Clean dispatch\n");
    clear_inputs();
    dut->disp_i_rs1 = 0x1111; dut->disp_i_rs2 = 0x2222;
    dut->disp_i_rs1en = 1; dut->disp_i_rs2en = 1;
    dut->disp_i_rs1idx = 5; dut->disp_i_rs2idx = 6;
    dut->disp_i_rdwen = 1; dut->disp_i_rdidx = 7;
    dut->disp_i_imm = 0x1234; dut->disp_i_pc = 0x8000;
    present(GRP_ALU, 0x10);
    CHECK(dut->disp_o_alu_valid == 1, "ALU op should dispatch, got %d", dut->disp_o_alu_valid);
    CHECK(dut->disp_i_ready == 1, "decode side should see ready, got %d", dut->disp_i_ready);
    CHECK(dut->disp_o_alu_rs1 == 0x1111, "rs1 should pass through, got 0x%08x", dut->disp_o_alu_rs1);
    CHECK(dut->disp_o_alu_rs2 == 0x2222, "rs2 should pass through, got 0x%08x", dut->disp_o_alu_rs2);
    CHECK(dut->disp_o_alu_rdwen == 1 && dut->disp_o_alu_rdidx == 7, "rd should pass through, got %d/%d",
          dut->disp_o_alu_rdwen, dut->disp_o_alu_rdidx);
    CHECK(dut->disp_o_alu_info == (GRP_ALU | 0x10 | 0x8), "info should pass through, got 0x%08x",
          dut->disp_o_alu_info);
    CHECK(dut->disp_o_alu_imm == 0x1234, "imm should pass through, got 0x%08x", dut->disp_o_alu_imm);
    CHECK(dut->disp_o_alu_pc == 0x8000, "pc should pass through, got 0x%08x", dut->disp_o_alu_pc);
    CHECK(dut->disp_oitf_ena == 0, "short op must not allocate an OITF entry, got %d", dut->disp_oitf_ena);
    // Fault flags pass through.
    dut->disp_i_misalgn = 1; dut->disp_i_buserr = 1; dut->disp_i_ilegl = 1;
    dut->eval();
    CHECK(dut->disp_o_alu_misalgn == 1 && dut->disp_o_alu_buserr == 1 && dut->disp_o_alu_ilegl == 1,
          "faults should pass through, got %d/%d/%d",
          dut->disp_o_alu_misalgn, dut->disp_o_alu_buserr, dut->disp_o_alu_ilegl);
    // itag mirrors the OITF write pointer.
    dut->disp_oitf_ptr = 1;
    dut->eval();
    CHECK(dut->disp_o_alu_itag == 1, "itag should mirror the OITF ptr, got %d", dut->disp_o_alu_itag);

    // ── Test 2: x0 sources read as zero ──────────────────────────────
    printf("Test 2: x0 zeroing\n");
    clear_inputs();
    dut->disp_i_rs1 = 0xDEAD; dut->disp_i_rs2 = 0xBEEF;
    dut->disp_i_rs1x0 = 1;
    present(GRP_ALU);
    CHECK(dut->disp_o_alu_rs1 == 0, "x0 rs1 must dispatch as 0, got 0x%08x", dut->disp_o_alu_rs1);
    CHECK(dut->disp_o_alu_rs2 == 0xBEEF, "rs2 unaffected, got 0x%08x", dut->disp_o_alu_rs2);
    dut->disp_i_rs2x0 = 1;
    dut->eval();
    CHECK(dut->disp_o_alu_rs2 == 0, "x0 rs2 must dispatch as 0, got 0x%08x", dut->disp_o_alu_rs2);

    // ── Test 3: OITF hazard stalls ───────────────────────────────────
    printf("Test 3: Hazard stalls\n");
    clear_inputs();
    present(GRP_ALU);
    CHECK(dut->disp_o_alu_valid == 1, "baseline dispatches, got %d", dut->disp_o_alu_valid);
    // RAW on each rs channel.
    dut->oitfrd_match_disprs1 = 1;
    dut->eval();
    CHECK(dut->disp_o_alu_valid == 0, "rs1 RAW must stall, got %d", dut->disp_o_alu_valid);
    CHECK(dut->disp_i_ready == 0, "decode sees not-ready during stall, got %d", dut->disp_i_ready);
    dut->oitfrd_match_disprs1 = 0;
    dut->oitfrd_match_disprs2 = 1;
    dut->eval();
    CHECK(dut->disp_o_alu_valid == 0, "rs2 RAW must stall, got %d", dut->disp_o_alu_valid);
    dut->oitfrd_match_disprs2 = 0;
    dut->oitfrd_match_disprs3 = 1;
    dut->eval();
    CHECK(dut->disp_o_alu_valid == 0, "rs3 RAW must stall, got %d", dut->disp_o_alu_valid);
    dut->oitfrd_match_disprs3 = 0;
    // WAW.
    dut->oitfrd_match_disprd = 1;
    dut->eval();
    CHECK(dut->disp_o_alu_valid == 0, "WAW must stall, got %d", dut->disp_o_alu_valid);
    dut->oitfrd_match_disprd = 0;
    dut->eval();
    CHECK(dut->disp_o_alu_valid == 1, "dispatch resumes when the hazard clears, got %d",
          dut->disp_o_alu_valid);

    // ── Test 4: CSR / fence serialization against the OITF ───────────
    printf("Test 4: CSR/fence serialization\n");
    clear_inputs();
    dut->oitf_empty = 0;                      // long-pipe op in flight
    present(GRP_CSR, 0x10);
    CHECK(dut->disp_o_alu_valid == 0, "CSR must wait for OITF empty, got %d", dut->disp_o_alu_valid);
    dut->oitf_empty = 1;
    dut->eval();
    CHECK(dut->disp_o_alu_valid == 1, "CSR dispatches once OITF drains, got %d", dut->disp_o_alu_valid);
    // fence (BJP group, bit 14) and fencei (bit 15) serialize the same way.
    dut->oitf_empty = 0;
    present(GRP_BJP, 1u << 14);
    CHECK(dut->disp_o_alu_valid == 0, "fence must wait for OITF empty, got %d", dut->disp_o_alu_valid);
    present(GRP_BJP, 1u << 15);
    CHECK(dut->disp_o_alu_valid == 0, "fencei must wait for OITF empty, got %d", dut->disp_o_alu_valid);
    // A plain branch (BJP group, no fence bits) is NOT serialized.
    present(GRP_BJP, 0x40 | 0x1000);
    CHECK(dut->disp_o_alu_valid == 1, "plain branch need not wait for the OITF, got %d",
          dut->disp_o_alu_valid);

    // ── Test 5: Long-pipe gating and OITF allocation ─────────────────
    printf("Test 5: Long-pipe / OITF allocation\n");
    clear_inputs();
    dut->disp_o_alu_longpipe = 1;
    dut->disp_i_rs1en = 1; dut->disp_i_rs2en = 1;
    dut->disp_i_rs1idx = 3; dut->disp_i_rs2idx = 4;
    dut->disp_i_rdwen = 1; dut->disp_i_rdidx = 9;
    dut->disp_i_pc = 0x9000;
    present(GRP_AGU, 0x10);                   // load
    CHECK(dut->disp_o_alu_valid == 1, "AGU op dispatches with OITF ready, got %d", dut->disp_o_alu_valid);
    CHECK(dut->disp_oitf_ena == 1, "long-pipe dispatch allocates an OITF entry, got %d",
          dut->disp_oitf_ena);
    CHECK(dut->disp_oitf_rs1idx == 3 && dut->disp_oitf_rs2idx == 4 && dut->disp_oitf_rdidx == 9,
          "OITF payload indices, got %d/%d/%d",
          dut->disp_oitf_rs1idx, dut->disp_oitf_rs2idx, dut->disp_oitf_rdidx);
    CHECK(dut->disp_oitf_rs1en == 1 && dut->disp_oitf_rs2en == 1 && dut->disp_oitf_rdwen == 1,
          "OITF payload enables, got %d/%d/%d",
          dut->disp_oitf_rs1en, dut->disp_oitf_rs2en, dut->disp_oitf_rdwen);
    CHECK(dut->disp_oitf_pc == 0x9000, "OITF pc, got 0x%08x", dut->disp_oitf_pc);
    CHECK(dut->disp_oitf_rs3en == 0 && dut->disp_oitf_rdfpu == 0,
          "no rs3/FPU in E203, got %d/%d", dut->disp_oitf_rs3en, dut->disp_oitf_rdfpu);
    // AGU-group op stalls when the OITF is full.
    dut->disp_oitf_ready = 0;
    dut->eval();
    CHECK(dut->disp_o_alu_valid == 0, "AGU op must wait for an OITF slot, got %d", dut->disp_o_alu_valid);
    CHECK(dut->disp_oitf_ena == 0, "no allocation while stalled, got %d", dut->disp_oitf_ena);
    dut->disp_oitf_ready = 1;
    dut->eval();
    // A short op (ALU group, longpipe low) ignores OITF fullness.
    dut->disp_o_alu_longpipe = 0;
    dut->disp_oitf_ready = 0;
    present(GRP_ALU);
    CHECK(dut->disp_o_alu_valid == 1, "short op ignores OITF fullness, got %d", dut->disp_o_alu_valid);
    CHECK(dut->disp_oitf_ena == 0, "short op does not allocate, got %d", dut->disp_oitf_ena);

    // ── Test 6: WFI halt handshake ───────────────────────────────────
    printf("Test 6: WFI halt\n");
    clear_inputs();
    present(GRP_ALU);
    dut->wfi_halt_exu_req = 1;
    dut->eval();
    CHECK(dut->disp_o_alu_valid == 0, "halt request must block dispatch, got %d", dut->disp_o_alu_valid);
    CHECK(dut->wfi_halt_exu_ack == 1, "ack asserts with OITF empty and no AMO, got %d",
          dut->wfi_halt_exu_ack);
    dut->oitf_empty = 0;
    dut->eval();
    CHECK(dut->wfi_halt_exu_ack == 0, "ack must wait for the OITF to drain, got %d",
          dut->wfi_halt_exu_ack);
    dut->oitf_empty = 1;
    dut->amo_wait = 1;
    dut->eval();
    CHECK(dut->wfi_halt_exu_ack == 0, "ack must wait out an AMO, got %d", dut->wfi_halt_exu_ack);
    dut->amo_wait = 0;
    dut->wfi_halt_exu_req = 0;
    dut->eval();
    CHECK(dut->disp_o_alu_valid == 1, "dispatch resumes after the halt clears, got %d",
          dut->disp_o_alu_valid);

    // ── Test 7: Downstream backpressure ──────────────────────────────
    printf("Test 7: ALU backpressure\n");
    clear_inputs();
    present(GRP_ALU);
    dut->disp_o_alu_ready = 0;
    dut->eval();
    CHECK(dut->disp_o_alu_valid == 1, "valid stays offered under backpressure, got %d",
          dut->disp_o_alu_valid);
    CHECK(dut->disp_i_ready == 0, "decode stalls while the ALU is busy, got %d", dut->disp_i_ready);
    dut->disp_o_alu_ready = 1;
    dut->eval();
    CHECK(dut->disp_i_ready == 1, "ready recovers, got %d", dut->disp_i_ready);

    printf("\n=== e203_exu_disp: %d failure(s) ===\n", fail_count);
    return fail_count ? 1 : 0;
}
