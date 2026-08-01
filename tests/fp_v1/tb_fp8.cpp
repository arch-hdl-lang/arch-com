// Testbench for Fp8Arith — verifies FP8 (E4M3 OCP OFP8 + E5M2) arithmetic,
// fma, is_nan, and the conversion surface against hand-computed RNE values.
// All expectations here are profile-independent (no overflow inputs; see
// tb_fp8_prof.cpp for the --fp-compat surface).
//
// E4M3 encodings used (bias 7, value = 1.mmm * 2^(e-7)):
//   1.5   = 0.0111.100 = 0x3C     2.25 = 0.1000.001 = 0x41
//   0.5   = 0.0110.000 = 0x30     3.75 = 0.1000.111 = 0x47
//   -0.75 = 1.0110.100 = 0xB4     3.5  = 0.1000.110 = 0x46
//   4.0   = 0.1001.000 = 0x48     256  = 0.1111.000 = 0x78 (OCP top binade)
//   448   = 0.1111.110 = 0x7E (max finite)   NaN = X.1111.111 = 0x7F
// E5M2 encodings (bias 15):
//   1.5 = 0.01111.10 = 0x3E   0.5 = 0.01110.00 = 0x38   2.0 = 0.10000.00 = 0x40
//   0.75 = 0.01110.10 = 0x3A  +inf = 0x7C   NaN class = 0x7D..0x7F
#include "VFp8Arith.h"
#include <cstdio>
#include <cstring>
static VFp8Arith dut;
static int pass=0, fail=0;
static uint32_t b32(float f){ uint32_t u; memcpy(&u,&f,4); return u; }
static float f32(uint32_t u){ float f; memcpy(&f,&u,4); return f; }
#define CHECK(c,m,...) do{ if(c){printf("  PASS: " m "\n",##__VA_ARGS__);++pass;} else {printf("  FAIL: " m "\n",##__VA_ARGS__);++fail;} }while(0)
int main(){
    // E4M3: 1.5 op 2.25 (all results exactly representable or a clean tie).
    dut.a4=0x3C; dut.b4=0x41; dut.c4=0x30;
    dut.a5=0x3E; dut.b5=0x38;
    dut.f=b32(1.5f);
    dut.eval();
    CHECK(dut.sum4==0x47,  "E4M3 1.5+2.25=3.75 -> 0x47 (got 0x%02X)", (unsigned)dut.sum4);
    CHECK(dut.diff4==0xB4, "E4M3 1.5-2.25=-0.75 -> 0xB4 (got 0x%02X)", (unsigned)dut.diff4);
    // 3.375 is exactly between 3.25 (mant 5) and 3.5 (mant 6): RNE tie ->
    // even mantissa 6 -> 3.5.
    CHECK(dut.prod4==0x46, "E4M3 1.5*2.25=3.375 ties to 3.5 -> 0x46 (got 0x%02X)", (unsigned)dut.prod4);
    // fma: 3.375+0.5=3.875, exactly between 3.75 (mant 7) and 4.0: tie ->
    // even -> 4.0. Single rounding of the exact f32 fma result.
    CHECK(dut.fused4==0x48, "E4M3 fma(1.5,2.25,0.5)=3.875 ties to 4.0 -> 0x48 (got 0x%02X)", (unsigned)dut.fused4);
    CHECK(dut.a4_gt_b4==0, "E4M3 1.5 > 2.25 is false (got %u)", (unsigned)dut.a4_gt_b4);
    CHECK(dut.a4_is_nan==0, "E4M3 1.5 is not NaN (got %u)", (unsigned)dut.a4_is_nan);
    CHECK(f32(dut.a4_to_f)==1.5f, "E4M3 1.5.to_fp32()=1.5 (got %g)", f32(dut.a4_to_f));
    CHECK(dut.sum5==0x40, "E5M2 1.5+0.5=2.0 -> 0x40 (got 0x%02X)", (unsigned)dut.sum5);
    CHECK(dut.prod5==0x3A, "E5M2 1.5*0.5=0.75 -> 0x3A (got 0x%02X)", (unsigned)dut.prod5);
    CHECK(dut.a5_lt_b5==0, "E5M2 1.5 < 0.5 is false (got %u)", (unsigned)dut.a5_lt_b5);
    CHECK(dut.a5_is_nan==0, "E5M2 1.5 is not NaN (got %u)", (unsigned)dut.a5_is_nan);
    CHECK(f32(dut.a5_to_f)==1.5f, "E5M2 1.5.to_fp32()=1.5 (got %g)", f32(dut.a5_to_f));
    CHECK(dut.f_to_4==0x3C, "f32(1.5).to_fp8e4m3()=0x3C (got 0x%02X)", (unsigned)dut.f_to_4);
    CHECK(dut.f_to_5==0x3E, "f32(1.5).to_fp8e5m2()=0x3E (got 0x%02X)", (unsigned)dut.f_to_5);

    // OCP top binade: exponent 15 with mantissa < 7 are FINITE (256..448).
    dut.a4=0x78; dut.eval();
    CHECK(f32(dut.a4_to_f)==256.0f, "E4M3 0x78 is finite 256, not inf (got %g)", f32(dut.a4_to_f));
    CHECK(dut.a4_is_nan==0, "E4M3 0x78 is not NaN (got %u)", (unsigned)dut.a4_is_nan);
    dut.a4=0x7E; dut.eval();
    CHECK(f32(dut.a4_to_f)==448.0f, "E4M3 0x7E is max finite 448 (got %g)", f32(dut.a4_to_f));
    dut.a4=0x7F; dut.eval();
    CHECK(dut.a4_is_nan==1, "E4M3 0x7F is the sole NaN (got %u)", (unsigned)dut.a4_is_nan);

    // E5M2 IEEE-style specials + subnormals.
    dut.a5=0x7C; dut.eval();
    CHECK(dut.a5_to_f==0x7F800000u, "E5M2 0x7C is +inf (got 0x%08X)", (unsigned)dut.a5_to_f);
    CHECK(dut.a5_is_nan==0, "E5M2 +inf is not NaN (got %u)", (unsigned)dut.a5_is_nan);
    dut.a5=0x7E; dut.eval();
    CHECK(dut.a5_is_nan==1, "E5M2 0x7E is NaN (got %u)", (unsigned)dut.a5_is_nan);
    dut.a5=0x01; dut.eval();
    CHECK(f32(dut.a5_to_f)==1.52587890625e-05f, "E5M2 min subnormal 0x01 = 2^-16 (got %g)", f32(dut.a5_to_f));
    dut.a4=0x01; dut.eval();
    CHECK(f32(dut.a4_to_f)==0.001953125f, "E4M3 min subnormal 0x01 = 2^-9 (got %g)", f32(dut.a4_to_f));

    // Narrowing rounds RNE: the 448/464 OCP boundary — 464 is exactly
    // between 448 (mant 6, even) and the overflow threshold: RNE keeps 448.
    dut.f=b32(464.0f); dut.eval();
    CHECK(dut.f_to_4==0x7E, "f32(464) ties DOWN to 448 = 0x7E, not overflow (got 0x%02X)", (unsigned)dut.f_to_4);
    // A non-trivial rounding case: 0.1 -> nearest E4M3 is 0.1015625 (0x1D).
    dut.f=b32(0.1f); dut.eval();
    CHECK(dut.f_to_4==0x1D, "f32(0.1) rounds to 0.1015625 = 0x1D (got 0x%02X)", (unsigned)dut.f_to_4);

    printf("=== %d pass / %d fail ===\n", pass, fail);
    return fail==0?0:1;
}
