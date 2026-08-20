// Conversion-matrix TB (riscv profile expectations, all hand-computed):
// every value is exact or single-rounded per the CR argument in the PR.
#include "VFp8Convert.h"
#include <cstdio>
#include <cstring>
static VFp8Convert dut;
static int pass=0, fail=0;
#define CHECK(c,m,...) do{ if(c){printf("  PASS: " m "\n",##__VA_ARGS__);++pass;} else {printf("  FAIL: " m "\n",##__VA_ARGS__);++fail;} }while(0)
int main(){
    dut.q4=0x3C; dut.q5=0x3E; dut.h=0x4058; dut.i=(uint16_t)(int16_t)-7; dut.u=1000;
    dut.eval();
    CHECK(dut.q4_h==0x3FC0, "e4m3(1.5)->bf16 0x3FC0 exact (got 0x%04X)", (unsigned)dut.q4_h);
    CHECK(dut.q5_h==0x3FC0, "e5m2(1.5)->bf16 0x3FC0 exact (got 0x%04X)", (unsigned)dut.q5_h);
    CHECK(dut.h_q4==0x46, "bf16(3.375)->e4m3 ties to 3.5=0x46 (got 0x%02X)", (unsigned)dut.h_q4);
    CHECK(dut.i_q4==0xCE, "sint(-7)->e4m3 -7=0xCE exact (got 0x%02X)", (unsigned)dut.i_q4);
    CHECK(dut.u_q5==0x64, "uint(1000)->e5m2 rounds to 1024=0x64 (got 0x%02X)", (unsigned)dut.u_q5);
    CHECK(dut.q4_u==1 && dut.q4_s==1, "e4m3(1.5)->int trunc 1 (got %u/%d)", (unsigned)dut.q4_u, (int)(int8_t)dut.q4_s);
    CHECK(dut.q5_u==1, "e5m2(1.5)->uint 1 (got %u)", (unsigned)dut.q5_u);
    CHECK(dut.x45==0x3E, "e4m3(1.5)->e5m2 0x3E exact (got 0x%02X)", (unsigned)dut.x45);
    CHECK(dut.x54==0x3C, "e5m2(1.5)->e4m3 0x3C exact (got 0x%02X)", (unsigned)dut.x54);

    // saturation + NaN + cross-overflow (riscv)
    dut.q4=0x7E; dut.q5=0x7C; dut.eval();       // 448 / +inf
    CHECK(dut.q4_u==255 && (int8_t)dut.q4_s==127, "448 saturates to_uint<8>=255 to_sint<8>=127 (got %u/%d)", (unsigned)dut.q4_u, (int)(int8_t)dut.q4_s);
    CHECK(dut.q5_u==65535, "+inf -> to_uint<16> saturates 65535 (got %u)", (unsigned)dut.q5_u);
    CHECK(dut.x54==0x7F, "e5m2 +inf -> e4m3 NaN 0x7F riscv (got 0x%02X)", (unsigned)dut.x54);
    dut.q4=0x7F; dut.eval();                    // e4m3 NaN
    CHECK(dut.q4_u==255, "e4m3 NaN -> to_uint<8> type-max riscv (got %u)", (unsigned)dut.q4_u);
    CHECK(dut.q4_h==0x7FC0, "e4m3 NaN -> bf16 canonical 0x7FC0 (got 0x%04X)", (unsigned)dut.q4_h);
    // e4m3 1.875 (3 mant bits) -> e5m2 (2 bits): tie to even -> 2.0
    dut.q4=0x3F; dut.eval();
    CHECK(dut.x45==0x40, "e4m3(1.875)->e5m2 ties to 2.0=0x40 (got 0x%02X)", (unsigned)dut.x45);
    // e5m2 1024 -> e4m3 overflow -> NaN (riscv, sign dropped)
    dut.q5=0x64; dut.eval();
    CHECK(dut.x54==0x7F, "e5m2(1024)->e4m3 overflows to NaN riscv (got 0x%02X)", (unsigned)dut.x54);
    printf("=== %d pass / %d fail ===\n", pass, fail);
    return fail==0?0:1;
}
