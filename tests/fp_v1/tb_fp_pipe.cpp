#include "VFpPipe.h"
#include <cstdio>
#include <cstring>
static VFpPipe dut;
static uint32_t b32(float f){ uint32_t u; memcpy(&u,&f,4); return u; }
static float f32(uint32_t u){ float f; memcpy(&f,&u,4); return f; }
static void tick(){ dut.clk=0; dut.eval(); dut.clk=1; dut.eval(); }
int main(){
    dut.rst=1; dut.a_in=0; dut.b_in=0; tick(); dut.rst=0;
    dut.a_in=b32(2.0f); dut.b_in=b32(3.0f);
    tick(); tick(); tick();
    int ok = f32(dut.y_out)==9.0f;
    printf("FP_PIPE: %s y=%g\n", ok?"PASS":"FAIL", f32(dut.y_out));
    return ok?0:1;
}
