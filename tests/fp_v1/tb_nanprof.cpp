// Profile probe for --fp-compat. Feeds a=b=+inf so (a-b) is NaN, then prints
// the canonical NaN bit pattern and the NaN->int result. The expected values
// depend on the compile-time --fp-compat profile (checked by fp_test.rs).
#include "VNanProf.h"
#include <cstdio>
#include <cstdint>
static VNanProf dut;
int main(){
    dut.a = 0x7F800000u; // +inf
    dut.b = 0x7F800000u; // +inf
    dut.eval();
    printf("nan_out=0x%08X nan_to_int=%d\n",
           (unsigned)dut.nan_out, (int)dut.nan_to_int);
    return 0;
}
