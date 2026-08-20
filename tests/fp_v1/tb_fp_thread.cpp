#include "VFpThread.h"
#include <cstdio>
#include <cstring>
static VFpThread dut;
static uint32_t b32(float f){ uint32_t u; memcpy(&u,&f,4); return u; }
static float f32(uint32_t u){ float f; memcpy(&f,&u,4); return f; }
static void tick(){ dut.clk=0; dut.eval(); dut.clk=1; dut.eval(); }
int main(){
    dut.rst=1; dut.go=0; dut.x=0; tick(); dut.rst=0;
    dut.go=1; dut.x=b32(2.0f);
    float last = 0;
    for(int i=0;i<8;i++){ tick(); last = f32(dut.y); }
    // audited sequence: +2, *2 alternating with waits -> 2,4,6,12,14,28,30,60
    int ok = last==60.0f;
    printf("FP_THREAD: %s y=%g\n", ok?"PASS":"FAIL", last);
    return ok?0:1;
}
