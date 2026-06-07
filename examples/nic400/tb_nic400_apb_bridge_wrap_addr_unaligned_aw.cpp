// AW-side mirror of `tb_nic400_apb_bridge_wrap_addr_unaligned.cpp`.
//
// AXI4 §A3.4.1 requires the WRAP base address to be naturally aligned
// to `(1 << ax_size)` — for both read AND write channels. The
// `aw_wrap_addr_aligned_apb` concurrent SVA at Nic400ApbBridge.arch
// fires on any AW handshake where `aw_burst == WRAP (2)` and the low
// `aw_size` address bits are non-zero. We drive `aw_addr = 0x8003`
// with `aw_size = 2` (4-byte access, low 2 bits must be clear) to
// trip it.
//
// Closes the AW-coverage half of arch-com#447 §4.

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

    // AW handshake with burst=WRAP (2), size=2 (4B), and an unaligned
    // base addr (low 2 bits non-zero). `aw_len=1` is a legal WRAP shape
    // (2-beat) so only the alignment check should fire.
    dut.axi_aw_valid = 1;
    dut.axi_aw_addr  = 0x8003;   // misaligned for size=2
    dut.axi_aw_id    = 0;
    dut.axi_aw_len   = 1;        // legal len (2-beat WRAP)
    dut.axi_aw_size  = 2;
    dut.axi_aw_burst = 2;        // WRAP

    for (int i = 0; i < 8; ++i) tick();

    std::printf("FAIL: WRAP aw_addr=0x8003 SVA aw_wrap_addr_aligned_apb did not "
                "fatal (was Verilator built with --assert?)\n");
    return 0;
}
