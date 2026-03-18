#include "VLiteBpu.h"
#include <cstdio>

int main() {
    auto* dut = new VLiteBpu();
    int failures = 0;

    // Clock the DUT (eval combinational + posedge)
    auto posedge = [&]() {
        dut->clk = 0; dut->eval();
        dut->clk = 1; dut->eval();
    };

    auto comb_eval = [&]() { dut->clk = 0; dut->eval(); };

    auto check_comb = [&](const char* test,
                          uint8_t exp_taken, uint32_t exp_op1, uint32_t exp_op2,
                          uint8_t exp_wait, uint8_t exp_rs1_ena) {
        comb_eval();
        bool ok = dut->prdt_taken      == exp_taken
               && dut->prdt_pc_add_op1 == exp_op1
               && dut->prdt_pc_add_op2 == exp_op2
               && dut->bpu_wait        == exp_wait
               && dut->bpu2rf_rs1_ena  == exp_rs1_ena;
        printf("  %s: %s\n", ok ? "PASS" : "FAIL", test);
        if (!ok) {
            printf("    taken=%u(exp %u) op1=0x%08x(exp 0x%08x) op2=0x%08x(exp 0x%08x)"
                   " wait=%u(exp %u) rs1_ena=%u(exp %u)\n",
                   (uint8_t)dut->prdt_taken,      exp_taken,
                   (uint32_t)dut->prdt_pc_add_op1, exp_op1,
                   (uint32_t)dut->prdt_pc_add_op2, exp_op2,
                   (uint8_t)dut->bpu_wait,         exp_wait,
                   (uint8_t)dut->bpu2rf_rs1_ena,   exp_rs1_ena);
            failures++;
        }
    };

    // Reset
    dut->rst_n = 0;
    dut->clk   = 0;
    dut->dec_jal   = 0; dut->dec_jalr  = 0; dut->dec_bxx   = 0;
    dut->dec_bjp_imm = 0; dut->dec_jalr_rs1idx = 0;
    dut->oitf_empty = 1; dut->ir_empty = 1; dut->ir_rs1en = 0;
    dut->jalr_rs1idx_cam_irrdidx = 0;
    dut->dec_i_valid = 0; dut->ir_valid_clr = 0;
    dut->rf2bpu_x1 = 0; dut->rf2bpu_rs1 = 0;
    dut->pc = 0;
    posedge(); // apply reset
    dut->rst_n = 1;

    printf("=== Test 1: No instruction ===\n");
    dut->pc = 0x1000;
    check_comb("idle: taken=0 wait=0 rs1_ena=0", 0, 0x1000, 0, 0, 0);

    printf("=== Test 2: JAL always taken, op1=PC ===\n");
    dut->dec_jal = 1; dut->dec_bjp_imm = 0x100;
    check_comb("jal: taken=1 op1=PC op2=imm", 1, 0x1000, 0x100, 0, 0);

    printf("=== Test 3: Bxx forward (positive offset) — not taken ===\n");
    dut->dec_jal = 0; dut->dec_bxx = 1; dut->dec_bjp_imm = 4;
    check_comb("bxx forward: taken=0 op1=PC", 0, 0x1000, 4, 0, 0);

    printf("=== Test 4: Bxx backward (negative offset) — taken ===\n");
    dut->dec_bjp_imm = (uint32_t)(-4);  // 0xFFFFFFFC — MSB set
    check_comb("bxx backward: taken=1 op1=PC", 1, 0x1000, (uint32_t)(-4), 0, 0);
    dut->dec_bxx = 0;

    printf("=== Test 5: JALR x0 — taken, op1=0 ===\n");
    dut->dec_jalr = 1; dut->dec_jalr_rs1idx = 0;
    dut->dec_bjp_imm = 0x200; dut->pc = 0x2000;
    check_comb("jalr x0: taken=1 op1=0 wait=0", 1, 0, 0x200, 0, 0);

    printf("=== Test 6: JALR x1, no dependency — taken, op1=rf2bpu_x1 ===\n");
    dut->dec_jalr_rs1idx = 1; dut->rf2bpu_x1 = 0xABCD0000;
    dut->oitf_empty = 1; dut->jalr_rs1idx_cam_irrdidx = 0;
    check_comb("jalr x1 no dep: taken=1 op1=x1 wait=0", 1, 0xABCD0000, 0x200, 0, 0);

    printf("=== Test 7: JALR x1, OITF not empty — bpu_wait=1 ===\n");
    dut->oitf_empty = 0;
    check_comb("jalr x1 oitf busy: wait=1", 1, 0xABCD0000, 0x200, 1, 0);
    dut->oitf_empty = 1;

    printf("=== Test 8: JALR x1, IR conflict — bpu_wait=1 ===\n");
    dut->jalr_rs1idx_cam_irrdidx = 1;
    check_comb("jalr x1 ir conflict: wait=1", 1, 0xABCD0000, 0x200, 1, 0);
    dut->jalr_rs1idx_cam_irrdidx = 0;

    printf("=== Test 9: JALR x2 (xN), no dep, dec_i_valid=1 — rs1_ena=1 ===\n");
    dut->dec_jalr_rs1idx = 2; dut->rf2bpu_rs1 = 0x5678;
    dut->dec_i_valid = 1; dut->oitf_empty = 1; dut->ir_empty = 1;
    // rs1xn_rdrf_r is still 0 (reset), so rs1xn_rdrf_set should be 1
    check_comb("jalr x2 no dep: rs1_ena=1 wait=0 op1=rs1", 1, 0x5678, 0x200, 0, 1);

    printf("=== Test 10: JALR x2, OITF busy — bpu_wait=1, rs1_ena=0 ===\n");
    dut->oitf_empty = 0;
    check_comb("jalr x2 oitf busy: wait=1 rs1_ena=0", 1, 0x5678, 0x200, 1, 0);
    dut->oitf_empty = 1;

    printf("=== Test 11: JALR x2, rs1xn state machine — clocks to 1 then 0 ===\n");
    // Clock once: rs1xn_rdrf_r becomes rs1xn_rdrf_set=1
    posedge();
    // Now rs1xn_rdrf_r=1, so rs1xn_rdrf_set=0, bpu2rf_rs1_ena=0
    check_comb("jalr x2 after latch: rs1_ena=0 wait=0", 1, 0x5678, 0x200, 0, 0);
    // Clock again: rs1xn_rdrf_r becomes 0
    posedge();
    dut->dec_i_valid = 1; // trigger new set
    check_comb("jalr x2 cleared: rs1_ena=1", 1, 0x5678, 0x200, 0, 1);

    delete dut;
    printf("\n%s  (%d failure(s))\n", failures ? "FAILED" : "ALL TESTS PASSED", failures);
    return failures ? 1 : 0;
}
