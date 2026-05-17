#include "VThreadAxiReadBeatInterleave.h"

#include <array>
#include <cstdio>
#include <cstdlib>

static VThreadAxiReadBeatInterleave dut;

static void eval_low() {
    dut.clk = 0;
    dut.eval();
}

static void tick() {
    dut.clk = 0;
    dut.eval();
    dut.clk = 1;
    dut.eval();
}

static void reset() {
    dut.rst = 1;
    dut.start = 0;
    dut.r_ready = 0;
    for (int i = 0; i < 4; ++i) {
        tick();
    }
    dut.rst = 0;
    tick();
}

int main() {
    reset();

    dut.r_ready = 1;
    dut.start = 1;
    tick();
    dut.start = 0;

    std::array<unsigned, 8> ids{};
    std::array<unsigned, 8> last{};
    std::array<unsigned, 8> data{};
    int beats = 0;

    for (int cycle = 0; cycle < 80; ++cycle) {
        eval_low();
        if (dut.r_valid && dut.r_ready) {
            if (beats >= 8) {
                std::printf("FAIL: extra beat id=%u last=%u data=0x%08x\n",
                            static_cast<unsigned>(dut.r_id),
                            static_cast<unsigned>(dut.r_last),
                            static_cast<unsigned>(dut.r_data));
                return 1;
            }
            ids[beats] = static_cast<unsigned>(dut.r_id);
            last[beats] = static_cast<unsigned>(dut.r_last);
            data[beats] = static_cast<unsigned>(dut.r_data);
            ++beats;
        }
        if (beats == 8) {
            break;
        }
        tick();
    }

    if (beats != 8) {
        std::printf("FAIL: beats=%d\n", beats);
        return 1;
    }

    unsigned per_id_beat[2] = {0, 0};
    for (int i = 0; i < 8; ++i) {
        if (ids[i] > 1) {
            std::printf("FAIL: beat %d has unexpected id=%u\n", i, ids[i]);
            return 1;
        }
        if (i != 0 && ids[i] == ids[i - 1]) {
            std::printf("FAIL: beat %d did not interleave, ids[%d]=ids[%d]=%u\n",
                        i, i - 1, i, ids[i]);
            return 1;
        }
        unsigned expected_beat = per_id_beat[ids[i]]++;
        unsigned expected_data = (ids[i] << 8) + expected_beat;
        unsigned expected_last = (expected_beat == 3) ? 1u : 0u;
        if (data[i] != expected_data || last[i] != expected_last) {
            std::printf("FAIL: beat %d got id=%u data=0x%08x last=%u; expected data=0x%08x last=%u\n",
                        i, ids[i], data[i], last[i],
                        expected_data, expected_last);
            return 1;
        }
    }
    if (per_id_beat[0] != 4 || per_id_beat[1] != 4) {
        std::printf("FAIL: lane counts id0=%u id1=%u\n", per_id_beat[0], per_id_beat[1]);
        return 1;
    }

    std::printf("PASS beat interleave alternating ids=");
    for (int i = 0; i < 8; ++i) {
        std::printf("%u", ids[i]);
    }
    std::printf("\n");
    return 0;
}
