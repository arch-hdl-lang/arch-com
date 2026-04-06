#include "Vpipelined_adder_32bit.h"
#include <cstdio>
#include <cstdlib>

int main() {
    Vpipelined_adder_32bit dut;

    // Reset
    dut.reset = 1;
    dut.clk = 0;
    dut.A = 0; dut.B = 0; dut.start = 0;
    for (int i = 0; i < 4; i++) {
        dut.clk = 0; dut.eval();
        dut.clk = 1; dut.eval();
    }
    dut.reset = 0;

    // Test cases
    struct TestCase { uint32_t a, b; };
    TestCase tests[] = {
        {0x00000001, 0x00000002},
        {0xFFFFFFFF, 0x00000001},
        {0x12345678, 0x9ABCDEF0},
        {0x00FF00FF, 0xFF00FF00},
        {100, 200},
        {0, 0},
    };
    int n_tests = sizeof(tests) / sizeof(tests[0]);

    // Feed inputs with start=1
    // Collect expected results
    uint32_t expected_S[100];
    uint32_t expected_Co[100];
    for (int i = 0; i < n_tests; i++) {
        uint64_t sum = (uint64_t)tests[i].a + tests[i].b;
        expected_S[i] = (uint32_t)(sum & 0xFFFFFFFF);
        expected_Co[i] = (uint32_t)(sum >> 32);
    }

    // Pipeline latency = 4 cycles
    int total_cycles = n_tests + 4;
    int results_checked = 0;
    int pass = 0, fail = 0;

    for (int cyc = 0; cyc < total_cycles; cyc++) {
        if (cyc < n_tests) {
            dut.A = tests[cyc].a;
            dut.B = tests[cyc].b;
            dut.start = 1;
        } else {
            dut.A = 0; dut.B = 0; dut.start = 0;
        }

        dut.clk = 0; dut.eval();
        dut.clk = 1; dut.eval();

        if (dut.done) {
            int idx = results_checked;
            if (idx < n_tests) {
                if (dut.S == expected_S[idx] && dut.Co == expected_Co[idx]) {
                    printf("PASS test %d: 0x%08X + 0x%08X = 0x%08X (Co=%d)\n",
                        idx, tests[idx].a, tests[idx].b, dut.S, dut.Co);
                    pass++;
                } else {
                    printf("FAIL test %d: 0x%08X + 0x%08X => got S=0x%08X Co=%d, expected S=0x%08X Co=%d\n",
                        idx, tests[idx].a, tests[idx].b, dut.S, dut.Co,
                        expected_S[idx], expected_Co[idx]);
                    fail++;
                }
            }
            results_checked++;
        }
    }

    printf("\n=== %d pass, %d fail ===\n", pass, fail);
    return fail > 0 ? 1 : 0;
}
