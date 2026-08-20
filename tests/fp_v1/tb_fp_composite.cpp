// Testbench for FpComposite — pins float semantics for composite positions
// (Vec elements, struct fields, function params). Every expectation is an
// exact value: integer arithmetic on the bit patterns (the pre-v2 hazard)
// produces wildly different results, so any dispatch regression trips.
// Runs against BOTH backends (native sim and Verilated SV).
#include "VFpComposite.h"
#include <cstdio>
#include <cstring>
static VFpComposite dut;
static int pass=0, fail=0;
static uint32_t b32(float f){ uint32_t u; memcpy(&u,&f,4); return u; }
static float f32(uint32_t u){ float f; memcpy(&f,&u,4); return f; }
static uint16_t bf16(float f){ uint32_t u=b32(f); u+=0x7FFF+((u>>16)&1); return (uint16_t)(u>>16); }
static float bf2f(uint16_t h){ return f32(((uint32_t)h)<<16); }
static void tick(){ dut.clk=0; dut.eval(); dut.clk=1; dut.eval(); }
#define CHECK(c,m,...) do{ if(c){printf("  PASS: " m "\n",##__VA_ARGS__);++pass;} else {printf("  FAIL: " m "\n",##__VA_ARGS__);++fail;} }while(0)
int main(){
    dut.rst=1; for(int i=0;i<4;i++) dut.v[i]=b32(0.0f); dut.s=0; tick();
    dut.rst=0;
    for(int i=0;i<4;i++) dut.v[i]=b32(1.0f+i);   // 1,2,3,4
    dut.s=b32(2.0f);
    dut.a_re=bf16(1.0f); dut.a_im=bf16(2.0f);
    dut.b_re=bf16(3.0f); dut.b_im=bf16(4.0f);
    dut.q4=0x3C;                                  // e4m3 1.5
    tick(); tick();
    // Vec elementwise: y[i] = v[i]*2
    CHECK(f32(dut.y[0])==2.0f && f32(dut.y[3])==8.0f, "Vec scale: y0=%g y3=%g", f32(dut.y[0]), f32(dut.y[3]));
    // acc0 = 1+1 = 2; acc1 = fma(2,2,fma(2,2,0)) = 8; dot = 10
    CHECK(f32(dut.dot)==10.0f, "Vec reg accumulate: dot=%g (want 10)", f32(dut.dot));
    // (1+2i)*(3+4i) with b.re bumped +0.5 in the im path:
    // p_re = 1*3 - 2*4 = -5 ; p_im = 1*4 + 2*3.5 = 11
    CHECK(bf2f(dut.p_re)==-5.0f, "struct bf16 re=-5 (got %g)", bf2f(dut.p_re));
    CHECK(bf2f(dut.p_im)==11.0f, "struct bf16 im=11 (got %g)", bf2f(dut.p_im));
    // fp8 function: sat_dbl(1.5) = 3.0 = 0x44
    CHECK(dut.q4n==0x44, "fp8 function 1.5+1.5=3.0 -> 0x44 (got 0x%02X)", (unsigned)dut.q4n);
    // Vec element compare vs coerced literal: 4.0 > 2.5
    CHECK(dut.vmax==1, "v[3]=4.0 > 2.5 (got %d)", (int)dut.vmax);
    printf("=== %d pass / %d fail ===\n", pass, fail);
    return fail==0?0:1;
}
