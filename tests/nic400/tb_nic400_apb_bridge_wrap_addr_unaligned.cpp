// WRAP-address alignment rejection TB for Nic400ApbBridge.
//
// AXI4 §A3.4.1 requires the WRAP base address to be aligned to
// (1 << ax_size). The `ar_wrap_addr_aligned_apb` concurrent SVA at
// Nic400ApbBridge.arch fires on any AR handshake where
// `ar_burst == WRAP (2)` and the low bits of `ar_addr` aren't zero
// for the declared size. We drive `ar_addr = 0x8003, ar_size = 2`
// (size=2 ⇒ 4-byte access, requires low 2 bits == 0) to trip it.
//
// Under Verilator `--assert`:
//   %Error: ASSERTION FAILED: Nic400ApbBridge.ar_wrap_addr_aligned_apb
//
// Reset polarity is `Reset<Async, Low>` (Nic400ApbBridge.arch:64).

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

    // AR handshake with burst=WRAP (2), ar_size=2 (4-byte access), and
    // ar_addr=0x8003 — base is NOT 4-byte aligned. The legal-len value
    // ar_len=3 keeps `ar_wrap_len_legal_apb` happy so we trip the
    // alignment SVA specifically, not the length SVA.
    dut.axi_ar_valid = 1;
    dut.axi_ar_addr  = 0x8003;   // low 2 bits non-zero — alignment violation
    dut.axi_ar_id    = 0;
    dut.axi_ar_len   = 3;        // legal 4-beat WRAP
    dut.axi_ar_size  = 2;        // 4-byte access — requires ar_addr[1:0] == 0
    dut.axi_ar_burst = 2;        // WRAP

    for (int i = 0; i < 8; ++i) tick();

    std::printf("FAIL: WRAP unaligned-addr SVA ar_wrap_addr_aligned_apb did not fatal "
                "(was Verilator built with --assert?)\n");
    return 0;
}
