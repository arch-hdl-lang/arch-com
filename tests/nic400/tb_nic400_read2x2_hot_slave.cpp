// Hot-slave test: M0 and M1 both target slave 0.
// Verifies:
//   - resource: mutex<round_robin> serializes both masters onto s0.
//   - Both transactions complete (no starvation).
//   - The arbitrated slave_ids carry distinct prefixes (0 for M0, 1 for M1).

#include "VNic400Read2x2.h"
#include <cstdint>
#include <cstdio>

static VNic400Read2x2 dut;

static void tick() {
    dut.clk = 0;
    dut.eval();
    dut.clk = 1;
    dut.eval();
}

int main() {
    dut.rst = 0;
    dut.m0_ar_valid = 0; dut.m1_ar_valid = 0;
    dut.s0_ar_ready = 0; dut.s1_ar_ready = 0;
    dut.s0_r_valid = 0;  dut.s1_r_valid = 0;
    dut.m0_r_ready = 0;  dut.m1_r_ready = 0;
    for (int i = 0; i < 4; ++i) tick();
    dut.rst = 1;
    tick();

    // Both masters drive AR with bit[28]=0 (slave 0).
    dut.m0_ar_valid = 1; dut.m0_ar_addr = 0x00001000; dut.m0_ar_id = 1;
    dut.m0_ar_size = 2; dut.m0_ar_burst = 1;
    dut.m1_ar_valid = 1; dut.m1_ar_addr = 0x00002000; dut.m1_ar_id = 2;
    dut.m1_ar_size = 2; dut.m1_ar_burst = 1;
    dut.s0_ar_ready = 1;

    // Capture the sequence of grants: arbiter should serialize them.
    int got_m0 = 0, got_m1 = 0;
    uint32_t first_id = 0, second_id = 0;
    int phase = 0;
    for (int i = 0; i < 24; ++i) {
        tick();
        if (dut.s0_ar_valid && dut.s0_ar_ready) {
            uint32_t id = dut.s0_ar_id;
            // m0_id=1 → prefixed = 0b0_001 = 1; m1_id=2 → prefixed = 0b1_010 = 10
            if (phase == 0) {
                first_id = id;
                phase = 1;
                // The losing master keeps requesting; the winner drops below.
                if (id == 1) {
                    dut.m0_ar_valid = 0;
                    got_m0 = 1;
                } else if (id == 10) {
                    dut.m1_ar_valid = 0;
                    got_m1 = 1;
                } else {
                    std::printf("FAIL hot-slave: unexpected first id=0x%x\n", id);
                    return 1;
                }
            } else if (phase == 1) {
                second_id = id;
                phase = 2;
                if (id == 1) {
                    if (got_m0) {
                        std::printf("FAIL hot-slave: M0 granted twice before M1\n");
                        return 1;
                    }
                    got_m0 = 1;
                    dut.m0_ar_valid = 0;
                } else if (id == 10) {
                    if (got_m1) {
                        std::printf("FAIL hot-slave: M1 granted twice before M0\n");
                        return 1;
                    }
                    got_m1 = 1;
                    dut.m1_ar_valid = 0;
                } else {
                    std::printf("FAIL hot-slave: unexpected second id=0x%x\n", id);
                    return 1;
                }
            }
            if (phase == 2) break;
        }
    }
    if (!got_m0 || !got_m1) {
        std::printf("FAIL hot-slave: got_m0=%d got_m1=%d (no fairness or starvation)\n",
                    got_m0, got_m1);
        return 1;
    }
    if (first_id == second_id) {
        std::printf("FAIL hot-slave: round-robin didn't alternate (both id=0x%x)\n", first_id);
        return 1;
    }

    std::printf("PASS Nic400Read2x2 hot-slave: arb serialized M0 (id=0x%x) and M1 (id=0x%x)\n",
                first_id, second_id);
    return 0;
}
