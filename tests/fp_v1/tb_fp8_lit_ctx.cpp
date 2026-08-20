#include "VFp8LitCtx.h"
#include <cstdio>
#include <cstdint>
static VFp8LitCtx dut; static int pass=0,fail=0;
#define CHECK(c,m,...) do{if(c){printf("  PASS: " m "\n",##__VA_ARGS__);++pass;}else{printf("  FAIL: " m "\n",##__VA_ARGS__);++fail;}}while(0)
static void tick(){ dut.clk=0; dut.eval(); dut.clk=1; dut.eval(); }
int main(){
  dut.rst=1; dut.a=0; tick();
  dut.rst=0;
  // `a` = E4M3 0.4375 (0.0101.110 = 0x2E), `a > 0.5` should be false.
  dut.a=0x2E; tick();
  CHECK((uint8_t)dut.o_init==0x3C, "init: e4m3(1.5)=0x3C (got 0x%02X)", (unsigned)dut.o_init);
  CHECK((uint8_t)dut.o_rst==0x3A,  "reset: e5m2(0.75)=0x3A (got 0x%02X)", (unsigned)dut.o_rst);
  CHECK((uint8_t)dut.o_let==0x46,  "let: e4m3(3.375) ties to 3.5=0x46 (got 0x%02X)", (unsigned)dut.o_let);
  CHECK((uint8_t)dut.o_let5==0x2E, "let: e5m2(0.1)=0.09375=0x2E, RNE (got 0x%02X)", (unsigned)dut.o_let5);
  CHECK(dut.o_cmp==0, "0.4375 > 0.5 is false (got %d)", (int)dut.o_cmp);
  // `a` = E4M3 1.5 (0x3C), `a > 0.5` should be true.
  dut.a=0x3C; dut.eval();
  CHECK(dut.o_cmp==1, "1.5 > 0.5 is true (got %d)", (int)dut.o_cmp);
  printf("=== %d pass / %d fail ===\n",pass,fail); return fail==0?0:1;
}
