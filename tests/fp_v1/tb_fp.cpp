// Testbench for FpArith — verifies FP32/BF16 arithmetic, fma, is_nan, and
// conversions against host IEEE-754 reference values.
#include "VFpArith.h"
#include <cstdio>
#include <cstring>
#include <cmath>
static VFpArith dut;
static int pass=0, fail=0;
static uint32_t b32(float f){ uint32_t u; memcpy(&u,&f,4); return u; }
static float f32(uint32_t u){ float f; memcpy(&f,&u,4); return f; }
static uint16_t bf16(float f){ uint32_t u=b32(f); uint32_t lsb=(u>>16)&1; u+=0x7FFF+lsb; return (uint16_t)(u>>16); }
static float bf2f(uint16_t h){ return f32(((uint32_t)h)<<16); }
#define CHECK(c,m,...) do{ if(c){printf("  PASS: " m "\n",##__VA_ARGS__);++pass;} else {printf("  FAIL: " m "\n",##__VA_ARGS__);++fail;} }while(0)
int main(){
    dut.a=b32(1.5f); dut.b=b32(2.25f); dut.c=b32(0.5f);
    dut.ha=bf16(1.5f); dut.hb=bf16(0.5f);
    dut.i=-7;
    dut.eval();
    CHECK(f32(dut.sum)==3.75f,  "FP32 1.5+2.25=3.75 (got %g)", f32(dut.sum));
    CHECK(f32(dut.diff)==-0.75f, "FP32 1.5-2.25=-0.75 (got %g)", f32(dut.diff));
    CHECK(f32(dut.prod)==3.375f, "FP32 1.5*2.25=3.375 (got %g)", f32(dut.prod));
    CHECK(f32(dut.fused)==fmaf(1.5f,2.25f,0.5f), "FP32 fma=%g (got %g)", fmaf(1.5f,2.25f,0.5f), f32(dut.fused));
    CHECK(dut.a_gt_b==0, "1.5 > 2.25 is false (got %u)", (unsigned)dut.a_gt_b);
    CHECK(dut.a_is_nan==0, "1.5 is not NaN (got %u)", (unsigned)dut.a_is_nan);
    CHECK(bf2f(dut.hsum)==2.0f, "BF16 1.5+0.5=2.0 (got %g)", bf2f(dut.hsum));
    CHECK(dut.a_to_h==bf16(1.5f), "1.5.to_bf16() (got 0x%04X)", (unsigned)dut.a_to_h);
    CHECK(f32(dut.ha_to_f)==1.5f, "bf16(1.5).to_fp32()=1.5 (got %g)", f32(dut.ha_to_f));
    CHECK(f32(dut.i_to_f)==-7.0f, "int(-7).to_fp32()=-7 (got %g)", f32(dut.i_to_f));
    CHECK((int32_t)dut.f_to_i==1, "1.5.to_sint<32>()=1 trunc (got %d)", (int32_t)dut.f_to_i);

    // NaN canonicalization + is_nan
    dut.a=0x7FC00000u; dut.eval();
    CHECK(dut.a_is_nan==1, "0x7FC00000 is NaN (got %u)", (unsigned)dut.a_is_nan);
    // sNaN input gets quieted to canonical qNaN through an op
    dut.a=0x7F800001u; dut.b=b32(1.0f); dut.eval();
    CHECK(dut.sum==0x7FC00000u, "sNaN+1.0 -> canonical qNaN 0x7FC00000 (got 0x%08X)", dut.sum);

    printf("=== %d pass / %d fail ===\n", pass, fail);
    return fail==0?0:1;
}
