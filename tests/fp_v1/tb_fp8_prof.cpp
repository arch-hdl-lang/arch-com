// Profile probe for FP8 --fp-compat: prints f32->fp8 narrowing results for
// +overflow, -overflow, ±inf, and NaN inputs. Expected bytes depend on the
// compile-time --fp-compat profile (checked by fp_test.rs).
#include "VFp8Prof.h"
#include <cstdio>
#include <cstring>
static VFp8Prof dut;
static uint32_t b32(float f){ uint32_t u; memcpy(&u,&f,4); return u; }
int main(){
    dut.f=b32(1000000.0f); dut.eval();
    printf("povf4=0x%02X povf5=0x%02X\n", (unsigned)dut.o4, (unsigned)dut.o5);
    dut.f=b32(-1000000.0f); dut.eval();
    printf("novf4=0x%02X novf5=0x%02X\n", (unsigned)dut.o4, (unsigned)dut.o5);
    dut.f=0x7F800000u; dut.eval();   // +inf
    printf("pinf4=0x%02X pinf5=0x%02X\n", (unsigned)dut.o4, (unsigned)dut.o5);
    dut.f=0xFF800000u; dut.eval();   // -inf
    printf("ninf4=0x%02X ninf5=0x%02X\n", (unsigned)dut.o4, (unsigned)dut.o5);
    dut.f=0x7FC00000u; dut.eval();   // NaN
    printf("nan4=0x%02X nan5=0x%02X\n", (unsigned)dut.o4, (unsigned)dut.o5);
    // Boundary values: 480 overflows E4M3 (>= max-finite + half-ULP).
    // 57344 is the E5M2 max finite (exact). 61440 = (57344+65536)/2 ties
    // between max finite (odd significand) and 2^16 (even): RNE picks the
    // even side -> overflow in both profiles.
    dut.f=b32(480.0f); dut.eval();
    printf("b480_4=0x%02X\n", (unsigned)dut.o4);
    dut.f=b32(57344.0f); dut.eval();
    printf("max5=0x%02X\n", (unsigned)dut.o5);
    dut.f=b32(61440.0f); dut.eval();
    printf("tie5=0x%02X\n", (unsigned)dut.o5);
    return 0;
}
