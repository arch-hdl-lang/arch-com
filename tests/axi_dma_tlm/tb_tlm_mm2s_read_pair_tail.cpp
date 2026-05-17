#include "VTlmMm2sReadPairTail.h"

#include <cstdint>
#include <cstdio>

static VTlmMm2sReadPairTail dut;
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
    dut.base_addr = 0x2000;
    dut.choose_xor = 0;
    dut.mem_read_req_ready = 0;
    dut.mem_read_rsp_valid = 0;
    dut.mem_read_rsp_data = 0;
    dut.mem_read_rsp_tag = 0;
    for (int i = 0; i < 4; ++i) {
        tick();
    }
    dut.rst = 0;
    tick();
}

int main() {
    reset();

    uint32_t req_addr[2] = {0, 0};
    uint32_t req_tag[2] = {0, 0};
    int req_count = 0;
    bool sent_tag1 = false;
    bool sent_tag0 = false;

    for (int c = 0; c < 50; ++c) {
        dut.mem_read_req_ready = 1;

        // Return responses out of order so the tail must consume values
        // routed through compiler-managed OOO tags.
        if (req_count >= 2 && !sent_tag1) {
            dut.mem_read_rsp_valid = 1;
            dut.mem_read_rsp_tag = req_tag[1];
            dut.mem_read_rsp_data = 0x01000000u | req_addr[1];
        } else if (req_count >= 2 && !sent_tag0) {
            dut.mem_read_rsp_valid = 1;
            dut.mem_read_rsp_tag = req_tag[0];
            dut.mem_read_rsp_data = 0x02000000u | req_addr[0];
        } else {
            dut.mem_read_rsp_valid = 0;
            dut.mem_read_rsp_data = 0;
            dut.mem_read_rsp_tag = 0;
        }

        eval_comb();

        if (dut.mem_read_req_valid && dut.mem_read_req_ready && req_count < 2) {
            req_addr[req_count] = dut.mem_read_addr;
            req_tag[req_count] = dut.mem_read_req_tag;
            std::printf("[cycle %2d] req%d addr=0x%08x tag=%u\n",
                        cycle_count, req_count, req_addr[req_count], req_tag[req_count]);
            req_count++;
        }

        if (dut.mem_read_rsp_valid && dut.mem_read_rsp_ready) {
            std::printf("[cycle %2d] rsp tag=%u data=0x%08x\n",
                        cycle_count, dut.mem_read_rsp_tag, dut.mem_read_rsp_data);
            if (!sent_tag1 && dut.mem_read_rsp_tag == req_tag[1]) {
                sent_tag1 = true;
            } else if (!sent_tag0 && dut.mem_read_rsp_tag == req_tag[0]) {
                sent_tag0 = true;
            }
        }

        tick();

        const uint32_t expected0 = 0x02000000u | req_addr[0];
        const uint32_t expected1 = 0x01000000u | req_addr[1];
        const uint32_t expected_sum = expected0 + expected1;
        if (sent_tag0 && sent_tag1 && dut.done && dut.data0 == expected0
            && dut.data1 == expected1 && dut.checksum == expected_sum) {
            std::printf("PASS TlmMm2sReadPairTail checksum=0x%08x data0=0x%08x data1=0x%08x\n",
                        dut.checksum, dut.data0, dut.data1);
            return 0;
        }
    }

    std::printf("FAIL req_count=%d sent_tag0=%d sent_tag1=%d done=%u checksum=0x%08x data0=0x%08x data1=0x%08x\n",
                req_count, sent_tag0, sent_tag1, dut.done, dut.checksum, dut.data0, dut.data1);
    return 1;
}
