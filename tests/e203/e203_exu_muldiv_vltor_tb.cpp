// Verilator cross-check testbench for ExuMuldiv
#include "VExuMuldiv.h"
#include "verilated.h"
#include <cstdio>
#include <cstdint>

static int fail_count = 0;
static VExuMuldiv* dut;

#define CHECK(cond, fmt, ...) do { \
    if (!(cond)) { printf("  FAIL: " fmt "\n", ##__VA_ARGS__); fail_count++; } \
} while(0)

static void tick() { dut->clk = 0; dut->eval(); dut->clk = 1; dut->eval(); }

static void reset() {
    dut->rst_n = 0; dut->i_valid = 0; dut->o_ready = 0;
    for (int i = 0; i < 3; i++) tick();
    dut->rst_n = 1; tick();
}

static uint32_t run_op(uint32_t rs1, uint32_t rs2,
                       bool mul, bool mulh, bool mulhsu, bool mulhu,
                       bool div, bool divu, bool rem, bool remu) {
    while (!dut->i_ready) tick();
    dut->i_valid=1; dut->i_rs1=rs1; dut->i_rs2=rs2;
    dut->i_mul=mul; dut->i_mulh=mulh; dut->i_mulhsu=mulhsu; dut->i_mulhu=mulhu;
    dut->i_div=div; dut->i_divu=divu; dut->i_rem=rem; dut->i_remu=remu;
    tick();
    dut->i_valid=0; dut->i_mul=0; dut->i_mulh=0; dut->i_mulhsu=0; dut->i_mulhu=0;
    dut->i_div=0; dut->i_divu=0; dut->i_rem=0; dut->i_remu=0;
    int c=0; while(!dut->o_valid && c<100){tick();c++;}
    if(!dut->o_valid){printf("  TIMEOUT!\n");fail_count++;return 0;}
    uint32_t r=dut->o_wdat; dut->o_ready=1; tick(); dut->o_ready=0;
    return r;
}

static uint32_t do_mul(uint32_t a, uint32_t b){return run_op(a,b,1,0,0,0,0,0,0,0);}
static uint32_t do_mulh(uint32_t a, uint32_t b){return run_op(a,b,0,1,0,0,0,0,0,0);}
static uint32_t do_mulhu(uint32_t a, uint32_t b){return run_op(a,b,0,0,0,1,0,0,0,0);}
static uint32_t do_div(uint32_t a, uint32_t b){return run_op(a,b,0,0,0,0,1,0,0,0);}
static uint32_t do_divu(uint32_t a, uint32_t b){return run_op(a,b,0,0,0,0,0,1,0,0);}
static uint32_t do_rem(uint32_t a, uint32_t b){return run_op(a,b,0,0,0,0,0,0,1,0);}
static uint32_t do_remu(uint32_t a, uint32_t b){return run_op(a,b,0,0,0,0,0,0,0,1);}

int main(int argc, char** argv) {
    Verilated::commandArgs(argc, argv);
    dut = new VExuMuldiv();
    reset();
    uint32_t r;

    printf("=== ExuMuldiv Verilator cross-check ===\n");

    printf("Test 1: MUL 7*3\n");
    r=do_mul(7,3); CHECK(r==21,"got 0x%08X",r);

    printf("Test 2: MUL 0xFFFF*0xFFFF\n");
    r=do_mul(0xFFFF,0xFFFF); CHECK(r==0xFFFE0001,"got 0x%08X",r);

    printf("Test 3: MUL (-3)*5\n");
    r=do_mul((uint32_t)-3,5); CHECK(r==(uint32_t)-15,"got 0x%08X",r);

    printf("Test 4: MULHU 0xFFFFFFFF*0xFFFFFFFF\n");
    r=do_mulhu(0xFFFFFFFF,0xFFFFFFFF); CHECK(r==0xFFFFFFFE,"got 0x%08X",r);

    printf("Test 5: MULH (-1)*(-1)\n");
    r=do_mulh((uint32_t)-1,(uint32_t)-1); CHECK(r==0,"got 0x%08X",r);

    printf("Test 6: DIVU 20/3\n");
    r=do_divu(20,3); CHECK(r==6,"got 0x%08X",r);

    printf("Test 7: REMU 20%%3\n");
    r=do_remu(20,3); CHECK(r==2,"got 0x%08X",r);

    printf("Test 8: DIV (-20)/3\n");
    r=do_div((uint32_t)-20,3); CHECK(r==(uint32_t)-6,"got 0x%08X",r);

    printf("Test 9: REM (-20)%%3\n");
    r=do_rem((uint32_t)-20,3); CHECK(r==(uint32_t)-2,"got 0x%08X",r);

    printf("Test 10: DIVU x/0\n");
    r=do_divu(42,0); CHECK(r==0xFFFFFFFF,"got 0x%08X",r);

    printf("Test 11: REMU x%%0\n");
    r=do_remu(42,0); CHECK(r==42,"got 0x%08X",r);

    printf("Test 12: DIVU 0/5\n");
    r=do_divu(0,5); CHECK(r==0,"got 0x%08X",r);

    if(fail_count==0) printf("\nAll 12 Verilator tests PASSED\n");
    else printf("\n%d test(s) FAILED\n",fail_count);

    delete dut;
    return fail_count?1:0;
}
