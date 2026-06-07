// EXCLUSIVE burst-length rejection TB for Nic400ApbBridge.
//
// AXI4 §A7.2.4: exclusive accesses (ax_lock = 1) must be at most 16
// beats, i.e. `ax_len <= 15`. The `ar_excl_len_legal_apb` concurrent
// SVA fires on any AR handshake where `ar_lock` is asserted and
// `ar_len > 15`.
//
// We drive `ar_lock = 1, ar_len = 16` — a 17-beat exclusive burst,
// just over the limit. The pow-2-byte and base-alignment halves of
// §A7.2.4 are not exercised here; they have separate (deferred) SVAs.
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

    // AR handshake: INCR (burst=1), addr=0x8000 (well aligned),
    // ax_lock = 1 (EXCLUSIVE), ar_len = 16 (17 beats — illegal).
    dut.axi_ar_valid = 1;
    dut.axi_ar_addr  = 0x8000;
    dut.axi_ar_id    = 0;
    dut.axi_ar_len   = 16;
    dut.axi_ar_size  = 2;
    dut.axi_ar_burst = 1;        // INCR
    dut.axi_ar_lock  = 1;        // EXCLUSIVE

    for (int i = 0; i < 8; ++i) tick();

    std::printf("FAIL: EXCLUSIVE ar_len=16 SVA ar_excl_len_legal_apb did not "
                "fatal (was Verilator built with --assert?)\n");
    return 0;
}
