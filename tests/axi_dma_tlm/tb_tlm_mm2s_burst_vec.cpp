#include "VTlmMm2sBurstVec.h"

#include <cstdint>
#include <cstdio>

static VTlmMm2sBurstVec dut;
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

static void drive_rsp_vec(uint32_t base) {
    dut.mem_read_burst_rsp_data_0 = base + 0;
    dut.mem_read_burst_rsp_data_1 = base + 1;
    dut.mem_read_burst_rsp_data_2 = base + 2;
    dut.mem_read_burst_rsp_data_3 = base + 3;
}

static void clear_rsp_vec() {
    drive_rsp_vec(0);
}

static void reset() {
    dut.rst = 1;
    dut.base_addr = 0x2000;
    dut.len0_i = 2;
    dut.len1_i = 4;
    dut.mem_read_burst_req_ready = 0;
    dut.mem_read_burst_rsp_valid = 0;
    dut.mem_read_burst_rsp_tag = 0;
    clear_rsp_vec();
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
    uint32_t req_len[2] = {0, 0};
    int req_count = 0;
    bool sent_tag1 = false;
    bool sent_tag0 = false;

    for (int c = 0; c < 50; ++c) {
        dut.mem_read_burst_req_ready = 1;

        // Return the second, longer request first. All four lanes are driven,
        // but the request length tells consumers how many lanes are meaningful.
        if (req_count >= 2 && !sent_tag1) {
            dut.mem_read_burst_rsp_valid = 1;
            dut.mem_read_burst_rsp_tag = req_tag[1];
            drive_rsp_vec(0xB1000000u | req_addr[1]);
        } else if (req_count >= 2 && !sent_tag0) {
            dut.mem_read_burst_rsp_valid = 1;
            dut.mem_read_burst_rsp_tag = req_tag[0];
            drive_rsp_vec(0xB0000000u | req_addr[0]);
        } else {
            dut.mem_read_burst_rsp_valid = 0;
            dut.mem_read_burst_rsp_tag = 0;
            clear_rsp_vec();
        }

        eval_comb();

        if (dut.mem_read_burst_req_valid && dut.mem_read_burst_req_ready && req_count < 2) {
            req_addr[req_count] = dut.mem_read_burst_addr;
            req_len[req_count] = dut.mem_read_burst_len;
            req_tag[req_count] = dut.mem_read_burst_req_tag;
            std::printf("[cycle %2d] req%d addr=0x%08x len=%u tag=%u\n",
                        cycle_count, req_count, req_addr[req_count], req_len[req_count],
                        req_tag[req_count]);
            req_count++;
        }

        if (dut.mem_read_burst_rsp_valid && dut.mem_read_burst_rsp_ready) {
            std::printf("[cycle %2d] rsp tag=%u lane0=0x%08x\n",
                        cycle_count, dut.mem_read_burst_rsp_tag,
                        dut.mem_read_burst_rsp_data_0);
            if (!sent_tag1 && dut.mem_read_burst_rsp_tag == req_tag[1]) {
                sent_tag1 = true;
            } else if (!sent_tag0 && dut.mem_read_burst_rsp_tag == req_tag[0]) {
                sent_tag0 = true;
            }
        }

        tick();

        const uint32_t exp0 = 0xB0000000u | req_addr[0];
        const uint32_t exp1 = 0xB1000000u | req_addr[1];
        if (sent_tag0 && sent_tag1
            && req_len[0] == 2 && req_len[1] == 4
            && dut.data0_0 == exp0 + 0 && dut.data0_1 == exp0 + 1
            && dut.data0_2 == exp0 + 2 && dut.data0_3 == exp0 + 3
            && dut.data1_0 == exp1 + 0 && dut.data1_1 == exp1 + 1
            && dut.data1_2 == exp1 + 2 && dut.data1_3 == exp1 + 3) {
            std::printf("PASS: TlmMm2sBurstVec len0=%u len1=%u data0[0]=0x%08x data1[3]=0x%08x\n",
                        req_len[0], req_len[1], dut.data0_0, dut.data1_3);
            return 0;
        }
    }

    std::printf("FAIL: req_count=%d sent_tag0=%d sent_tag1=%d len0=%u len1=%u data0_0=0x%08x data1_0=0x%08x\n",
                req_count, sent_tag0, sent_tag1, req_len[0], req_len[1],
                dut.data0_0, dut.data1_0);
    return 1;
}
