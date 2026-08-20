// ARCH sim testbench for e203_exu_decode — E203 RV32IMC decoder.
// Tests: representative real RV32I/M/C encodings (assembly noted per test)
// across all groups — R/I ALU ops, LUI/AUIPC, loads/stores, branches,
// JAL/JALR, SYSTEM (ecall/ebreak/mret/csr), M-extension, and compressed
// (CI/CR/CS/CB/CJ/CL/CSS formats) — checking register index/enable
// extraction, immediate generation (I/S/B/U/J), the dec_info group+sub-op
// encoding, muldiv flags, bjp classification and bjp_imm, illegal-instruction
// detection (reserved encodings, bad shamt, all-zeros/all-ones), and
// pc/misalgn/buserr/prdt pass-through.
//
// NOTE: this replaces a stale tb (VExuDecode.h) that targeted an earlier
// revision of the fixture before the e203 fixtures were rewritten against the
// real E203 RTL, which renamed the construct to `e203_exu_decode`. The old tb
// (and its e203_exu_decode_vltor_tb.cpp Verilator twin, deleted with this
// rewrite) has not compiled since. Ported to the current class name
// (Ve203_exu_decode).
//
// KNOWN ISSUE (fixture divergence from RV32I, asserted as-implemented):
// - The immediate-shift sub-decode keys on funct7[6] (instr[31]) instead of
//   funct7[5] (instr[30]): `rv32_srai = is_op_imm & f3_101 & funct7[6]==1`
//   and `rv32_srli = ... & funct7[6]==0`. Real SRAI (funct7=0x20, bit30 set,
//   bit31 clear) therefore decodes as SRLI — an arithmetic shift silently
//   becomes a logical shift. The fixture's own SRAI arm is unreachable,
//   because the shamt-legal gate (`funct7[6]==0`) marks every bit31-set
//   shift illegal. Register-form SRA (funct7 compare) is correct. Tests
//   below pin SRAI->SRL-bit and note the spec value.
// - SLLI/SRLI with instr[30] set (reserved encodings per RV32I) are accepted
//   as legal generic op-imm ops instead of being flagged illegal.
//
// Fixture quirks vs the reference E203 decoder (asserted as-implemented,
// noted here so nobody mistakes them for TB intent):
// - mret/dret are classified into the ALU group by `alu_op` (which includes
//   them), so dec_info carries NO mret/dret sub-op bit — the bjp_info bits
//   0x2000/0x4000 are unreachable for them. dec_info for mret is just the
//   rv32 flag (0x8).
// - c.sub/c.xor/c.or/c.and report dec_rs2en = 0 even though they read rs2
//   (rv16_rs2en excludes the CS-ALU subset).
// - c.li reports rs1 = rd (CI format lumping) instead of x0; c.lwsp reports
//   rs1 = rd instead of x2.
// - Compressed loads/stores produce dec_imm = 0 (RV16 offsets are not routed
//   into the general immediate mux).
//
// The module is purely combinational: drive i_instr, eval(), check.
//
// Run with:
//   arch sim tests/e203/e203_exu_decode.arch --tb tests/e203/e203_exu_decode_tb.cpp

#include "Ve203_exu_decode.h"
#include <cstdio>
#include <cstdint>

static int fail_count = 0;

#define CHECK(cond, fmt, ...) do { \
    if (!(cond)) { \
        printf("  FAIL: " fmt "\n", ##__VA_ARGS__); \
        fail_count++; \
    } \
} while(0)

static Ve203_exu_decode* dut;

// dec_info encoding: group in [2:0], rv32 flag = 0x8, sub-ops from bit 4 up.
enum : uint32_t { GRP_ALU = 0, GRP_AGU = 1, GRP_BJP = 2, GRP_CSR = 3, GRP_MDV = 4, RV32 = 0x8 };

static void decode(uint32_t instr) {
    dut->i_instr = instr;
    dut->eval();
}

static void clear_inputs() {
    dut->i_instr = 0x00000013;  // nop (addi x0,x0,0)
    dut->i_pc = 0;
    dut->i_prdt_taken = 0;
    dut->i_misalgn = 0;
    dut->i_buserr = 0;
    dut->i_muldiv_b2b = 0;
    dut->dbg_mode = 0;
    dut->nice_xs_off = 0;
    dut->eval();
}

int main() {
    dut = new Ve203_exu_decode;
    clear_inputs();

    // ── Test 1: R-type ALU ops ───────────────────────────────────────
    printf("Test 1: R-type ALU\n");
    decode(0x002081B3u);          // add x3, x1, x2
    CHECK(dut->dec_rv32 == 1, "add is a 32-bit instr, got %d", dut->dec_rv32);
    CHECK(dut->dec_rs1idx == 1 && dut->dec_rs2idx == 2 && dut->dec_rdidx == 3,
          "add x3,x1,x2 fields, got rs1=%d rs2=%d rd=%d", dut->dec_rs1idx, dut->dec_rs2idx, dut->dec_rdidx);
    CHECK(dut->dec_rs1en == 1 && dut->dec_rs2en == 1 && dut->dec_rdwen == 1,
          "add enables, got rs1en=%d rs2en=%d rdwen=%d", dut->dec_rs1en, dut->dec_rs2en, dut->dec_rdwen);
    CHECK(dut->dec_info == (GRP_ALU | RV32 | 0x10), "add info should be 0x18, got 0x%08x", dut->dec_info);
    CHECK(dut->dec_ilegl == 0, "add is legal, got %d", dut->dec_ilegl);
    CHECK(dut->dec_rs1x0 == 0 && dut->dec_rs2x0 == 0, "no x0 sources, got %d/%d",
          dut->dec_rs1x0, dut->dec_rs2x0);
    decode(0x40628233u);          // sub x4, x5, x6
    CHECK(dut->dec_info == (GRP_ALU | RV32 | 0x20), "sub info should be 0x28, got 0x%08x", dut->dec_info);
    CHECK(dut->dec_rdidx == 4, "sub rd should be 4, got %d", dut->dec_rdidx);

    // ── Test 2: I-type ALU + shifts ──────────────────────────────────
    printf("Test 2: I-type ALU\n");
    decode(0xFFF00293u);          // addi x5, x0, -1
    CHECK(dut->dec_rs1x0 == 1, "addi rs1 is x0, got %d", dut->dec_rs1x0);
    CHECK(dut->dec_imm == 0xFFFFFFFFu, "addi imm should sign-extend to -1, got 0x%08x", dut->dec_imm);
    CHECK(dut->dec_info == (GRP_ALU | RV32 | 0x10 | 0x8000),
          "addi info = ADD|OP2IMM = 0x8018, got 0x%08x", dut->dec_info);
    decode(0x0010D093u);          // srli x1, x1, 1
    CHECK(dut->dec_info == (GRP_ALU | RV32 | 0x100 | 0x8000),
          "srli info = SRL|OP2IMM = 0x8108, got 0x%08x", dut->dec_info);
    // KNOWN ISSUE pin (see header): real SRAI decodes as SRLI because the
    // fixture keys the sra/srl-immediate split on instr[31], not instr[30].
    decode(0x41F4D493u);          // srai x9, x9, 31 (spec: SRA bit 0x200)
    CHECK(dut->dec_info == (GRP_ALU | RV32 | 0x100 | 0x8000),
          "KNOWN ISSUE: fixture decodes srai as SRL (0x8108), got 0x%08x", dut->dec_info);
    CHECK(dut->dec_ilegl == 0, "srai shamt 31 is legal, got %d", dut->dec_ilegl);
    // Bit31-set immediate shift hits the fixture's shamt gate and is illegal.
    decode(0x8010D093u);          // srli/srai with instr[31] set
    CHECK(dut->dec_ilegl == 1, "bit31-set shift-imm must be illegal, got %d", dut->dec_ilegl);
    // KNOWN ISSUE pin: reserved slli encoding (instr[30] set) is accepted as
    // a generic op-imm instead of being illegal.
    decode(0x40109093u);          // slli x1, x1, 1 with instr[30] set (reserved)
    CHECK(dut->dec_ilegl == 0,
          "KNOWN ISSUE: fixture accepts reserved slli (spec: illegal), got %d", dut->dec_ilegl);

    // ── Test 3: LUI / AUIPC ──────────────────────────────────────────
    printf("Test 3: LUI/AUIPC\n");
    decode(0xABCDE3B7u);          // lui x7, 0xABCDE
    CHECK(dut->dec_imm == 0xABCDE000u, "lui imm should be 0xABCDE000, got 0x%08x", dut->dec_imm);
    CHECK(dut->dec_info == (GRP_ALU | RV32 | 0x4000 | 0x8000),
          "lui info = LUI|OP2IMM = 0xC008, got 0x%08x", dut->dec_info);
    CHECK(dut->dec_rdidx == 7 && dut->dec_rdwen == 1, "lui rd, got rd=%d wen=%d",
          dut->dec_rdidx, dut->dec_rdwen);
    decode(0x00001397u);          // auipc x7, 0x1
    CHECK(dut->dec_imm == 0x1000, "auipc imm should be 0x1000, got 0x%08x", dut->dec_imm);
    CHECK(dut->dec_info == (GRP_ALU | RV32 | 0x10 | 0x8000 | 0x10000),
          "auipc info = ADD|OP2IMM|OP1PC = 0x18018, got 0x%08x", dut->dec_info);

    // ── Test 4: Loads / stores ───────────────────────────────────────
    printf("Test 4: Loads/stores\n");
    decode(0x00412303u);          // lw x6, 4(x2)
    CHECK(dut->dec_info == (GRP_AGU | RV32 | 0x10), "lw info = AGU|LOAD = 0x19, got 0x%08x", dut->dec_info);
    CHECK(dut->dec_imm == 4, "lw imm should be 4, got 0x%08x", dut->dec_imm);
    CHECK(dut->dec_rs1idx == 2 && dut->dec_rdidx == 6, "lw fields, got rs1=%d rd=%d",
          dut->dec_rs1idx, dut->dec_rdidx);
    CHECK(dut->dec_rs2en == 0 && dut->dec_rdwen == 1, "lw enables, got rs2en=%d rdwen=%d",
          dut->dec_rs2en, dut->dec_rdwen);
    decode(0x00612423u);          // sw x6, 8(x2)
    CHECK(dut->dec_info == (GRP_AGU | RV32 | 0x20), "sw info = AGU|STORE = 0x29, got 0x%08x", dut->dec_info);
    CHECK(dut->dec_imm == 8, "sw S-type imm should be 8, got 0x%08x", dut->dec_imm);
    CHECK(dut->dec_rs2en == 1 && dut->dec_rdwen == 0, "sw enables, got rs2en=%d rdwen=%d",
          dut->dec_rs2en, dut->dec_rdwen);
    decode(0xFE612E23u);          // sw x6, -4(x2)
    CHECK(dut->dec_imm == 0xFFFFFFFCu, "sw negative imm should sign-extend, got 0x%08x", dut->dec_imm);

    // ── Test 5: Branches ─────────────────────────────────────────────
    printf("Test 5: Branches\n");
    decode(0x00208463u);          // beq x1, x2, +8
    CHECK(dut->dec_bjp == 1 && dut->dec_bxx == 1, "beq classification, got bjp=%d bxx=%d",
          dut->dec_bjp, dut->dec_bxx);
    CHECK(dut->dec_info == (GRP_BJP | RV32 | 0x40 | 0x1000),
          "beq info = BEQ|BXX = 0x104A, got 0x%08x", dut->dec_info);
    CHECK(dut->dec_bjp_imm == 8, "beq target offset should be +8, got 0x%08x", dut->dec_bjp_imm);
    CHECK(dut->dec_rs1en == 1 && dut->dec_rs2en == 1 && dut->dec_rdwen == 0,
          "beq enables, got %d/%d/%d", dut->dec_rs1en, dut->dec_rs2en, dut->dec_rdwen);
    // Prediction bit folds into the info bus.
    dut->i_prdt_taken = 1;
    dut->eval();
    CHECK(dut->dec_info == (GRP_BJP | RV32 | 0x40 | 0x1000 | 0x20),
          "prdt_taken should add bit 0x20, got 0x%08x", dut->dec_info);
    dut->i_prdt_taken = 0;
    decode(0xFE20CEE3u);          // blt x1, x2, -4
    CHECK(dut->dec_bjp_imm == 0xFFFFFFFCu, "blt backward offset -4, got 0x%08x", dut->dec_bjp_imm);
    CHECK(dut->dec_info == (GRP_BJP | RV32 | 0x100 | 0x1000),
          "blt info = BLT|BXX, got 0x%08x", dut->dec_info);

    // ── Test 6: JAL / JALR ───────────────────────────────────────────
    printf("Test 6: JAL/JALR\n");
    decode(0x100000EFu);          // jal x1, +0x100
    CHECK(dut->dec_jal == 1 && dut->dec_bjp == 1, "jal classification, got jal=%d bjp=%d",
          dut->dec_jal, dut->dec_bjp);
    CHECK(dut->dec_info == (GRP_BJP | RV32 | 0x10), "jal info = JUMP = 0x1A, got 0x%08x", dut->dec_info);
    CHECK(dut->dec_bjp_imm == 0x100, "jal offset should be +0x100, got 0x%08x", dut->dec_bjp_imm);
    CHECK(dut->dec_rdidx == 1 && dut->dec_rdwen == 1, "jal writes ra, got rd=%d wen=%d",
          dut->dec_rdidx, dut->dec_rdwen);
    decode(0x00008067u);          // jalr x0, 0(x1) — ret
    CHECK(dut->dec_jalr == 1, "ret is a jalr, got %d", dut->dec_jalr);
    CHECK(dut->dec_jalr_rs1idx == 1, "jalr rs1 index should be 1, got %d", dut->dec_jalr_rs1idx);
    CHECK(dut->dec_rdidx == 0 && dut->dec_rs1en == 1, "ret fields, got rd=%d rs1en=%d",
          dut->dec_rdidx, dut->dec_rs1en);

    // ── Test 7: SYSTEM ───────────────────────────────────────────────
    printf("Test 7: SYSTEM ops\n");
    decode(0x00000073u);          // ecall
    CHECK(dut->dec_info == (GRP_ALU | RV32 | 0x40000), "ecall info bit 0x40000, got 0x%08x", dut->dec_info);
    CHECK(dut->dec_ilegl == 0, "ecall is legal, got %d", dut->dec_ilegl);
    decode(0x00100073u);          // ebreak
    CHECK(dut->dec_info == (GRP_ALU | RV32 | 0x80000), "ebreak info bit 0x80000, got 0x%08x", dut->dec_info);
    decode(0x10500073u);          // wfi
    CHECK(dut->dec_info == (GRP_ALU | RV32 | 0x100000), "wfi info bit 0x100000, got 0x%08x", dut->dec_info);
    decode(0x30200073u);          // mret
    CHECK(dut->dec_ilegl == 0, "mret is legal, got %d", dut->dec_ilegl);
    // Fixture quirk (see header): mret lands in the ALU group with no sub-bit.
    CHECK(dut->dec_info == (GRP_ALU | RV32), "fixture mret info is bare 0x8, got 0x%08x", dut->dec_info);
    decode(0x305312F3u);          // csrrw x5, mtvec, x6
    CHECK(dut->dec_info == (GRP_CSR | RV32 | 0x10), "csrrw info = 0x1B, got 0x%08x", dut->dec_info);
    CHECK(dut->dec_rs1en == 1 && dut->dec_rdwen == 1, "csrrw enables, got %d/%d",
          dut->dec_rs1en, dut->dec_rdwen);
    decode(0x3051E2F3u);          // csrrsi x5, mtvec, 3
    CHECK(dut->dec_info == (GRP_CSR | RV32 | 0x20 | 0x80),
          "csrrsi info = SET|IMM = 0xAB, got 0x%08x", dut->dec_info);

    // ── Test 8: M-extension ──────────────────────────────────────────
    printf("Test 8: M-extension\n");
    decode(0x027302B3u);          // mul x5, x6, x7
    CHECK(dut->dec_info == (GRP_MDV | RV32 | 0x10), "mul info = 0x1C, got 0x%08x", dut->dec_info);
    CHECK(dut->dec_mul == 1 && dut->dec_div == 0, "mul flags, got mul=%d div=%d",
          dut->dec_mul, dut->dec_div);
    decode(0x027312B3u);          // mulh x5, x6, x7
    CHECK(dut->dec_info == (GRP_MDV | RV32 | 0x20), "mulh info = 0x2C, got 0x%08x", dut->dec_info);
    CHECK(dut->dec_mulhsu == 1, "mulh sets the mulh-family flag, got %d", dut->dec_mulhsu);
    decode(0x027342B3u);          // div x5, x6, x7
    CHECK(dut->dec_info == (GRP_MDV | RV32 | 0x100), "div info = 0x10C, got 0x%08x", dut->dec_info);
    CHECK(dut->dec_div == 1, "div flag, got %d", dut->dec_div);
    decode(0x027372B3u);          // remu x5, x6, x7
    CHECK(dut->dec_info == (GRP_MDV | RV32 | 0x800), "remu info = 0x80C, got 0x%08x", dut->dec_info);
    CHECK(dut->dec_remu == 1, "remu flag, got %d", dut->dec_remu);
    // b2b hint folds into the info bus.
    dut->i_muldiv_b2b = 1;
    dut->eval();
    CHECK(dut->dec_info == (GRP_MDV | RV32 | 0x800 | 0x1000), "b2b adds bit 0x1000, got 0x%08x",
          dut->dec_info);
    dut->i_muldiv_b2b = 0;

    // ── Test 9: Compressed instructions ──────────────────────────────
    printf("Test 9: RV32C\n");
    decode(0x0511u);              // c.addi x10, 4
    CHECK(dut->dec_rv32 == 0, "c.addi is 16-bit, got %d", dut->dec_rv32);
    CHECK(dut->dec_rs1idx == 10 && dut->dec_rdidx == 10, "c.addi rs1=rd=10, got %d/%d",
          dut->dec_rs1idx, dut->dec_rdidx);
    CHECK(dut->dec_info == (GRP_ALU | 0x10 | 0x8000), "c.addi info = ADD|OP2IMM (no rv32), got 0x%08x",
          dut->dec_info);
    decode(0x852Eu);              // c.mv x10, x11
    CHECK(dut->dec_rs1idx == 0 && dut->dec_rs2idx == 11 && dut->dec_rdidx == 10,
          "c.mv fields, got rs1=%d rs2=%d rd=%d", dut->dec_rs1idx, dut->dec_rs2idx, dut->dec_rdidx);
    CHECK(dut->dec_rs1x0 == 1, "c.mv rs1 is x0, got %d", dut->dec_rs1x0);
    CHECK(dut->dec_info == (GRP_ALU | 0x10), "c.mv info = ADD, got 0x%08x", dut->dec_info);
    decode(0x952Eu);              // c.add x10, x11
    CHECK(dut->dec_rs1idx == 10 && dut->dec_rs2idx == 11 && dut->dec_rdidx == 10,
          "c.add fields, got rs1=%d rs2=%d rd=%d", dut->dec_rs1idx, dut->dec_rs2idx, dut->dec_rdidx);
    decode(0x8C05u);              // c.sub x8, x9
    CHECK(dut->dec_rs1idx == 8 && dut->dec_rs2idx == 9 && dut->dec_rdidx == 8,
          "c.sub fields, got rs1=%d rs2=%d rd=%d", dut->dec_rs1idx, dut->dec_rs2idx, dut->dec_rdidx);
    CHECK(dut->dec_info == (GRP_ALU | 0x20), "c.sub info = SUB, got 0x%08x", dut->dec_info);
    // Fixture quirk (see header): CS-ALU rs2en is 0.
    CHECK(dut->dec_rs2en == 0, "fixture c.sub rs2en is 0, got %d", dut->dec_rs2en);
    decode(0x4004u);              // c.lw x9, 0(x8)
    CHECK(dut->dec_info == (GRP_AGU | 0x10), "c.lw info = AGU|LOAD (no rv32), got 0x%08x", dut->dec_info);
    CHECK(dut->dec_rs1idx == 8 && dut->dec_rdidx == 9, "c.lw fields, got rs1=%d rd=%d",
          dut->dec_rs1idx, dut->dec_rdidx);
    decode(0xC01Au);              // c.swsp x6, 0(sp)
    CHECK(dut->dec_info == (GRP_AGU | 0x20), "c.swsp info = AGU|STORE, got 0x%08x", dut->dec_info);
    CHECK(dut->dec_rs1idx == 2 && dut->dec_rs2idx == 6, "c.swsp rs1=sp rs2=6, got %d/%d",
          dut->dec_rs1idx, dut->dec_rs2idx);
    decode(0xC401u);              // c.beqz x8, . (offset bits [11:10] -> imm 2)
    CHECK(dut->dec_bxx == 1 && dut->dec_rv32 == 0, "c.beqz classification, got bxx=%d rv32=%d",
          dut->dec_bxx, dut->dec_rv32);
    CHECK(dut->dec_info == (GRP_BJP | 0x40 | 0x1000), "c.beqz info = BEQ|BXX, got 0x%08x", dut->dec_info);
    CHECK(dut->dec_bjp_imm == 2, "c.beqz fixture imm should be 2, got 0x%08x", dut->dec_bjp_imm);
    CHECK(dut->dec_rs1idx == 8 && dut->dec_rdwen == 0, "c.beqz fields, got rs1=%d wen=%d",
          dut->dec_rs1idx, dut->dec_rdwen);
    decode(0xA001u);              // c.j .
    CHECK(dut->dec_bjp == 1 && dut->dec_jal == 0 && dut->dec_rdidx == 0,
          "c.j is a no-link jump, got bjp=%d jal=%d rd=%d", dut->dec_bjp, dut->dec_jal, dut->dec_rdidx);
    CHECK(dut->dec_info == (GRP_BJP | 0x10), "c.j info = JUMP, got 0x%08x", dut->dec_info);
    decode(0x2001u);              // c.jal .
    CHECK(dut->dec_jal == 1 && dut->dec_rdidx == 1, "c.jal links ra, got jal=%d rd=%d",
          dut->dec_jal, dut->dec_rdidx);
    decode(0x8082u);              // c.jr x1 — ret
    CHECK(dut->dec_jalr == 1 && dut->dec_rdidx == 0 && dut->dec_rs1idx == 1,
          "c.jr fields, got jalr=%d rd=%d rs1=%d", dut->dec_jalr, dut->dec_rdidx, dut->dec_rs1idx);
    decode(0x9002u);              // c.ebreak
    CHECK(dut->dec_info == (GRP_ALU | 0x80000), "c.ebreak info bit 0x80000, got 0x%08x", dut->dec_info);
    CHECK(dut->dec_ilegl == 0, "c.ebreak is legal, got %d", dut->dec_ilegl);

    // ── Test 10: Illegal encodings ───────────────────────────────────
    printf("Test 10: Illegal instructions\n");
    decode(0x00000000u);          // all-zeros (canonically illegal)
    CHECK(dut->dec_ilegl == 1, "all-zeros must be illegal, got %d", dut->dec_ilegl);
    decode(0xFFFFFFFFu);          // all-ones
    CHECK(dut->dec_ilegl == 1, "all-ones must be illegal, got %d", dut->dec_ilegl);
    decode(0x0000007Fu);          // opcode 0x7F — reserved
    CHECK(dut->dec_ilegl == 1, "reserved major opcode must be illegal, got %d", dut->dec_ilegl);
    decode(0x00002083u);          // "lw" with funct3=2 is legal — sanity that detection is not stuck
    CHECK(dut->dec_ilegl == 0, "lw x1,0(x0) is legal, got %d", dut->dec_ilegl);

    // ── Test 11: Pass-through signals ────────────────────────────────
    printf("Test 11: Pass-through\n");
    clear_inputs();
    dut->i_pc = 0x80001234u;
    dut->i_misalgn = 1;
    dut->i_buserr = 1;
    dut->eval();
    CHECK(dut->dec_pc == 0x80001234u, "pc should pass through, got 0x%08x", dut->dec_pc);
    CHECK(dut->dec_misalgn == 1, "misalgn should pass through, got %d", dut->dec_misalgn);
    CHECK(dut->dec_buserr == 1, "buserr should pass through, got %d", dut->dec_buserr);
    CHECK(dut->dec_nice == 0 && dut->nice_cmt_off_ilgl_o == 0, "NICE outputs tied low, got %d/%d",
          dut->dec_nice, dut->nice_cmt_off_ilgl_o);

    printf("\n=== e203_exu_decode: %d failure(s) ===\n", fail_count);
    return fail_count ? 1 : 0;
}
