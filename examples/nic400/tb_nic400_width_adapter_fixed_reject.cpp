// FIXED-burst rejection TB for Nic400WidthAdapter.
//
// Nic400WidthAdapter.arch:299-300 carries two concurrent SVAs:
//   ar_burst_supported: (m.ar_valid && m.ar_ready) |-> (m.ar_burst == 1 || m.ar_burst == 2)
//   aw_burst_supported: (m.aw_valid && m.aw_ready) |-> (m.aw_burst == 1 || m.aw_burst == 2)
//
// They reject FIXED (`burst == 0`) and the reserved code (`burst == 3`).
// Under Verilator `--assert` a violation surfaces as
//   %Error: ASSERTION FAILED: Nic400WidthAdapter.ar_burst_supported
// followed by `$fatal(1, ...)` and a non-zero exit. The
// `expect_verilator_fatal` harness in `tests/common/mod.rs` matches on
// the SVA label substring to keep the test pinned to the *specific*
// assertion rather than any unrelated fatal.
//
// This TB is the auto-CI counterpart to the manual repro recipe in
// `nic400_interconnect_spec.md` §15.1 — kept as a standalone
// binary (own `main`) so its exit-code semantics (must abort) stay
// orthogonal to the regular `tb_nic400_width_adapter.cpp` which exits
// 0 on success.
//
// Reset polarity is `Reset<Async, Low>` (Nic400WidthAdapter.arch:60),
// so the design is OUT of reset when `rst == 1` and held in reset
// when `rst == 0`.

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

    // Drive an AR handshake with burst=FIXED (00). The adapter's AR
    // thread waits for `m.ar_valid`, then drives `s.ar_valid = true`
    // and `m.ar_ready = s.ar_ready`. Holding `s_ar_ready=1` closes the
    // master-side handshake unconditionally, regardless of burst code,
    // so the concurrent SVA fires on the very next rising edge.
    dut.m_ar_valid = 1;
    dut.m_ar_addr  = 0xF000;
    dut.m_ar_id    = 0;
    dut.m_ar_len   = 3;
    dut.m_ar_size  = 3;
    dut.m_ar_burst = 0;   // FIXED — must be rejected by ar_burst_supported.
    dut.s_ar_ready = 1;

    // A handful of cycles is more than enough for the SVA to evaluate.
    // If Verilator was built with `--assert`, this loop never completes
    // — `$fatal(1, "ASSERTION FAILED: Nic400WidthAdapter.ar_burst_supported")`
    // aborts the sim first.
    for (int i = 0; i < 8; ++i) tick();

    // Reaching this point means the SVA did NOT fire. Print a
    // distinctive marker so the harness's `expected_substr` check has
    // something to *not* match — and return 0 so the harness's
    // `non-zero exit` assertion catches the regression loudly.
    std::printf("FAIL: FIXED-burst SVA ar_burst_supported did not fatal "
                "(was Verilator built with --assert?)\n");
    return 0;
}
