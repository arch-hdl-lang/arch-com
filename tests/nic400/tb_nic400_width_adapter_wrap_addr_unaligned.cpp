// WRAP-address alignment rejection TB for Nic400WidthAdapter.
//
// AXI4 §A3.4.1 requires the WRAP base address to be aligned to
// (1 << ax_size). The `ar_wrap_addr_aligned_widthadapter` concurrent
// SVA at Nic400WidthAdapter.arch fires on any wide-master AR
// handshake where `m.ar_burst == WRAP (2)` and the low bits of
// `m.ar_addr` aren't zero for the declared size. We drive
// `ar_addr = 0x8003, ar_size = 2` (size=2 ⇒ 4-byte access, requires
// low 2 bits == 0) to trip it.
//
// Under Verilator `--assert`:
//   %Error: ASSERTION FAILED: Nic400WidthAdapter.ar_wrap_addr_aligned_widthadapter
//
// Reset polarity is `Reset<Async, Low>` (Nic400WidthAdapter.arch:60).

#include "VNic400WidthAdapter.h"

#include <cstdint>
#include <cstdio>

static VNic400WidthAdapter dut;

static void tick() {
    dut.clk = 0;
    dut.eval();
    dut.clk = 1;
    dut.eval();
}

static void clear_inputs() {
    dut.m_ar_valid = 0; dut.m_ar_addr = 0; dut.m_ar_id = 0; dut.m_ar_len = 0;
    dut.m_ar_size = 0;  dut.m_ar_burst = 0; dut.m_ar_lock = 0;
    dut.m_ar_cache = 0; dut.m_ar_prot = 0;  dut.m_ar_qos = 0; dut.m_ar_region = 0;
    dut.m_r_ready = 0;
    dut.m_aw_valid = 0; dut.m_aw_addr = 0; dut.m_aw_id = 0; dut.m_aw_len = 0;
    dut.m_aw_size = 0;  dut.m_aw_burst = 0; dut.m_aw_lock = 0;
    dut.m_aw_cache = 0; dut.m_aw_prot = 0;  dut.m_aw_qos = 0; dut.m_aw_region = 0;
    dut.m_w_valid = 0; dut.m_w_data = 0; dut.m_w_strb = 0; dut.m_w_last = 0;
    dut.m_b_ready = 0;
    dut.s_ar_ready = 0;
    dut.s_r_valid = 0; dut.s_r_data = 0; dut.s_r_id = 0; dut.s_r_resp = 0; dut.s_r_last = 0;
    dut.s_aw_ready = 0;
    dut.s_w_ready = 0;
    dut.s_b_valid = 0; dut.s_b_id = 0; dut.s_b_resp = 0;
}

int main() {
    dut.rst = 0;
    clear_inputs();
    for (int i = 0; i < 4; ++i) tick();
    dut.rst = 1;
    for (int i = 0; i < 3; ++i) tick();

    // AR with burst=WRAP, ar_size=2 (4-byte), ar_addr=0x8003 — base is
    // NOT 4-byte aligned. The legal ar_len=3 keeps the length SVA happy
    // so we trip the alignment SVA specifically.
    dut.m_ar_valid = 1;
    dut.m_ar_addr  = 0x8003;     // low 2 bits non-zero
    dut.m_ar_id    = 0;
    dut.m_ar_len   = 3;          // legal 4-beat WRAP
    dut.m_ar_size  = 2;          // 4-byte — requires ar_addr[1:0] == 0
    dut.m_ar_burst = 2;          // WRAP
    dut.s_ar_ready = 1;

    for (int i = 0; i < 8; ++i) tick();

    std::printf("FAIL: WRAP unaligned-addr SVA ar_wrap_addr_aligned_widthadapter did not fatal "
                "(was Verilator built with --assert?)\n");
    return 0;
}
