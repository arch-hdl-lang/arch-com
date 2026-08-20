// ARCH sim testbench for e203_ifu_minidec — E203 IFU mini-decoder.
// minidec wraps e203_exu_decode with fetch-time constants (pc=0, no
// prdt/misalgn/buserr) and exposes only the BPU-relevant subset. This tb
// checks that subset over real RV32IMC encodings (assembly noted per test):
// rs1/rs2 enable+index extraction, the six muldiv flags, rv32/bjp/jal/jalr/
// bxx classification, dec_jalr_rs1idx selection, and bjp_imm generation.
// Deep decoder behavior (info bus, illegal detection, every RVC format) is
// covered by e203_exu_decode_tb.cpp; known fixture divergences (SRAI/SRLI
// keying on instr[31], simplified RVC immediates, c.j not flagged dec_jal)
// are documented there and asserted as-implemented here.
//
// NOTE: this replaces a stale tb (VIfuMinidec.h) that targeted an earlier
// revision of this fixture; the fixture was renamed to `e203_ifu_minidec`
// when the e203 corpus was rewritten and the old tb has not compiled since.
//
// Run with:
//   arch sim tests/e203/e203_ifu_minidec.arch tests/e203/e203_exu_decode.arch \
//            --tb tests/e203/e203_ifu_minidec_tb.cpp

#include "Ve203_ifu_minidec.h"
#include <cstdio>
#include <cstdint>

static int fail_count = 0;

#define CHECK(cond, fmt, ...) do { \
    if (!(cond)) { \
        printf("  FAIL: " fmt "\n", ##__VA_ARGS__); \
        fail_count++; \
    } \
} while(0)

static Ve203_ifu_minidec* dut;

static void decode(uint32_t instr) {
    dut->instr = instr;
    dut->eval();
}

int main() {
    dut = new Ve203_ifu_minidec;

    // ── Test 1: Plain ALU op — no branch, both regs read ─────────────
    printf("Test 1: add x3,x1,x2\n");
    decode(0x002081B3);
    CHECK(dut->dec_rv32 == 1, "rv32 should be 1, got %d", dut->dec_rv32);
    CHECK(dut->dec_rs1en == 1 && dut->dec_rs2en == 1, "add reads rs1+rs2, got %d/%d",
          dut->dec_rs1en, dut->dec_rs2en);
    CHECK(dut->dec_rs1idx == 1, "rs1idx should be 1, got %d", dut->dec_rs1idx);
    CHECK(dut->dec_rs2idx == 2, "rs2idx should be 2, got %d", dut->dec_rs2idx);
    CHECK(dut->dec_bjp == 0, "add is not a bjp, got %d", dut->dec_bjp);
    CHECK(dut->dec_mul == 0 && dut->dec_mulhsu == 0 && dut->dec_div == 0 &&
          dut->dec_rem == 0 && dut->dec_divu == 0 && dut->dec_remu == 0,
          "add sets no muldiv flag");

    // ── Test 2: JAL ──────────────────────────────────────────────────
    printf("Test 2: jal x1,+0x100\n");
    decode(0x100000EF);
    CHECK(dut->dec_bjp == 1 && dut->dec_jal == 1, "jal classification, got bjp=%d jal=%d",
          dut->dec_bjp, dut->dec_jal);
    CHECK(dut->dec_jalr == 0 && dut->dec_bxx == 0, "jal is not jalr/bxx, got %d/%d",
          dut->dec_jalr, dut->dec_bxx);
    CHECK(dut->dec_bjp_imm == 0x100, "jal imm should be +0x100, got 0x%08x", dut->dec_bjp_imm);
    CHECK(dut->dec_rs1en == 0, "jal reads no rs1, got %d", dut->dec_rs1en);
    CHECK(dut->dec_jalr_rs1idx == 0, "non-jalr rs1idx should be 0, got %d", dut->dec_jalr_rs1idx);

    printf("Test 2b: jal x0,-4\n");
    decode(0xFFDFF06F);
    CHECK(dut->dec_bjp_imm == 0xFFFFFFFCu, "jal -4 imm should sign-extend, got 0x%08x",
          dut->dec_bjp_imm);

    // ── Test 3: JALR ─────────────────────────────────────────────────
    printf("Test 3: jalr x1,-16(x5)\n");
    decode(0xFF0280E7);
    CHECK(dut->dec_bjp == 1 && dut->dec_jalr == 1, "jalr classification, got bjp=%d jalr=%d",
          dut->dec_bjp, dut->dec_jalr);
    CHECK(dut->dec_jalr_rs1idx == 5, "jalr rs1idx should be 5, got %d", dut->dec_jalr_rs1idx);
    CHECK(dut->dec_bjp_imm == 0xFFFFFFF0u, "jalr imm should be -16, got 0x%08x", dut->dec_bjp_imm);
    CHECK(dut->dec_rs1en == 1 && dut->dec_rs1idx == 5, "jalr reads rs1=x5, got en=%d idx=%d",
          dut->dec_rs1en, dut->dec_rs1idx);

    // ── Test 4: Conditional branches ─────────────────────────────────
    printf("Test 4: beq x1,x2,-8 / bne x3,x4,+16\n");
    decode(0xFE208CE3);
    CHECK(dut->dec_bjp == 1 && dut->dec_bxx == 1, "beq classification, got bjp=%d bxx=%d",
          dut->dec_bjp, dut->dec_bxx);
    CHECK(dut->dec_jal == 0 && dut->dec_jalr == 0, "beq is not jal/jalr, got %d/%d",
          dut->dec_jal, dut->dec_jalr);
    CHECK(dut->dec_bjp_imm == 0xFFFFFFF8u, "beq -8 imm should sign-extend, got 0x%08x",
          dut->dec_bjp_imm);
    CHECK(dut->dec_rs1en == 1 && dut->dec_rs2en == 1, "beq reads rs1+rs2, got %d/%d",
          dut->dec_rs1en, dut->dec_rs2en);
    CHECK(dut->dec_rs1idx == 1 && dut->dec_rs2idx == 2, "beq regs x1/x2, got %d/%d",
          dut->dec_rs1idx, dut->dec_rs2idx);
    decode(0x00419863);
    CHECK(dut->dec_bxx == 1, "bne classification, got %d", dut->dec_bxx);
    CHECK(dut->dec_bjp_imm == 0x10, "bne +16 imm, got 0x%08x", dut->dec_bjp_imm);

    // ── Test 5: M-extension flags ────────────────────────────────────
    printf("Test 5: muldiv flags\n");
    decode(0x027302B3);               // mul x5,x6,x7
    CHECK(dut->dec_mul == 1 && dut->dec_mulhsu == 0, "mul flag, got mul=%d mulhsu=%d",
          dut->dec_mul, dut->dec_mulhsu);
    CHECK(dut->dec_rs1en == 1 && dut->dec_rs2en == 1, "mul reads rs1+rs2, got %d/%d",
          dut->dec_rs1en, dut->dec_rs2en);
    decode(0x023110B3);               // mulh x1,x2,x3
    // dec_mulhsu is the shared "high half" flag: mulh|mulhsu|mulhu.
    CHECK(dut->dec_mulhsu == 1 && dut->dec_mul == 0, "mulh flag, got mulhsu=%d mul=%d",
          dut->dec_mulhsu, dut->dec_mul);
    decode(0x027342B3);               // div x5,x6,x7
    CHECK(dut->dec_div == 1 && dut->dec_divu == 0, "div flag, got div=%d divu=%d",
          dut->dec_div, dut->dec_divu);
    decode(0x027352B3);               // divu x5,x6,x7
    CHECK(dut->dec_divu == 1 && dut->dec_div == 0, "divu flag, got divu=%d div=%d",
          dut->dec_divu, dut->dec_div);
    decode(0x027362B3);               // rem x5,x6,x7
    CHECK(dut->dec_rem == 1 && dut->dec_remu == 0, "rem flag, got rem=%d remu=%d",
          dut->dec_rem, dut->dec_remu);
    decode(0x027372B3);               // remu x5,x6,x7
    CHECK(dut->dec_remu == 1 && dut->dec_rem == 0, "remu flag, got remu=%d rem=%d",
          dut->dec_remu, dut->dec_rem);

    // ── Test 6: Compressed branches/jumps ────────────────────────────
    printf("Test 6: RVC bjp subset\n");
    decode(0xC401);                   // c.beqz x8, . (fixture imm = 2)
    CHECK(dut->dec_rv32 == 0, "c.beqz is 16-bit, got rv32=%d", dut->dec_rv32);
    CHECK(dut->dec_bjp == 1 && dut->dec_bxx == 1, "c.beqz classification, got bjp=%d bxx=%d",
          dut->dec_bjp, dut->dec_bxx);
    CHECK(dut->dec_rs1en == 1 && dut->dec_rs1idx == 8, "c.beqz reads x8, got en=%d idx=%d",
          dut->dec_rs1en, dut->dec_rs1idx);
    // Simplified fixture RVC immediate (see decode tb header): this encoding -> 2.
    CHECK(dut->dec_bjp_imm == 2, "c.beqz fixture imm should be 2, got 0x%08x", dut->dec_bjp_imm);

    decode(0xA001);                   // c.j .
    CHECK(dut->dec_bjp == 1, "c.j is a bjp, got %d", dut->dec_bjp);
    // Fixture quirk (pinned in decode tb): c.j does NOT set dec_jal.
    CHECK(dut->dec_jal == 0, "fixture: c.j does not flag dec_jal, got %d", dut->dec_jal);

    decode(0x2001);                   // c.jal .
    CHECK(dut->dec_jal == 1 && dut->dec_rv32 == 0, "c.jal flags dec_jal, got jal=%d rv32=%d",
          dut->dec_jal, dut->dec_rv32);

    decode(0x9282);                   // c.jalr x5
    CHECK(dut->dec_jalr == 1, "c.jalr flags dec_jalr, got %d", dut->dec_jalr);
    CHECK(dut->dec_jalr_rs1idx == 5, "c.jalr rs1idx should be 5, got %d", dut->dec_jalr_rs1idx);

    decode(0x8082);                   // c.jr x1 (ret)
    CHECK(dut->dec_jalr == 1, "c.jr flags dec_jalr, got %d", dut->dec_jalr);
    CHECK(dut->dec_rs1idx == 1, "c.jr reads x1, got %d", dut->dec_rs1idx);
    // Fixture quirk: dec_jalr_rs1idx only covers rv32 jalr and c.jalr, so
    // c.jr reports 0 here (asserted as-implemented, consistent with the
    // decode tb's coverage of rs1_idx instead).
    CHECK(dut->dec_jalr_rs1idx == 0, "fixture: c.jr jalr_rs1idx is 0, got %d",
          dut->dec_jalr_rs1idx);

    // ── Test 7: Non-branch RVC op routes register fields ─────────────
    printf("Test 7: c.add x10,x11\n");
    decode(0x952E);                   // c.add x10,x11 (rd=10, rs2=11, bit12=1)
    CHECK(dut->dec_rv32 == 0 && dut->dec_bjp == 0, "c.add is 16-bit non-bjp, got rv32=%d bjp=%d",
          dut->dec_rv32, dut->dec_bjp);
    CHECK(dut->dec_rs1en == 1 && dut->dec_rs2en == 1, "c.add reads rs1+rs2, got %d/%d",
          dut->dec_rs1en, dut->dec_rs2en);
    CHECK(dut->dec_rs1idx == 10, "c.add rs1 (=rd) should be 10, got %d", dut->dec_rs1idx);
    CHECK(dut->dec_rs2idx == 11, "c.add rs2 should be 11, got %d", dut->dec_rs2idx);

    printf("\n=== e203_ifu_minidec: %d failure(s) ===\n", fail_count);
    return fail_count ? 1 : 0;
}
