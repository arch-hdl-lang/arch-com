#include "VBf16LitCtx.h"
#include <cstdio>
static VBf16LitCtx dut; static int pass=0,fail=0;
#define CHECK(c,m,...) do{if(c){printf("  PASS: " m "\n",##__VA_ARGS__);++pass;}else{printf("  FAIL: " m "\n",##__VA_ARGS__);++fail;}}while(0)
static void tick(){ dut.clk=0; dut.eval(); dut.clk=1; dut.eval(); }
int main(){
  dut.rst=1; dut.a=0; tick();
  dut.rst=0;
  // `a` = bf16(0.4) = 0x3ECD, comparison `a > 0.5` should be false.
  dut.a=0x3ECD; tick();
  CHECK((uint16_t)dut.o_init==0x3FC0, "init: bf16(1.5)=0x3FC0 (got 0x%04X)", (unsigned)dut.o_init);
  CHECK((uint16_t)dut.o_let==0x4049, "let: bf16(pi)=0x4049 (got 0x%04X)", (unsigned)dut.o_let);
  CHECK((uint16_t)dut.o_let2==0x3DCD, "let: bf16(0.1)=0x3DCD, RNE not truncation (got 0x%04X)", (unsigned)dut.o_let2);
  CHECK(dut.o_cmp==0, "0.4 > 0.5 is false (got %d)", (int)dut.o_cmp);
  // `a` = bf16(0.6) = 0x3F19, comparison `a > 0.5` should be true.
  dut.a=0x3F19; dut.eval();
  CHECK(dut.o_cmp==1, "0.6 > 0.5 is true (got %d)", (int)dut.o_cmp);
  printf("=== %d pass / %d fail ===\n",pass,fail); return fail==0?0:1;
}
