#include "VFpAcc.h"
#include <cstdio>
#include <cstring>
static VFpAcc dut; static int pass=0,fail=0;
static uint32_t b32(float f){uint32_t u;memcpy(&u,&f,4);return u;}
static float f32(uint32_t u){float f;memcpy(&f,&u,4);return f;}
#define CHECK(c,m,...) do{if(c){printf("  PASS: " m "\n",##__VA_ARGS__);++pass;}else{printf("  FAIL: " m "\n",##__VA_ARGS__);++fail;}}while(0)
static void tick(){ dut.clk=0; dut.eval(); dut.clk=1; dut.eval(); }
int main(){
  dut.rst=1; dut.en=0; dut.x=b32(0.0f); tick();
  CHECK(f32(dut.acc)==0.0f, "reset acc=0.0 (got %g)", f32(dut.acc));
  dut.rst=0; dut.en=1;
  dut.x=b32(1.5f); tick();
  dut.x=b32(2.25f); tick();
  dut.x=b32(0.25f); tick();
  CHECK(f32(dut.acc)==4.0f, "1.5+2.25+0.25=4.0 (got %g)", f32(dut.acc));
  printf("=== %d pass / %d fail ===\n",pass,fail); return fail==0?0:1;
}
