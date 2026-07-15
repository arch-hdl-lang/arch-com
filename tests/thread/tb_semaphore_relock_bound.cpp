// TB for semaphore_relock_bound.arch (arch#696 regression, concurrency bound).
// Four lanes re-lock a semaphore<2>; each holds until its TB-driven relN. The TB
// asserts the ≤2 concurrency bound EVERY cycle, that the pool actually reaches 2
// concurrent holders, and drives 60 rounds of release+re-admit churn (the path
// the #696 fix newly exercises). Complements tb_semaphore_relock.cpp, which
// checks fairness/no-starvation; this one guards the safety bound.
#include "VSemRelockBound.h"
#include <cstdio>
static VSemRelockBound dut;
static int maxc=0, cyc=0;
static int sample(){ int b=(int)dut.busy0+(int)dut.busy1+(int)dut.busy2+(int)dut.busy3;
  if(b>maxc)maxc=b; if(b>2){std::printf("FAIL SemRelockBound: OVER-SUBSCRIBED cyc=%d busy=%d (want<=2)\n",cyc,b);return 1;} return 0; }
static int tick(){ dut.clk=0; dut.eval(); dut.clk=1; dut.eval(); cyc++; return sample(); }
int main(){
  dut.rst=1; dut.rel0=dut.rel1=dut.rel2=dut.rel3=0; if(tick())return 1;
  dut.rst=0;
  for(int i=0;i<20;i++) if(tick()) return 1;
  if(maxc!=2){ std::printf("FAIL SemRelockBound: expected exactly 2 concurrent, got max=%d\n",maxc); return 1; }
  for(int r=0;r<60;r++){
    dut.rel0=dut.busy0; dut.rel1=dut.busy1; dut.rel2=dut.busy2; dut.rel3=dut.busy3;
    if(tick())return 1; if(tick())return 1;
    dut.rel0=dut.rel1=dut.rel2=dut.rel3=0;
    if(tick())return 1;
  }
  std::printf("max_concurrent=%d over %d cycles\n",maxc,cyc);
  std::printf("PASS SemRelockBound (reached 2, never exceeded 2 through churn)\n");
  return 0;
}
