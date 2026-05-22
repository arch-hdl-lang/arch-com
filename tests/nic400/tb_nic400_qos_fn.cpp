// C++ runner for Nic400QosFn_test.harc. Mirrors the HARC assertions.

#include "VNic400QosFn.h"

#include <cstdint>
#include <cstdio>

static VNic400QosFn dut;

static void settle() { dut.eval(); }

static int check(const char *name, uint32_t got, uint32_t want) {
    if (got != want) {
        std::printf("FAIL Nic400QosFn[%s]: got 0x%x, expected 0x%x\n", name, got, want);
        return 1;
    }
    return 0;
}

int main() {
    int err = 0;

    // 1. Empty req
    dut.req_mask = 0;
    dut.last_grant = 0;
    dut.qos_packed = 0x3210;
    settle();
    err += check("empty", dut.grant, 0);

    // 2. Single requester m1, qos=0
    dut.req_mask = 0b0010;
    dut.last_grant = 0;
    dut.qos_packed = 0;
    settle();
    err += check("single_m1", dut.grant, 0b0010);

    // 3. m0=qos3, m2=qos5 -> m2
    dut.req_mask = 0b0101;
    dut.last_grant = 0;
    dut.qos_packed = 0x0503;
    settle();
    err += check("qos_pick_m2", dut.grant, 0b0100);

    // 4. All four equal qos=2, last_grant=m0 -> m1
    dut.req_mask = 0b1111;
    dut.last_grant = 0b0001;
    dut.qos_packed = 0x2222;
    settle();
    err += check("rr_after_m0", dut.grant, 0b0010);

    // 5. wrap-around: last_grant=m3 -> m0
    dut.req_mask = 0b1111;
    dut.last_grant = 0b1000;
    dut.qos_packed = 0x2222;
    settle();
    err += check("rr_wrap", dut.grant, 0b0001);

    // 6. m0,m2 tied top qos=7, last_grant=m0 -> m2
    dut.req_mask = 0b0101;
    dut.last_grant = 0b0001;
    dut.qos_packed = 0x0707;
    settle();
    err += check("tied_top_rr", dut.grant, 0b0100);

    // 7. all 4 reqs, qos {1,3,1,3}, last=m1 -> m3
    dut.req_mask = 0b1111;
    dut.last_grant = 0b0010;
    dut.qos_packed = 0x3131;
    settle();
    err += check("4way_tied", dut.grant, 0b1000);

    if (err) {
        std::printf("FAIL Nic400QosFn: %d sub-cases failed\n", err);
        return 1;
    }
    std::printf("PASS Nic400QosFn — empty, single, qos-pick, rr-fairness, tied-top all OK\n");
    return 0;
}
