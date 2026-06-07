// WRAP-axlen rejection TB for Nic400ApbBridge.
//
// AXI4 §A3.4.1 requires WRAP bursts to have ax_len ∈ {1, 3, 7, 15}
// (i.e. 2/4/8/16-beat bursts). The `ar_wrap_len_legal_apb` concurrent
// SVA at Nic400ApbBridge.arch fires on any AR handshake where
// `ar_burst == WRAP (2)` and `ar_len ∉ {1, 3, 7, 15}`. We drive a
// 3-beat WRAP (ar_len = 2) to trip it.
//
// Under Verilator `--assert` the SVA surfaces as
//   %Error: ASSERTION FAILED: Nic400ApbBridge.ar_wrap_len_legal_apb
// followed by `$fatal(1, ...)` and non-zero exit. The
// `expect_verilator_fatal_multi` harness in `tests/common/mod.rs`
// pins the substring to the exact SVA label.
//
// Reset polarity is `Reset<Async, Low>` (Nic400ApbBridge.arch:64),
// so rst=0 holds, rst=1 releases.

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

    // AR handshake with burst=WRAP (2) and len=2 (3-beat — illegal).
    // The bridge's read thread waits on ar_valid and asserts ar_ready
    // unconditionally for one cycle, so the SVA fires on the very next
    // rising edge.
    dut.axi_ar_valid = 1;
    dut.axi_ar_addr  = 0x8000;   // aligned (size=2 → 4B → low 2 bits clear)
    dut.axi_ar_id    = 0;
    dut.axi_ar_len   = 2;        // 3-beat WRAP — illegal per AXI4 §A3.4.1
    dut.axi_ar_size  = 2;
    dut.axi_ar_burst = 2;        // WRAP

    for (int i = 0; i < 8; ++i) tick();

    std::printf("FAIL: WRAP ar_len=2 SVA ar_wrap_len_legal_apb did not fatal "
                "(was Verilator built with --assert?)\n");
    return 0;
}
