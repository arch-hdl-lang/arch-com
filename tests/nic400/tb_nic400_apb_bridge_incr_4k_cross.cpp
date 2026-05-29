// INCR 4 KB-boundary rejection TB for Nic400ApbBridge.
//
// AXI4 §A3.4.1: an INCR burst must not cross a 4 KB address boundary —
// the master is responsible for splitting bursts that would. The
// `ar_incr_no_4k_cross_apb` concurrent SVA fires on any AR handshake
// where `ar_burst == INCR (1)` and
// `ar_addr[11:0] + (ar_len+1)*(1<<ar_size) > 4096`.
//
// We drive `ar_addr = 0x0FF8, ar_size = 2, ar_len = 3` — base 8 bytes
// short of the 0x1000 boundary, 4 beats × 4 bytes = 16 bytes, crossing
// by 8.
//
// Reset polarity is `Reset<Async, Low>`, so rst=0 holds, rst=1 releases.

#include "VNic400ApbBridge.h"

#include <cstdint>
#include <cstdio>

static VNic400ApbBridge dut;

static void tick() {
    dut.clk = 0;
    dut.eval();
    dut.clk = 1;
    dut.eval();
}

static void clear_inputs() {
    dut.axi_ar_valid = 0; dut.axi_ar_addr = 0; dut.axi_ar_id = 0;
    dut.axi_ar_len = 0; dut.axi_ar_size = 0; dut.axi_ar_burst = 0;
    dut.axi_ar_lock = 0; dut.axi_ar_cache = 0; dut.axi_ar_prot = 0;
    dut.axi_ar_qos = 0; dut.axi_ar_region = 0;
    dut.axi_r_ready = 0;
    dut.axi_aw_valid = 0; dut.axi_aw_addr = 0; dut.axi_aw_id = 0;
    dut.axi_aw_len = 0; dut.axi_aw_size = 0; dut.axi_aw_burst = 0;
    dut.axi_aw_lock = 0; dut.axi_aw_cache = 0; dut.axi_aw_prot = 0;
    dut.axi_aw_qos = 0; dut.axi_aw_region = 0;
    dut.axi_w_valid = 0; dut.axi_w_data = 0; dut.axi_w_strb = 0; dut.axi_w_last = 0;
    dut.axi_b_ready = 0;
    dut.apb_prdata = 0; dut.apb_pready = 0; dut.apb_pslverr = 0;
}

int main() {
    dut.rst = 0;
    clear_inputs();
    for (int i = 0; i < 4; ++i) tick();
    dut.rst = 1;
    for (int i = 0; i < 3; ++i) tick();

    // AR handshake: INCR (burst=1), addr=0x0FF8, size=2 (4B), len=3 (4 beats).
    // Total bytes = 16; last byte address = 0x1007 — crosses 4 KB at 0x1000.
    dut.axi_ar_valid = 1;
    dut.axi_ar_addr  = 0x0FF8;
    dut.axi_ar_id    = 0;
    dut.axi_ar_len   = 3;
    dut.axi_ar_size  = 2;
    dut.axi_ar_burst = 1;        // INCR

    for (int i = 0; i < 8; ++i) tick();

    std::printf("FAIL: INCR ar_addr=0x0FF8 ar_len=3 size=2 SVA "
                "ar_incr_no_4k_cross_apb did not fatal "
                "(was Verilator built with --assert?)\n");
    return 0;
}
