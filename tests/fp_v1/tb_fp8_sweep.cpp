// FP8 SV-vs-sim cross-check sweep. This ONE testbench source runs against
// BOTH backends — the arch native sim and the Verilated `arch build` SV
// (their C++ TB APIs are compatible) — and dumps every DUT output to
// fp8_sweep.bin in the working directory. fp_test.rs runs it twice and
// byte-compares the two dumps: the native sim's hand-written C++ helpers and
// the IR-rendered synthesizable SV are independent implementations, so a
// byte-identical dump over the full input space is a strong cross-oracle.
//
// Coverage:
//  - all 2^16 (a,b) pairs for both formats' add/sub/mul/fma (c tied to b)
//    and all compares — exhaustive for every 8-bit binary op;
//  - all 2^8 fp8 widen inputs (covered inside the 2^16 loop via a4/a5);
//  - f32->fp8 narrowing over 3 * 2^25 stratified f32 patterns: every
//    (sign, exponent, mantissa[22:7]) with the low 7 mantissa bits set to
//    all-zero / LSB-only / all-ones — exercising exact, sticky-low and
//    guard+sticky rounding paths across every binade. (The full 2^32 narrow
//    sweep is the long-phase variant: set ARCH_FP8_SWEEP_FULL=1.)
#include "VFp8Arith.h"
#include <cstdio>
#include <cstdlib>
#include <cstdint>
static VFp8Arith dut;
int main(){
    FILE* f = fopen("fp8_sweep.bin", "wb");
    if (!f) { perror("fp8_sweep.bin"); return 2; }
    // 2^16 binary-op sweep (both formats in lockstep).
    for (unsigned a = 0; a < 256; a++) {
        for (unsigned b = 0; b < 256; b++) {
            dut.a4 = a; dut.b4 = b; dut.c4 = b;
            dut.a5 = a; dut.b5 = b;
            dut.f  = 0;
            dut.eval();
            const uint32_t w4 = dut.a4_to_f, w5 = dut.a5_to_f;
            uint8_t rec[15] = {
                (uint8_t)dut.sum4, (uint8_t)dut.diff4, (uint8_t)dut.prod4,
                (uint8_t)dut.fused4,
                (uint8_t)((dut.a4_gt_b4 & 1) | ((dut.a4_is_nan & 1) << 1) |
                          ((dut.a5_lt_b5 & 1) << 2) | ((dut.a5_is_nan & 1) << 3)),
                (uint8_t)dut.sum5, (uint8_t)dut.prod5,
                (uint8_t)(w4 >> 24), (uint8_t)(w4 >> 16), (uint8_t)(w4 >> 8), (uint8_t)w4,
                (uint8_t)(w5 >> 24), (uint8_t)(w5 >> 16), (uint8_t)(w5 >> 8), (uint8_t)w5,
            };
            fwrite(rec, 1, sizeof rec, f);
        }
    }
    // Narrowing sweep: stratified (default) or full 2^32 (long phase).
    const int full = getenv("ARCH_FP8_SWEEP_FULL") != nullptr;
    dut.a4 = 0; dut.b4 = 0; dut.c4 = 0; dut.a5 = 0; dut.b5 = 0;
    if (full) {
        // Full 2^32 narrow sweep: an 8.6 GB dump is impractical, so fold the
        // outputs into an FNV-1a hash and print it — both backends must
        // report the same hash.
        uint64_t h = 1469598103934665603ull;
        uint32_t x = 0;
        do {
            dut.f = x; dut.eval();
            h = (h ^ (uint8_t)dut.f_to_4) * 1099511628211ull;
            h = (h ^ (uint8_t)dut.f_to_5) * 1099511628211ull;
            x++;
        } while (x != 0);
        fclose(f);
        printf("ARCH_FP8_SWEEP: DONE narrow_hash=%016llx\n", (unsigned long long)h);
        return 0;
    } else {
        static const uint32_t low[3] = {0x00u, 0x01u, 0x7Fu};
        for (int v = 0; v < 3; v++) {
            for (uint32_t hi = 0; hi < (1u << 25); hi++) {
                dut.f = (hi << 7) | low[v];
                dut.eval();
                uint8_t rec[2] = { (uint8_t)dut.f_to_4, (uint8_t)dut.f_to_5 };
                fwrite(rec, 1, 2, f);
            }
        }
    }
    fclose(f);
    printf("ARCH_FP8_SWEEP: DONE\n");
    return 0;
}
