#include "VFpFsm.h"
#include <cstdio>
#include <cstring>
static VFpFsm dut;
static uint32_t b32(float f){ uint32_t u; memcpy(&u,&f,4); return u; }
static float f32(uint32_t u){ float f; memcpy(&f,&u,4); return f; }
static void tick(){ dut.clk=0; dut.eval(); dut.clk=1; dut.eval(); }
int main(){
    dut.rst=1; dut.run=0; dut.x=0; tick(); dut.rst=0;
    dut.run=1; dut.x=b32(1.5f);
    tick();          // Idle -> Accum
    tick(); tick();  // two accumulations
    int ok = f32(dut.acc)==3.0f;
    printf("FP_FSM: %s acc=%g\n", ok?"PASS":"FAIL", f32(dut.acc));
    return ok?0:1;
}
