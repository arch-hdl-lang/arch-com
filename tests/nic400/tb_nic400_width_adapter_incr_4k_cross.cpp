// INCR 4 KB boundary-crossing rejection TB for Nic400WidthAdapter.
//
// AXI4 §A3.4.1 forbids an INCR burst from crossing a 4 KB address
// boundary. PR #466 added a master-side SVA on the WidthAdapter:
//
//   assert ar_incr_no_4k_cross_widthadapter:
//     (m.ar_valid && m.ar_ready && m.ar_burst == 1)
//     |-> (m.ar_addr[11:0].zext<17>()
//          + ((m.ar_len.zext<9>() + 1) << m.ar_size).zext<17>()
//          <= 17'd4096);
//
// at Nic400WidthAdapter.arch (with an aw_ twin). PR #466's commit note
// argued that "harness CI on APB is sufficient confirmation" because
// the SVAs are structurally identical to the APB bridge's. That claim
// is load-bearing on a subtle property:
//
//   The WidthAdapter scales the *axlen* and *axsize* it forwards
//   downstream (master axlen N becomes slave axlen (N+1)*RATIO-1,
//   master axsize becomes log2(S_DATA_W/8)) — but it preserves the
//   *total byte count* and forwards the *base address unchanged*.
//   Therefore the master-side 4 KB span =
//     (m.ar_len+1) * (1 << m.ar_size)
//   equals the slave-side span and the boundary computation is
//   genuinely the same on both sides of the adapter. The SVA
//   references the *pre-scaling* master-side values (m.ar_len /
//   m.ar_size), so it catches the exact set of master bursts whose
//   footprint crosses 4 KB. arch-com PR #477's Finding 6 worried that
//   "(N+1)·RATIO axlen doubling could make the slave cross a
//   boundary the master didn't", but because the SVA reads master
//   axlen *before* scaling and total byte count is invariant under
//   scaling, the two views coincide. This TB pins that property so
//   any future refactor that accidentally references the post-scaling
//   `s.ar_*` signals in the SVA still trips on a master-side
//   crossing — and so any codegen regression that drops the
//   _widthadapter-labelled SVA gets caught loudly here rather than
//   only via the APB bridge's matching SVA.
//
// Stimulus derivation:
//   • M_DATA_W = 64 (default) ⇒ ar_size = 3 (8 bytes/beat) is a
//     full-width master access.
//   • ar_len = 7 ⇒ 8 beats × 8 B = 64 B total burst.
//   • ar_addr = 0x0FF8 ⇒ low 12 bits = 0xFF8 (= 4088 decimal).
//   • 4088 + 64 = 4152 > 4096 ⇒ burst occupies 0x0FF8..0x1037,
//     stepping 56 bytes into the next 4 KB page. SVA must fire.
//   • RATIO = 2 (M_DATA_W/S_DATA_W = 64/32). Slave sees axlen = 15,
//     axsize = 2 (4 bytes), same 64 B span from 0x0FF8 — identical
//     crossing. Confirms the byte-count invariance argument above.
//
// Under Verilator `--assert`:
//   %Error: ASSERTION FAILED:
//     Nic400WidthAdapter.ar_incr_no_4k_cross_widthadapter
//
// Reset polarity is `Reset<Async, Low>` (Nic400WidthAdapter.arch:60),
// so the design is OUT of reset when `rst == 1`.

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
    // Master side (TB drives master-style requests INTO the adapter).
    dut.m_ar_valid = 0; dut.m_ar_addr = 0; dut.m_ar_id = 0; dut.m_ar_len = 0;
    dut.m_ar_size = 0;  dut.m_ar_burst = 0; dut.m_ar_lock = 0;
    dut.m_ar_cache = 0; dut.m_ar_prot = 0;  dut.m_ar_qos = 0; dut.m_ar_region = 0;
    dut.m_r_ready = 0;
    dut.m_aw_valid = 0; dut.m_aw_addr = 0; dut.m_aw_id = 0; dut.m_aw_len = 0;
    dut.m_aw_size = 0;  dut.m_aw_burst = 0; dut.m_aw_lock = 0;
    dut.m_aw_cache = 0; dut.m_aw_prot = 0;  dut.m_aw_qos = 0; dut.m_aw_region = 0;
    dut.m_w_valid = 0; dut.m_w_data = 0; dut.m_w_strb = 0; dut.m_w_last = 0;
    dut.m_b_ready = 0;
    // Slave side (TB plays the downstream slave).
    dut.s_ar_ready = 0;
    dut.s_r_valid = 0; dut.s_r_data = 0; dut.s_r_id = 0; dut.s_r_resp = 0; dut.s_r_last = 0;
    dut.s_aw_ready = 0;
    dut.s_w_ready = 0;
    dut.s_b_valid = 0; dut.s_b_id = 0; dut.s_b_resp = 0;
}

int main() {
    // Active-low async reset: drive rst=0 to hold, rst=1 to release.
    dut.rst = 0;
    clear_inputs();
    for (int i = 0; i < 4; ++i) tick();
    dut.rst = 1;
    for (int i = 0; i < 3; ++i) tick();

    // AR with burst=INCR (01), ar_size=3 (8-byte master beats),
    // ar_len=7 (8 beats × 8 B = 64 B), ar_addr=0x0FF8. The burst
    // footprint 0x0FF8..0x1037 straddles the 4 KB boundary at 0x1000,
    // so the master-side `ar_incr_no_4k_cross_widthadapter` SVA must
    // fire on the AR handshake cycle.
    dut.m_ar_valid = 1;
    dut.m_ar_addr  = 0x0FF8;
    dut.m_ar_id    = 0;
    dut.m_ar_len   = 7;
    dut.m_ar_size  = 3;
    dut.m_ar_burst = 1;          // INCR
    dut.s_ar_ready = 1;          // close the handshake unconditionally

    // A handful of cycles is more than enough for the SVA to evaluate.
    // If Verilator was built with `--assert`, this loop never completes
    // — `$fatal(1, "ASSERTION FAILED: ...")` aborts the sim first.
    for (int i = 0; i < 8; ++i) tick();

    std::printf("FAIL: INCR 4 KB-cross SVA ar_incr_no_4k_cross_widthadapter did not fatal "
                "(was Verilator built with --assert?)\n");
    return 0;
}
