#include "VTlmIndexedBurstTarget.h"

#include <cstdio>
#include <cstdlib>

static VTlmIndexedBurstTarget dut;

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
    dut.s_read_burst_req_valid = 0;
    dut.s_read_burst_addr = 0;
    dut.s_read_burst_len = 0;
    dut.s_read_burst_req_tag = 0;
    dut.s_read_burst_rsp_ready = 0;
    for (int i = 0; i < 4; ++i) {
        tick();
    }
    dut.rst = 0;
    tick();
}

static void issue(unsigned tag, unsigned addr) {
    dut.s_read_burst_addr = addr;
    dut.s_read_burst_len = 2;
    dut.s_read_burst_req_tag = tag;
    dut.s_read_burst_req_valid = 1;
    for (int i = 0; i < 8; ++i) {
        eval_low();
        if (dut.s_read_burst_req_ready) {
            tick();
            dut.s_read_burst_req_valid = 0;
            eval_low();
            return;
        }
        tick();
    }
    std::printf("FAIL: request tag %u was not accepted\n", tag);
    std::exit(1);
}

int main() {
    reset();
    dut.s_read_burst_rsp_ready = 0;
    issue(0, 0x1000);
    issue(1, 0x2000);

    int seen0 = 0;
    int seen1 = 0;
    for (int cycle = 0; cycle < 40; ++cycle) {
        eval_low();
        if (dut.s_read_burst_rsp_valid) {
            dut.s_read_burst_rsp_ready = 1;
            unsigned tag = dut.s_read_burst_rsp_tag;
            tick();
            if (tag == 0) {
                seen0 = 1;
            }
            if (tag == 1) {
                seen1 = 1;
            }
            dut.s_read_burst_rsp_ready = 0;
            if (seen0 && seen1) {
                std::printf("PASS indexed response arb tags seen\n");
                return 0;
            }
        } else {
            tick();
        }
    }

    std::printf("FAIL: responses seen0=%d seen1=%d valid=%d tag=%u\n",
                seen0, seen1, static_cast<int>(dut.s_read_burst_rsp_valid),
                static_cast<unsigned>(dut.s_read_burst_rsp_tag));
    return 1;
}
