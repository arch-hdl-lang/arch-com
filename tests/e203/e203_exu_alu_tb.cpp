// E203 ExuAlu testbench — tests the ALU top-level that instantiates
// AluDpath and BjpUnit.  Exercises ALU ops, BJP comparisons, and AGU paths.

#include <cstdio>
#include <cstdint>
#include <cstdlib>

#include "VExuAlu.h"

static int errors = 0;

static void check(const char* name, uint32_t got, uint32_t exp) {
    if (got != exp) {
        printf("FAIL %s: got 0x%08x, expected 0x%08x\n", name, got, exp);
        errors++;
    }
}

static void check_bool(const char* name, bool got, bool exp) {
    if (got != exp) {
        printf("FAIL %s: got %d, expected %d\n", name, (int)got, (int)exp);
        errors++;
    }
}

int main() {
    VExuAlu dut;
    dut.rst_n = 1;

    // Helper: clear all operation selects
    auto clear_ops = [&]() {
        dut.i_valid = 1;
        dut.o_ready = 1;
        dut.i_alu = 0; dut.i_bjp = 0; dut.i_agu = 0;
        dut.i_alu_add = 0; dut.i_alu_sub = 0; dut.i_alu_xor = 0;
        dut.i_alu_sll = 0; dut.i_alu_srl = 0; dut.i_alu_sra = 0;
        dut.i_alu_or = 0; dut.i_alu_and = 0; dut.i_alu_slt = 0;
        dut.i_alu_sltu = 0; dut.i_alu_lui = 0;
        dut.i_beq = 0; dut.i_bne = 0; dut.i_blt = 0; dut.i_bge = 0;
        dut.i_bltu = 0; dut.i_bgeu = 0; dut.i_jump = 0;
        dut.i_agu_swap = 0; dut.i_agu_add = 0; dut.i_agu_and = 0;
        dut.i_agu_or = 0; dut.i_agu_xor = 0; dut.i_agu_max = 0;
        dut.i_agu_min = 0; dut.i_agu_maxu = 0; dut.i_agu_minu = 0;
        dut.i_agu_sbf_0_ena = 0; dut.i_agu_sbf_1_ena = 0;
        dut.i_rs1 = 0; dut.i_rs2 = 0; dut.i_pc = 0; dut.i_imm = 0;
        dut.i_rdidx = 0;
    };

    // ── Test 1: ALU ADD ────────────────────────────────────────────────
    printf("Test 1: ALU ADD\n");
    clear_ops();
    dut.i_alu = 1; dut.i_alu_add = 1;
    dut.i_rs1 = 100; dut.i_rs2 = 200;
    dut.i_rdidx = 5;
    dut.eval();
    check("ADD result", dut.o_wdat, 300);
    check("ADD rdidx", dut.o_rdidx, 5);
    check_bool("ADD o_valid", dut.o_valid, true);
    check_bool("ADD i_ready", dut.i_ready, true);

    // ── Test 2: ALU SUB ────────────────────────────────────────────────
    printf("Test 2: ALU SUB\n");
    clear_ops();
    dut.i_alu = 1; dut.i_alu_sub = 1;
    dut.i_rs1 = 500; dut.i_rs2 = 200;
    dut.eval();
    check("SUB result", dut.o_wdat, 300);

    // ── Test 3: ALU XOR ────────────────────────────────────────────────
    printf("Test 3: ALU XOR\n");
    clear_ops();
    dut.i_alu = 1; dut.i_alu_xor = 1;
    dut.i_rs1 = 0xFF00FF00; dut.i_rs2 = 0x0F0F0F0F;
    dut.eval();
    check("XOR result", dut.o_wdat, 0xF00FF00F);

    // ── Test 4: ALU SLL ────────────────────────────────────────────────
    printf("Test 4: ALU SLL\n");
    clear_ops();
    dut.i_alu = 1; dut.i_alu_sll = 1;
    dut.i_rs1 = 1; dut.i_rs2 = 16;
    dut.eval();
    check("SLL result", dut.o_wdat, 0x10000);

    // ── Test 5: ALU SRL ────────────────────────────────────────────────
    printf("Test 5: ALU SRL\n");
    clear_ops();
    dut.i_alu = 1; dut.i_alu_srl = 1;
    dut.i_rs1 = 0x80000000; dut.i_rs2 = 4;
    dut.eval();
    check("SRL result", dut.o_wdat, 0x08000000);

    // ── Test 6: ALU SRA ────────────────────────────────────────────────
    printf("Test 6: ALU SRA\n");
    clear_ops();
    dut.i_alu = 1; dut.i_alu_sra = 1;
    dut.i_rs1 = 0x80000000; dut.i_rs2 = 4;
    dut.eval();
    check("SRA result", dut.o_wdat, 0xF8000000);

    // ── Test 7: ALU SLT (signed) ───────────────────────────────────────
    printf("Test 7: ALU SLT\n");
    clear_ops();
    dut.i_alu = 1; dut.i_alu_slt = 1;
    dut.i_rs1 = 0xFFFFFFFF; // -1 signed
    dut.i_rs2 = 1;
    dut.eval();
    check("SLT(-1 < 1)", dut.o_wdat, 1);

    // ── Test 8: ALU SLTU (unsigned) ────────────────────────────────────
    printf("Test 8: ALU SLTU\n");
    clear_ops();
    dut.i_alu = 1; dut.i_alu_sltu = 1;
    dut.i_rs1 = 1;
    dut.i_rs2 = 0xFFFFFFFF;
    dut.eval();
    check("SLTU(1 < 0xFFFF...)", dut.o_wdat, 1);

    // ── Test 9: ALU LUI ────────────────────────────────────────────────
    printf("Test 9: ALU LUI\n");
    clear_ops();
    dut.i_alu = 1; dut.i_alu_lui = 1;
    dut.i_rs2 = 0x12345000;
    dut.eval();
    check("LUI result", dut.o_wdat, 0x12345000);

    // ── Test 10: BJP BEQ taken ─────────────────────────────────────────
    printf("Test 10: BJP BEQ taken\n");
    clear_ops();
    dut.i_bjp = 1; dut.i_beq = 1;
    dut.i_rs1 = 42; dut.i_rs2 = 42;
    dut.i_pc = 0x1000; dut.i_imm = 0x100;
    dut.eval();
    check_bool("BEQ taken", dut.o_bjp_taken, true);
    check("BEQ target", dut.o_bjp_tgt, 0x1100);
    check("BEQ link", dut.o_bjp_lnk, 0x1004);

    // ── Test 11: BJP BEQ not taken ─────────────────────────────────────
    printf("Test 11: BJP BEQ not taken\n");
    clear_ops();
    dut.i_bjp = 1; dut.i_beq = 1;
    dut.i_rs1 = 42; dut.i_rs2 = 43;
    dut.i_pc = 0x1000; dut.i_imm = 0x100;
    dut.eval();
    check_bool("BEQ not taken", dut.o_bjp_taken, false);

    // ── Test 12: BJP BNE ───────────────────────────────────────────────
    printf("Test 12: BJP BNE\n");
    clear_ops();
    dut.i_bjp = 1; dut.i_bne = 1;
    dut.i_rs1 = 10; dut.i_rs2 = 20;
    dut.eval();
    check_bool("BNE taken", dut.o_bjp_taken, true);

    // ── Test 13: BJP BLT (signed) ──────────────────────────────────────
    printf("Test 13: BJP BLT\n");
    clear_ops();
    dut.i_bjp = 1; dut.i_blt = 1;
    dut.i_rs1 = 0xFFFFFFFF; // -1
    dut.i_rs2 = 1;
    dut.eval();
    check_bool("BLT(-1 < 1)", dut.o_bjp_taken, true);

    // ── Test 14: BJP BLTU (unsigned) ───────────────────────────────────
    printf("Test 14: BJP BLTU\n");
    clear_ops();
    dut.i_bjp = 1; dut.i_bltu = 1;
    dut.i_rs1 = 5; dut.i_rs2 = 10;
    dut.eval();
    check_bool("BLTU(5 < 10)", dut.o_bjp_taken, true);

    // ── Test 15: BJP JAL (unconditional jump) ──────────────────────────
    printf("Test 15: BJP JAL\n");
    clear_ops();
    dut.i_bjp = 1; dut.i_jump = 1;
    dut.i_pc = 0x2000; dut.i_imm = 0x400;
    dut.eval();
    check_bool("JAL taken", dut.o_bjp_taken, true);
    check("JAL target", dut.o_bjp_tgt, 0x2400);
    check("JAL link (wdat)", dut.o_wdat, 0x2004);

    // ── Test 16: AGU ADD (load/store addr) ─────────────────────────────
    printf("Test 16: AGU ADD\n");
    clear_ops();
    dut.i_agu = 1; dut.i_agu_add = 1;
    dut.i_rs1 = 0x80000000; dut.i_imm = 0x100;
    dut.eval();
    check("AGU ADD result", dut.o_wdat, 0x80000100);

    // ── Test 17: AGU XOR (AMO) ─────────────────────────────────────────
    printf("Test 17: AGU XOR\n");
    clear_ops();
    dut.i_agu = 1; dut.i_agu_xor = 1;
    dut.i_rs1 = 0xAAAAAAAA; dut.i_imm = 0x55555555;
    dut.eval();
    check("AGU XOR result", dut.o_wdat, 0xFFFFFFFF);

    // ── Test 18: AGU SWAP ──────────────────────────────────────────────
    printf("Test 18: AGU SWAP\n");
    clear_ops();
    dut.i_agu = 1; dut.i_agu_swap = 1;
    dut.i_rs1 = 0xDEAD; dut.i_imm = 0xBEEF;
    dut.eval();
    check("AGU SWAP result", dut.o_wdat, 0xBEEF);

    // ── Test 19: Valid/ready handshake ──────────────────────────────────
    printf("Test 19: Handshake\n");
    clear_ops();
    dut.i_valid = 0; dut.o_ready = 0;
    dut.eval();
    check_bool("o_valid when i_valid=0", dut.o_valid, false);
    check_bool("i_ready when o_ready=0", dut.i_ready, false);

    // ── Test 20: BJP wdat is link addr ─────────────────────────────────
    printf("Test 20: BJP wdat = link\n");
    clear_ops();
    dut.i_bjp = 1; dut.i_jump = 1;
    dut.i_pc = 0x3000; dut.i_imm = 0x800;
    dut.i_rdidx = 1;  // ra
    dut.eval();
    check("BJP wdat = PC+4", dut.o_wdat, 0x3004);
    check("BJP rdidx", dut.o_rdidx, 1);

    // ── Summary ────────────────────────────────────────────────────────
    printf("\n=== ExuAlu: %d tests, %d errors ===\n", 20, errors);
    return errors ? 1 : 0;
}
