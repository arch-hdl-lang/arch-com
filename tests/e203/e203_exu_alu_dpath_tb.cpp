#include "VAluDpath.h"
#include <cstdio>
#include <cstdint>

int main() {
    auto* dut = new VAluDpath();
    int failures = 0;

    // bjp_req_alu_add_res always reflects the shared adder (by spec); only
    // validate it when BJP is the active requester (exp_bjp_add != UINT32_MAX).
    auto check = [&](const char* name,
                     uint32_t exp_alu, uint32_t exp_bjp_add,
                     uint8_t  exp_bjp_cmp, uint32_t exp_agu) {
        dut->eval();
        bool bjp_add_ok = (exp_bjp_add == UINT32_MAX)
                        || (dut->bjp_req_alu_add_res == exp_bjp_add);
        bool ok = dut->alu_req_alu_res  == exp_alu
               && bjp_add_ok
               && (uint8_t)dut->bjp_req_alu_cmp_res == exp_bjp_cmp
               && dut->agu_req_alu_res  == exp_agu;
        printf("  %s: %s\n", ok ? "PASS" : "FAIL", name);
        if (!ok) {
            printf("    alu_res=0x%08x(exp 0x%08x) bjp_add=0x%08x(exp 0x%08x)"
                   " bjp_cmp=%u(exp %u) agu_res=0x%08x(exp 0x%08x)\n",
                   dut->alu_req_alu_res, exp_alu,
                   dut->bjp_req_alu_add_res,
                   (exp_bjp_add == UINT32_MAX) ? dut->bjp_req_alu_add_res : exp_bjp_add,
                   (uint8_t)dut->bjp_req_alu_cmp_res, exp_bjp_cmp,
                   dut->agu_req_alu_res, exp_agu);
            failures++;
        }
    };

    // Default: all requests off, operands zero
    dut->clk = 0; dut->rst_n = 1;
    dut->alu_req_alu = 0; dut->bjp_req_alu = 0; dut->agu_req_alu = 0;
    dut->alu_req_alu_add = 0; dut->alu_req_alu_sub = 0; dut->alu_req_alu_xor = 0;
    dut->alu_req_alu_sll = 0; dut->alu_req_alu_srl = 0; dut->alu_req_alu_sra = 0;
    dut->alu_req_alu_or  = 0; dut->alu_req_alu_and = 0;
    dut->alu_req_alu_slt = 0; dut->alu_req_alu_sltu = 0; dut->alu_req_alu_lui = 0;
    dut->alu_req_alu_op1 = 0; dut->alu_req_alu_op2 = 0;
    dut->bjp_req_alu_op1 = 0; dut->bjp_req_alu_op2 = 0;
    dut->bjp_req_alu_cmp_eq = 0; dut->bjp_req_alu_cmp_ne = 0;
    dut->bjp_req_alu_cmp_lt = 0; dut->bjp_req_alu_cmp_gt = 0;
    dut->bjp_req_alu_cmp_ltu = 0; dut->bjp_req_alu_cmp_gtu = 0;
    dut->bjp_req_alu_add = 0;
    dut->agu_req_alu_op1 = 0; dut->agu_req_alu_op2 = 0;
    dut->agu_req_alu_swap = 0; dut->agu_req_alu_add = 0;
    dut->agu_req_alu_and = 0; dut->agu_req_alu_or  = 0; dut->agu_req_alu_xor = 0;
    dut->agu_req_alu_max = 0; dut->agu_req_alu_min = 0;
    dut->agu_req_alu_maxu = 0; dut->agu_req_alu_minu = 0;
    dut->agu_sbf_0_ena = 0; dut->agu_sbf_0_nxt = 0;
    dut->agu_sbf_1_ena = 0; dut->agu_sbf_1_nxt = 0;

    printf("=== ALU arithmetic ===\n");
    dut->alu_req_alu = 1;
    dut->alu_req_alu_op1 = 10; dut->alu_req_alu_op2 = 3;
    dut->alu_req_alu_add = 1;
    check("ADD 10+3=13", 13, UINT32_MAX, 0, 0);
    dut->alu_req_alu_add = 0;

    dut->alu_req_alu_sub = 1;
    check("SUB 10-3=7", 7, UINT32_MAX, 0, 0);
    dut->alu_req_alu_sub = 0;

    dut->alu_req_alu_op1 = 0xFF; dut->alu_req_alu_op2 = 0x0F;
    dut->alu_req_alu_and = 1;
    check("AND 0xFF&0x0F=0x0F", 0x0F, UINT32_MAX, 0, 0);
    dut->alu_req_alu_and = 0;

    dut->alu_req_alu_or = 1;
    check("OR 0xFF|0x0F=0xFF", 0xFF, UINT32_MAX, 0, 0);
    dut->alu_req_alu_or = 0;

    dut->alu_req_alu_xor = 1;
    check("XOR 0xFF^0x0F=0xF0", 0xF0, UINT32_MAX, 0, 0);
    dut->alu_req_alu_xor = 0;

    printf("=== ALU shifts ===\n");
    dut->alu_req_alu_op1 = 1; dut->alu_req_alu_op2 = 4;
    dut->alu_req_alu_sll = 1;
    check("SLL 1<<4=16", 16, UINT32_MAX, 0, 0);
    dut->alu_req_alu_sll = 0;

    dut->alu_req_alu_op1 = 0x100; dut->alu_req_alu_op2 = 4;
    dut->alu_req_alu_srl = 1;
    check("SRL 0x100>>4=0x10", 0x10, UINT32_MAX, 0, 0);
    dut->alu_req_alu_srl = 0;

    dut->alu_req_alu_op1 = (uint32_t)(-16); dut->alu_req_alu_op2 = 2;
    dut->alu_req_alu_sra = 1;
    check("SRA -16>>2=-4", (uint32_t)(-4), UINT32_MAX, 0, 0);
    dut->alu_req_alu_sra = 0;

    printf("=== ALU SLT/SLTU ===\n");
    dut->alu_req_alu_op1 = (uint32_t)(-1); dut->alu_req_alu_op2 = 1;
    dut->alu_req_alu_slt = 1;
    check("SLT -1<1 (signed) = 1", 1, UINT32_MAX, 0, 0);
    dut->alu_req_alu_slt = 0;

    dut->alu_req_alu_sltu = 1;
    check("SLTU 0xFFFFFFFF<1 (unsigned) = 0", 0, UINT32_MAX, 0, 0);
    dut->alu_req_alu_sltu = 0;

    dut->alu_req_alu_op1 = 5; dut->alu_req_alu_op2 = 10;
    dut->alu_req_alu_slt = 1;
    check("SLT 5<10 = 1", 1, UINT32_MAX, 0, 0);
    dut->alu_req_alu_slt = 0;

    printf("=== ALU LUI ===\n");
    dut->alu_req_alu_op2 = 0xDEAD0000;
    dut->alu_req_alu_lui = 1;
    check("LUI op2=0xDEAD0000", 0xDEAD0000, UINT32_MAX, 0, 0);
    dut->alu_req_alu_lui = 0;
    dut->alu_req_alu = 0;

    printf("=== BJP comparisons ===\n");
    dut->bjp_req_alu = 1;
    dut->bjp_req_alu_op1 = 5; dut->bjp_req_alu_op2 = 5;
    dut->bjp_req_alu_cmp_eq = 1;
    check("BJP EQ 5==5 = 1", 0, UINT32_MAX, 1, 0);
    dut->bjp_req_alu_cmp_eq = 0;

    dut->bjp_req_alu_cmp_ne = 1;
    check("BJP NE 5!=5 = 0", 0, UINT32_MAX, 0, 0);
    dut->bjp_req_alu_cmp_ne = 0;

    dut->bjp_req_alu_op1 = 3; dut->bjp_req_alu_op2 = 7;
    dut->bjp_req_alu_cmp_lt = 1;
    check("BJP LT 3<7 = 1", 0, UINT32_MAX, 1, 0);
    dut->bjp_req_alu_cmp_lt = 0;

    dut->bjp_req_alu_cmp_gt = 1;
    check("BJP GT 3>7 = 0", 0, UINT32_MAX, 0, 0);
    dut->bjp_req_alu_cmp_gt = 0;

    // Signed: -1 < 1
    dut->bjp_req_alu_op1 = (uint32_t)(-1); dut->bjp_req_alu_op2 = 1;
    dut->bjp_req_alu_cmp_lt = 1;
    check("BJP LT(signed) -1<1 = 1", 0, UINT32_MAX, 1, 0);
    dut->bjp_req_alu_cmp_lt = 0;

    // Unsigned: 0xFFFFFFFF > 1
    dut->bjp_req_alu_cmp_gtu = 1;
    check("BJP GTU 0xFFFFFFFF>1 = 1", 0, UINT32_MAX, 1, 0);
    dut->bjp_req_alu_cmp_gtu = 0;

    printf("=== BJP add (JAL/JALR return addr) ===\n");
    dut->bjp_req_alu_op1 = 0x1000; dut->bjp_req_alu_op2 = 4;
    dut->bjp_req_alu_add = 1;
    check("BJP ADD 0x1000+4=0x1004", 0, 0x1004, 0, 0);
    dut->bjp_req_alu_add = 0;
    dut->bjp_req_alu = 0;

    printf("=== AGU AMO operations ===\n");
    dut->agu_req_alu = 1;
    dut->agu_req_alu_op1 = 0x100; dut->agu_req_alu_op2 = 0x200;
    dut->agu_req_alu_add = 1;
    check("AGU ADD 0x100+0x200=0x300", 0, UINT32_MAX, 0, 0x300);
    dut->agu_req_alu_add = 0;

    dut->agu_req_alu_swap = 1;
    check("AGU SWAP result=op2=0x200", 0, UINT32_MAX, 0, 0x200);
    dut->agu_req_alu_swap = 0;

    // max(signed): 5 vs -3 → 5
    dut->agu_req_alu_op1 = 5; dut->agu_req_alu_op2 = (uint32_t)(-3);
    dut->agu_req_alu_max = 1;
    check("AGU MAX(signed) max(5,-3)=5", 0, UINT32_MAX, 0, 5);
    dut->agu_req_alu_max = 0;

    dut->agu_req_alu_min = 1;
    check("AGU MIN(signed) min(5,-3)=-3", 0, UINT32_MAX, 0, (uint32_t)(-3));
    dut->agu_req_alu_min = 0;

    // maxu: 5 vs 0xFFFFFFFF → 0xFFFFFFFF
    dut->agu_req_alu_op1 = 5; dut->agu_req_alu_op2 = 0xFFFFFFFF;
    dut->agu_req_alu_maxu = 1;
    check("AGU MAXU maxu(5,0xFFFFFFFF)=0xFFFFFFFF", 0, UINT32_MAX, 0, 0xFFFFFFFF);
    dut->agu_req_alu_maxu = 0;

    dut->agu_req_alu_minu = 1;
    check("AGU MINU minu(5,0xFFFFFFFF)=5", 0, UINT32_MAX, 0, 5);
    dut->agu_req_alu_minu = 0;
    dut->agu_req_alu = 0;

    delete dut;
    printf("\n%s  (%d failure(s))\n", failures ? "FAILED" : "ALL TESTS PASSED", failures);
    return failures ? 1 : 0;
}
