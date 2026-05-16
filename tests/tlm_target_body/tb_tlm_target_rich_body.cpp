#include "VTlmTargetRichBody.h"

#include <cstdint>
#include <cstdio>

static VTlmTargetRichBody dut;
static int cycle_count = 0;

static void eval_comb() {
    dut.eval();
}

static void tick() {
    dut.clk = 0;
    dut.eval();
    dut.clk = 1;
    dut.eval();
    cycle_count++;
}

static void reset() {
    dut.rst = 1;
    dut.ready_i = 0;
    dut.s_read_req_valid = 0;
    dut.s_read_addr = 0;
    dut.s_read_count = 0;
    dut.s_read_rsp_ready = 0;
    for (int i = 0; i < 4; ++i) {
        tick();
    }
    dut.rst = 0;
    tick();
}

static bool transact(uint32_t addr, uint32_t count, uint32_t expected) {
    bool request_accepted = false;
    dut.s_read_addr = addr;
    dut.s_read_count = count;
    dut.s_read_req_valid = 1;
    dut.s_read_rsp_ready = 0;
    dut.ready_i = 0;

    for (int c = 0; c < 80; ++c) {
        bool drop_req_after_tick = false;
        if (c == 4) {
            dut.ready_i = 1;
        }
        eval_comb();
        if (dut.s_read_req_valid && dut.s_read_req_ready) {
            request_accepted = true;
            drop_req_after_tick = true;
        }
        if (dut.s_read_rsp_valid) {
            if (dut.s_read_rsp_data != expected) {
                std::printf("FAIL rsp addr=0x%08x count=%u got=0x%08x expected=0x%08x cycle=%d\n",
                            addr, count, dut.s_read_rsp_data, expected, cycle_count);
                return false;
            }
            dut.s_read_rsp_ready = 1;
            tick();
            dut.s_read_rsp_ready = 0;
            return request_accepted;
        }
        tick();
        if (drop_req_after_tick) {
            dut.s_read_req_valid = 0;
        }
    }
    std::printf("FAIL timeout addr=0x%08x count=%u accepted=%d cycle=%d\n",
                addr, count, request_accepted, cycle_count);
    return false;
}

int main() {
    reset();

    if (!transact(0x100u, 2, 0x10cu)) {
        return 1;
    }
    for (int i = 0; i < 2; ++i) {
        tick();
    }
    if (!transact(0x101u, 3, 0x118u)) {
        return 1;
    }

    std::printf("PASS TlmTargetRichBody\n");
    return 0;
}
