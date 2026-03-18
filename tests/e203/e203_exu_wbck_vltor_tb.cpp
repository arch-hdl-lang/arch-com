#include "Ve203_exu_wbck.h"
#include "verilated.h"
#include <cstdio>

int main(int argc, char** argv) {
    Verilated::commandArgs(argc, argv);
    auto* dut = new Ve203_exu_wbck();
    int failures = 0;

    auto eval = [&]() { dut->clk = 0; dut->eval(); };

    auto check = [&](const char* test,
                     uint8_t exp_longp_ready, uint8_t exp_alu_ready,
                     uint8_t exp_rf_ena, uint32_t exp_wdat, uint8_t exp_rdidx) {
        eval();
        bool ok = dut->longp_wbck_i_ready == exp_longp_ready
               && dut->alu_wbck_i_ready   == exp_alu_ready
               && dut->rf_wbck_o_ena      == exp_rf_ena
               && dut->rf_wbck_o_wdat     == exp_wdat
               && dut->rf_wbck_o_rdidx    == exp_rdidx;
        printf("  %s: %s\n", ok ? "PASS" : "FAIL", test);
        if (!ok) {
            printf("    longp_ready=%u(exp %u) alu_ready=%u(exp %u) "
                   "rf_ena=%u(exp %u) wdat=0x%08x(exp 0x%08x) rdidx=%u(exp %u)\n",
                   (uint8_t)dut->longp_wbck_i_ready, exp_longp_ready,
                   (uint8_t)dut->alu_wbck_i_ready,   exp_alu_ready,
                   (uint8_t)dut->rf_wbck_o_ena,      exp_rf_ena,
                   (uint32_t)dut->rf_wbck_o_wdat,    exp_wdat,
                   (uint8_t)dut->rf_wbck_o_rdidx,    exp_rdidx);
            failures++;
        }
    };

    dut->alu_wbck_i_valid   = 0; dut->alu_wbck_i_wdat  = 0; dut->alu_wbck_i_rdidx  = 0;
    dut->longp_wbck_i_valid = 0; dut->longp_wbck_i_wdat = 0; dut->longp_wbck_i_rdidx = 0;
    dut->longp_wbck_i_flags = 0; dut->longp_wbck_i_rdfpu = 0;

    printf("=== Test 1: No requests ===\n");
    check("idle: longp_ready=1 alu_ready=1 rf_ena=0", 1, 1, 0, 0, 0);

    printf("=== Test 2: ALU only ===\n");
    dut->alu_wbck_i_valid = 1; dut->alu_wbck_i_wdat = 0xDEAD1234; dut->alu_wbck_i_rdidx = 7;
    check("alu valid: rf_ena=1 wdat=0xDEAD1234 rdidx=7 alu_ready=1", 1, 1, 1, 0xDEAD1234, 7);

    printf("=== Test 3: Longp only ===\n");
    dut->alu_wbck_i_valid = 0;
    dut->longp_wbck_i_valid = 1; dut->longp_wbck_i_wdat = 0xBEEF5678; dut->longp_wbck_i_rdidx = 15;
    dut->longp_wbck_i_rdfpu = 0;
    check("longp valid: rf_ena=1 wdat=0xBEEF5678 rdidx=15 alu_ready=0", 1, 0, 1, 0xBEEF5678, 15);

    printf("=== Test 4: Both valid — longp wins ===\n");
    dut->alu_wbck_i_valid = 1; dut->alu_wbck_i_wdat = 0xAAAAAAAA; dut->alu_wbck_i_rdidx = 3;
    dut->longp_wbck_i_valid = 1; dut->longp_wbck_i_wdat = 0x55555555; dut->longp_wbck_i_rdidx = 20;
    check("collision: longp wins wdat=0x55555555 rdidx=20 alu_ready=0", 1, 0, 1, 0x55555555, 20);

    printf("=== Test 5: Longp writing to FPU reg — rf_ena suppressed ===\n");
    dut->alu_wbck_i_valid = 0;
    dut->longp_wbck_i_valid = 1; dut->longp_wbck_i_wdat = 0x12345678; dut->longp_wbck_i_rdidx = 5;
    dut->longp_wbck_i_rdfpu = 1;
    check("longp rdfpu=1: rf_ena=0 (FPU dest, no int regfile write)", 1, 0, 0, 0x12345678, 5);

    printf("=== Test 6: ALU with rdfpu=0 — normal write ===\n");
    dut->alu_wbck_i_valid = 1; dut->alu_wbck_i_wdat = 0xCAFEBABE; dut->alu_wbck_i_rdidx = 31;
    dut->longp_wbck_i_valid = 0; dut->longp_wbck_i_rdfpu = 0;
    check("alu rdidx=31: rf_ena=1 wdat=0xCAFEBABE", 1, 1, 1, 0xCAFEBABE, 31);

    dut->final(); delete dut;
    printf("\n%s  (%d failure(s))\n", failures ? "FAILED" : "ALL TESTS PASSED", failures);
    return failures ? 1 : 0;
}
