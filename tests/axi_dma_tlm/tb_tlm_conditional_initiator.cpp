#include "VTlmConditionalInitiator.h"

#include <cstdint>
#include <cstdio>

static VTlmConditionalInitiator dut;
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
    dut.sel = 0;
    dut.m_read_req_ready = 0;
    dut.m_read_rsp_valid = 0;
    dut.m_read_rsp_data = 0;
    for (int i = 0; i < 4; ++i) {
        tick();
    }
    dut.rst = 0;
    tick();
}

static uint32_t response_for(uint32_t addr) {
    return 0xC0000000u | (addr & 0x0000FFFFu);
}

int main() {
    reset();

    bool pending = false;
    uint32_t pending_rsp = 0;
    bool saw_else_req = false;
    bool saw_then_req = false;

    for (int c = 0; c < 80; ++c) {
        dut.sel = saw_else_req ? 1 : 0;
        dut.m_read_req_ready = 1;
        dut.m_read_rsp_valid = pending ? 1 : 0;
        dut.m_read_rsp_data = pending_rsp;

        eval_comb();

        if (dut.m_read_req_valid && dut.m_read_req_ready) {
            if (pending) {
                std::printf("FAIL: overlapping conditional request at cycle %d\n", cycle_count);
                return 1;
            }
            const uint32_t addr = dut.m_read_addr;
            if (!saw_else_req) {
                if (addr != 0x00002000u) {
                    std::printf("FAIL: first branch addr=0x%08x expected else addr\n", addr);
                    return 1;
                }
                saw_else_req = true;
            } else if (!saw_then_req) {
                if (addr != 0x00001000u) {
                    std::printf("FAIL: second branch addr=0x%08x expected then addr\n", addr);
                    return 1;
                }
                saw_then_req = true;
            }
            pending_rsp = response_for(addr);
            pending = true;
        }

        if (dut.m_read_rsp_valid && dut.m_read_rsp_ready) {
            pending = false;
        }

        tick();
        eval_comb();

        if (saw_else_req && saw_then_req
            && dut.else_seen_out
            && dut.then_seen_out
            && dut.data_out == response_for(0x00001000u)) {
            std::printf("PASS TlmConditionalInitiator data=0x%08x\n", dut.data_out);
            return 0;
        }
    }

    std::printf("FAIL timeout else_req=%d then_req=%d else_seen=%u then_seen=%u data=0x%08x\n",
                saw_else_req, saw_then_req,
                static_cast<unsigned>(dut.else_seen_out),
                static_cast<unsigned>(dut.then_seen_out),
                static_cast<unsigned>(dut.data_out));
    return 1;
}
