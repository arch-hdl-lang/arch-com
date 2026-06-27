#include "VFpSat.h"
#include <cstdio>
#include <cstring>
static VFpSat dut; static int pass=0,fail=0;
static uint32_t b32(float f){uint32_t u;memcpy(&u,&f,4);return u;}
#define CHECK(c,m,...) do{if(c){printf("  PASS: " m "\n",##__VA_ARGS__);++pass;}else{printf("  FAIL: " m "\n",##__VA_ARGS__);++fail;}}while(0)
int main(){
  dut.f=b32(1000.0f); dut.eval();
  CHECK((int8_t)dut.s8==127,  "1000->s8 saturates 127 (got %d)",(int)(int8_t)dut.s8);
  CHECK((uint8_t)dut.u8==255, "1000->u8 saturates 255 (got %u)",(unsigned)(uint8_t)dut.u8);
  dut.f=b32(-5.0f); dut.eval();
  CHECK((int8_t)dut.s8==-5,  "-5->s8 = -5 (got %d)",(int)(int8_t)dut.s8);
  CHECK((uint8_t)dut.u8==0,  "-5->u8 saturates 0 (got %u)",(unsigned)(uint8_t)dut.u8);
  dut.f=0x7FC00000u; dut.eval();
  CHECK((int8_t)dut.s8==127, "NaN->s8 = max (got %d)",(int)(int8_t)dut.s8);
  CHECK((int32_t)dut.s32==2147483647, "NaN->s32 = INT32_MAX (got %d)",(int32_t)dut.s32);
  dut.f=b32(2.7f); dut.eval();
  CHECK((int8_t)dut.s8==2, "2.7->s8 = 2 trunc (got %d)",(int)(int8_t)dut.s8);
  printf("=== %d pass / %d fail ===\n",pass,fail); return fail==0?0:1;
}
