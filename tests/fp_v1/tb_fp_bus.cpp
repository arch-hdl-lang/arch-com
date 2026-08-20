#include "VFpBusMod.h"
#include <cstdio>
#include <cstring>
static VFpBusMod dut;
static uint32_t b32(float f){ uint32_t u; memcpy(&u,&f,4); return u; }
static float f32(uint32_t u){ float f; memcpy(&f,&u,4); return f; }
int main(){
    dut.s_valid=1; dut.s_data=b32(1.25f);
    dut.eval();
    int ok = f32(dut.m_data)==2.5f;
    printf("FP_BUS: %s m_data=%g\n", ok?"PASS":"FAIL", f32(dut.m_data));
    return ok?0:1;
}
