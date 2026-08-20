#include "VFpTlmTop.h"
#include <cstdio>
#include <cstring>
static VFpTlmTop dut;
static float f32(uint32_t u){ float f; memcpy(&f,&u,4); return f; }
static void tick(){ dut.clk=0; dut.eval(); dut.clk=1; dut.eval(); }
int main(){
    dut.rst=1; dut.go=0; tick(); dut.rst=0; dut.go=1;
    for(int i=0;i<10;i++) tick();
    int ok = f32(dut.y)==7.5f;
    printf("FP_TLM: %s y=%g\n", ok?"PASS":"FAIL", f32(dut.y));
    return ok?0:1;
}
