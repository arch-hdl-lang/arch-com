#include "VBjpUnit.h"
#include <cstdio>
#include <cstdint>

int main() {
    auto* dut = new VBjpUnit();
    int failures = 0;

    // Helper: set all branch/jump selectors to 0 and configure a single one
    auto clear_sel = [&]() {
        dut->i_beq = 0; dut->i_bne = 0; dut->i_blt = 0;
        dut->i_bge = 0; dut->i_bltu = 0; dut->i_bgeu = 0;
        dut->i_jump = 0;
    };

    // check(name, exp_taken, exp_tgt, exp_lnk, exp_cmp)
    auto check = [&](const char* name,
                     uint8_t exp_taken, uint32_t exp_tgt,
                     uint32_t exp_lnk, uint8_t exp_cmp) {
        dut->eval();
        bool ok = (uint8_t)dut->o_taken   == exp_taken
               && dut->o_tgt             == exp_tgt
               && dut->o_lnk             == exp_lnk
               && (uint8_t)dut->o_cmp_res == exp_cmp;
        printf("  %s: %s\n", ok ? "PASS" : "FAIL", name);
        if (!ok) {
            printf("    taken=%u(exp %u) tgt=0x%08x(exp 0x%08x)"
                   " lnk=0x%08x(exp 0x%08x) cmp=%u(exp %u)\n",
                   (uint8_t)dut->o_taken, exp_taken,
                   dut->o_tgt, exp_tgt,
                   dut->o_lnk, exp_lnk,
                   (uint8_t)dut->o_cmp_res, exp_cmp);
            failures++;
        }
    };

    // Default operands
    dut->i_tgt_op1 = 0; dut->i_tgt_op2 = 0;
    dut->i_cmp_rs1 = 0; dut->i_cmp_rs2 = 0;
    dut->i_lnk_pc = 0;
    clear_sel();

    // ── Target and link address ───────────────────────────────────────────
    printf("=== Target address ===\n");
    dut->i_tgt_op1 = 0x1000; dut->i_tgt_op2 = 0x20;
    dut->i_lnk_pc  = 0x1000;
    dut->i_beq = 1; dut->i_cmp_rs1 = 5; dut->i_cmp_rs2 = 5;  // EQ → taken
    check("tgt = 0x1000+0x20 = 0x1020; lnk = 0x1004",
          1, 0x1020, 0x1004, 1);
    dut->i_beq = 0;

    dut->i_tgt_op1 = 0xFFFFFFFC; dut->i_tgt_op2 = 8;  // wrap around
    dut->i_lnk_pc  = 0x8000;
    check("tgt = 0xFFFFFFFC+8 = 0x4 (wrap); lnk = 0x8004",
          0, 0x4, 0x8004, 0);

    // ── BEQ ──────────────────────────────────────────────────────────────
    printf("=== BEQ ===\n");
    dut->i_tgt_op1 = 0x1000; dut->i_tgt_op2 = 4; dut->i_lnk_pc = 0x1000;
    dut->i_beq = 1;

    dut->i_cmp_rs1 = 42; dut->i_cmp_rs2 = 42;
    check("BEQ 42==42 → taken", 1, 0x1004, 0x1004, 1);

    dut->i_cmp_rs1 = 1; dut->i_cmp_rs2 = 2;
    check("BEQ 1==2 → not taken", 0, 0x1004, 0x1004, 0);

    dut->i_beq = 0;

    // ── BNE ──────────────────────────────────────────────────────────────
    printf("=== BNE ===\n");
    dut->i_bne = 1;

    dut->i_cmp_rs1 = 1; dut->i_cmp_rs2 = 2;
    check("BNE 1!=2 → taken", 1, 0x1004, 0x1004, 1);

    dut->i_cmp_rs1 = 7; dut->i_cmp_rs2 = 7;
    check("BNE 7!=7 → not taken", 0, 0x1004, 0x1004, 0);

    dut->i_bne = 0;

    // ── BLT (signed) ─────────────────────────────────────────────────────
    printf("=== BLT (signed) ===\n");
    dut->i_blt = 1;

    dut->i_cmp_rs1 = 3; dut->i_cmp_rs2 = 10;
    check("BLT 3<10 (signed) → taken", 1, 0x1004, 0x1004, 1);

    dut->i_cmp_rs1 = (uint32_t)(-1); dut->i_cmp_rs2 = 1;
    check("BLT -1<1 (signed) → taken", 1, 0x1004, 0x1004, 1);

    dut->i_cmp_rs1 = 10; dut->i_cmp_rs2 = 3;
    check("BLT 10<3 → not taken", 0, 0x1004, 0x1004, 0);

    dut->i_cmp_rs1 = 5; dut->i_cmp_rs2 = 5;
    check("BLT 5<5 → not taken", 0, 0x1004, 0x1004, 0);

    dut->i_blt = 0;

    // ── BGE (signed) ─────────────────────────────────────────────────────
    printf("=== BGE (signed) ===\n");
    dut->i_bge = 1;

    dut->i_cmp_rs1 = 10; dut->i_cmp_rs2 = 3;
    check("BGE 10>=3 (signed) → taken", 1, 0x1004, 0x1004, 1);

    dut->i_cmp_rs1 = 5; dut->i_cmp_rs2 = 5;
    check("BGE 5>=5 (signed) → taken", 1, 0x1004, 0x1004, 1);

    dut->i_cmp_rs1 = 1; dut->i_cmp_rs2 = (uint32_t)(-1);
    check("BGE 1>=-1 (signed) → taken", 1, 0x1004, 0x1004, 1);

    dut->i_cmp_rs1 = (uint32_t)(-2); dut->i_cmp_rs2 = 1;
    check("BGE -2>=1 (signed) → not taken", 0, 0x1004, 0x1004, 0);

    dut->i_bge = 0;

    // ── BLTU (unsigned) ───────────────────────────────────────────────────
    printf("=== BLTU (unsigned) ===\n");
    dut->i_bltu = 1;

    dut->i_cmp_rs1 = 1; dut->i_cmp_rs2 = 0xFFFFFFFF;
    check("BLTU 1<0xFFFFFFFF (unsigned) → taken", 1, 0x1004, 0x1004, 1);

    dut->i_cmp_rs1 = 0xFFFFFFFF; dut->i_cmp_rs2 = 1;
    check("BLTU 0xFFFFFFFF<1 → not taken", 0, 0x1004, 0x1004, 0);

    dut->i_cmp_rs1 = 0; dut->i_cmp_rs2 = 0;
    check("BLTU 0<0 → not taken", 0, 0x1004, 0x1004, 0);

    dut->i_bltu = 0;

    // ── BGEU (unsigned) ───────────────────────────────────────────────────
    printf("=== BGEU (unsigned) ===\n");
    dut->i_bgeu = 1;

    dut->i_cmp_rs1 = 0xFFFFFFFF; dut->i_cmp_rs2 = 1;
    check("BGEU 0xFFFFFFFF>=1 (unsigned) → taken", 1, 0x1004, 0x1004, 1);

    dut->i_cmp_rs1 = 5; dut->i_cmp_rs2 = 5;
    check("BGEU 5>=5 → taken", 1, 0x1004, 0x1004, 1);

    dut->i_cmp_rs1 = 1; dut->i_cmp_rs2 = 0xFFFFFFFF;
    check("BGEU 1>=0xFFFFFFFF → not taken", 0, 0x1004, 0x1004, 0);

    dut->i_bgeu = 0;

    // ── JAL / JALR: unconditional jump ───────────────────────────────────
    printf("=== Unconditional jump (JAL/JALR) ===\n");
    dut->i_jump = 1;

    // JAL: PC=0x2000, offset=0x100 → target=0x2100, lnk=0x2004
    dut->i_tgt_op1 = 0x2000; dut->i_tgt_op2 = 0x100;
    dut->i_lnk_pc  = 0x2000;
    dut->i_cmp_rs1 = 1; dut->i_cmp_rs2 = 2;  // comparison false, but jump overrides
    check("JAL tgt=0x2100; taken regardless of cmp", 1, 0x2100, 0x2004, 0);

    // JALR: rs1=0x3000, offset=0x18 → target=0x3018; lnk=0x3004
    dut->i_tgt_op1 = 0x3000; dut->i_tgt_op2 = 0x18;
    dut->i_lnk_pc  = 0x3000;
    check("JALR rs1=0x3000+0x18=0x3018", 1, 0x3018, 0x3004, 0);

    // Jump with cmp true — o_taken still 1
    dut->i_cmp_rs1 = 5; dut->i_cmp_rs2 = 5;
    dut->i_beq = 1;
    check("JAL with BEQ true: taken=1", 1, 0x3018, 0x3004, 1);
    dut->i_beq = 0;
    dut->i_jump = 0;

    // ── No selector active → no branch ───────────────────────────────────
    printf("=== No selector (default 0) ===\n");
    dut->i_cmp_rs1 = 0xDEAD; dut->i_cmp_rs2 = 0xBEEF;
    check("No op active: taken=0 cmp=0", 0, 0x3018, 0x3004, 0);

    delete dut;
    printf("\n%s  (%d failure(s))\n",
           failures ? "FAILED" : "ALL TESTS PASSED", failures);
    return failures ? 1 : 0;
}
