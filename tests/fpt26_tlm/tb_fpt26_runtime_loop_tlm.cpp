#include "VFpt26RuntimeLoopTlm.h"

#include <cstdint>
#include <cstdio>

static VFpt26RuntimeLoopTlm dut;
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

static uint32_t hbm_token(uint32_t tile) {
    return 0x11000000u | (tile & 0xfu);
}

static uint32_t qk_token(uint32_t head, uint32_t tile) {
    return 0x21000000u | ((head & 0x3u) << 4) | (tile & 0xfu);
}

static void reset() {
    dut.rst = 1;
    dut.qk_limit = 4;
    dut.hbm_read_k_req_ready = 0;
    dut.hbm_read_k_rsp_valid = 0;
    dut.hbm_read_k_rsp_data = 0;
    dut.qk_qk_tile_req_ready = 0;
    dut.qk_qk_tile_rsp_valid = 0;
    dut.qk_qk_tile_rsp_data = 0;
    for (int i = 0; i < 4; ++i) {
        tick();
    }
    dut.rst = 0;
    tick();
}

int main() {
    reset();

    bool hbm_pending = false;
    bool qk_pending = false;
    uint32_t hbm_rsp = 0;
    uint32_t qk_rsp = 0;
    int hbm_reqs = 0;
    int qk_reqs = 0;

    constexpr uint32_t limit = 4;
    uint32_t expected_checksum = 0;
    uint32_t expected_scores[4] = {0, 0, 0, 0};
    for (uint32_t i = 0; i < limit; ++i) {
        const uint32_t h = hbm_token(i);
        const uint32_t q = qk_token(i & 0x3u, i);
        expected_checksum += h;
        expected_checksum += q;
        expected_scores[i & 0x3u] += q;
    }

    for (int c = 0; c < 200; ++c) {
        dut.hbm_read_k_req_ready = 1;
        dut.qk_qk_tile_req_ready = 1;
        dut.hbm_read_k_rsp_valid = hbm_pending ? 1 : 0;
        dut.hbm_read_k_rsp_data = hbm_rsp;
        dut.qk_qk_tile_rsp_valid = qk_pending ? 1 : 0;
        dut.qk_qk_tile_rsp_data = qk_rsp;

        eval_comb();

        if (dut.hbm_read_k_req_valid && dut.hbm_read_k_req_ready) {
            if (hbm_pending) {
                std::printf("FAIL: overlapping HBM request at cycle %d\n", cycle_count);
                return 1;
            }
            if (dut.hbm_read_k_kv_head != 0 || dut.hbm_read_k_tile != static_cast<uint32_t>(hbm_reqs)) {
                std::printf("FAIL: bad HBM request %d kv=%u tile=%u\n",
                            hbm_reqs, dut.hbm_read_k_kv_head, dut.hbm_read_k_tile);
                return 1;
            }
            hbm_rsp = hbm_token(dut.hbm_read_k_tile);
            hbm_pending = true;
            hbm_reqs++;
        }

        if (dut.qk_qk_tile_req_valid && dut.qk_qk_tile_req_ready) {
            if (qk_pending) {
                std::printf("FAIL: overlapping QK request at cycle %d\n", cycle_count);
                return 1;
            }
            const uint32_t idx = static_cast<uint32_t>(qk_reqs);
            if (dut.qk_qk_tile_head != (idx & 0x3u)
                || dut.qk_qk_tile_kv_head != 0
                || dut.qk_qk_tile_tile != idx
                || dut.qk_qk_tile_k_token != hbm_token(idx)) {
                std::printf("FAIL: bad QK request %d head=%u kv=%u tile=%u k=0x%08x\n",
                            qk_reqs, dut.qk_qk_tile_head, dut.qk_qk_tile_kv_head,
                            dut.qk_qk_tile_tile, dut.qk_qk_tile_k_token);
                return 1;
            }
            qk_rsp = qk_token(dut.qk_qk_tile_head, dut.qk_qk_tile_tile);
            qk_pending = true;
            qk_reqs++;
        }

        if (dut.hbm_read_k_rsp_valid && dut.hbm_read_k_rsp_ready) {
            hbm_pending = false;
        }
        if (dut.qk_qk_tile_rsp_valid && dut.qk_qk_tile_rsp_ready) {
            qk_pending = false;
        }

        tick();
        eval_comb();

        if (dut.done_out) {
            const bool pass = hbm_reqs == static_cast<int>(limit)
                && qk_reqs == static_cast<int>(limit)
                && dut.hbm_calls_out == limit
                && dut.qk_calls_out == limit
                && dut.checksum_out == expected_checksum
                && dut.score0_out == expected_scores[0]
                && dut.score1_out == expected_scores[1]
                && dut.score2_out == expected_scores[2]
                && dut.score3_out == expected_scores[3];
            if (pass) {
                std::printf("PASS Fpt26RuntimeLoopTlm hbm=%d qk=%d checksum=0x%08x\n",
                            hbm_reqs, qk_reqs, dut.checksum_out);
                return 0;
            }
            std::printf("FAIL done mismatch hbm=%d qk=%d hbm_out=%u qk_out=%u checksum=0x%08x exp=0x%08x scores=%08x,%08x,%08x,%08x\n",
                        hbm_reqs, qk_reqs, dut.hbm_calls_out, dut.qk_calls_out,
                        dut.checksum_out, expected_checksum,
                        dut.score0_out, dut.score1_out, dut.score2_out, dut.score3_out);
            return 1;
        }
    }

    std::printf("FAIL timeout hbm=%d qk=%d done=%u checksum=0x%08x\n",
                hbm_reqs, qk_reqs, dut.done_out, dut.checksum_out);
    return 1;
}
