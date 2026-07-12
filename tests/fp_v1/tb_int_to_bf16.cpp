// Pinned characterization testbench for int.to_bf16() (issue #629).
//
// int.to_bf16() is DECLARED as f32-routed: narrow_bf16(f32(i)), the same
// convention as bf16 fma's f32-accumulate (PR #627). This is a double
// rounding and is NOT correctly-rounded int->bf16 for |i| >= 2^24. That
// behavior is intentional (see doc/ARCH_HDL_Specification.md §3.8 "Rounding
// convention" and doc/proposal_fp_rounding_semantics.md) -- if this test ever
// fails after a codegen change, the semantics changed and the decision in
// issue #629 needs to be revisited (and the spec updated), not just this
// test's expected values.
#include "VIntToBf16.h"
#include <cstdio>
static VIntToBf16 dut;
static int pass = 0, fail = 0;
#define CHECK(c, m, ...)                                    \
    do {                                                    \
        if (c) {                                            \
            printf("  PASS: " m "\n", ##__VA_ARGS__);        \
            ++pass;                                         \
        } else {                                             \
            printf("  FAIL: " m "\n", ##__VA_ARGS__);        \
            ++fail;                                          \
        }                                                    \
    } while (0)

int main() {
    // Witness (issue #629): i = 2^24 + 2^16 + 1 = 16842753. (float)i ties-to-
    // even *down* to 16842752, the exact bf16 midpoint between 0x4b80 and
    // 0x4b81; the f32->bf16 narrow then ties-to-even to the even significand
    // 0x4b80. Direct correctly-rounded int->bf16 would instead see
    // 16842753 > 16842752 (the midpoint) and round *up* to 0x4b81 -- 1 bf16
    // ULP away from the f32-routed result locked here.
    dut.i = 16842753;
    dut.eval();
    CHECK(dut.h == 0x4b80,
          "int(16842753).to_bf16() is f32-routed = 0x4b80 (got 0x%04X)",
          (unsigned)dut.h);

    // Exact case below 2^24: no double-rounding hazard -- f32-routed and
    // correctly-rounded bf16 agree bit-for-bit. i=1000 -> f32(1000.0) is
    // exact -> single RNE narrow to bf16 -> 0x447a.
    dut.i = 1000;
    dut.eval();
    CHECK(dut.h == 0x447a,
          "int(1000).to_bf16() (exact, |i|<2^24) = 0x447a (got 0x%04X)",
          (unsigned)dut.h);

    printf("=== %d pass / %d fail ===\n", pass, fail);
    return fail == 0 ? 0 : 1;
}
