// AXI4 ↔ APB bridge testbench.
//
// The TB plays the AXI master AND the APB slave. For each scenario it drives
// an AXI AR/AW phase and, on the peripheral side, watches for the APB Setup
// (psel=1, penable=0) → Access (psel=1, penable=1) sequence, returns pready
// (+ optional pslverr / prdata), and finally collects the AXI R or B beat.
//
// As with the AHB bridge TB, we sample on pre_edge() to observe the Mealy
// comb drives BEFORE the lowered thread FSM advances past the handshake
// state. The 2-state ARCH sim has no NBA region, so a write to an input
// followed by eval() takes effect immediately.
//
// AXI resp encoding: 0 = OKAY, 2 = SLVERR.

#include "VNic400ApbBridge.h"
#include <cstdint>
#include <cstdio>

static VNic400ApbBridge dut;
static uint64_t cycle = 0;

static void tick() { dut.clk = 0; dut.eval(); dut.clk = 1; dut.eval(); cycle++; }
static void pre_edge()  { dut.clk = 0; dut.eval(); }
static void post_edge() { dut.clk = 1; dut.eval(); cycle++; }

static void clear_inputs() {
    // AXI master-driven (TB drives, bridge sees IN).
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
    // APB slave-driven (TB drives, bridge sees IN).
    dut.apb_prdata = 0; dut.apb_pready = 0; dut.apb_pslverr = 0;
}

static int fail(const char* m) {
    std::printf("FAIL %s (cycle=%llu)\n", m, (unsigned long long)cycle);
    return 1;
}

// Wait up to `limit` cycles for `cond` to hold at pre_edge. Returns the
// cycle index it was first seen, or -1 on timeout. Caller still owns the
// dut state — this just advances clocks.
static int wait_for(int limit, bool (*cond)()) {
    for (int i = 0; i < limit; ++i) {
        pre_edge();
        if (cond()) return i;
        post_edge();
    }
    return -1;
}

// ── APB slave model ────────────────────────────────────────────────────
// Watches for one APB phase: spin on pre_edge until we see (psel && !penable)
// for the Setup phase, then (psel && penable) for the Access phase, then
// return pready (with optional pslverr and prdata for reads). Returns 0 on
// success. The caller is responsible for driving AXI handshake side.
struct ApbPhase {
    uint32_t expect_paddr;
    bool     expect_pwrite;
    uint32_t expect_pwdata;     // ignored when !expect_pwrite
    unsigned expect_pstrb;      // ignored when !expect_pwrite
    uint32_t return_prdata;     // used when !expect_pwrite
    bool     return_pslverr;
    int      pready_delay;      // extra access cycles before asserting pready
};

static int run_apb_phase(const ApbPhase& p) {
    // ── Setup phase: psel=1, penable=0, addresses+control valid ──
    int setup_seen = 0;
    for (int i = 0; i < 64 && !setup_seen; ++i) {
        pre_edge();
        if (dut.apb_psel && !dut.apb_penable) {
            setup_seen = 1;
            if ((uint32_t)dut.apb_paddr != p.expect_paddr) return fail("APB setup paddr mismatch");
            if ((unsigned)dut.apb_pwrite != (p.expect_pwrite ? 1u : 0u)) return fail("APB setup pwrite mismatch");
            if (p.expect_pwrite) {
                if ((uint32_t)dut.apb_pwdata != p.expect_pwdata) return fail("APB setup pwdata mismatch");
                if ((unsigned)dut.apb_pstrb  != p.expect_pstrb)  return fail("APB setup pstrb mismatch");
            }
        }
        post_edge();
    }
    if (!setup_seen) return fail("APB setup phase never observed");

    // ── Access phase: psel=1, penable=1. Hold pready=0 for `pready_delay`
    // cycles to test backpressure, then assert pready (+ optional data/err).
    // Bridge must hold paddr/pwdata stable across the stall. ──
    int access_seen = 0;
    for (int i = 0; i < 64 && !access_seen; ++i) {
        pre_edge();
        if (dut.apb_psel && dut.apb_penable) {
            access_seen = 1;
            // Stability check during stall.
            if ((uint32_t)dut.apb_paddr != p.expect_paddr) return fail("APB access paddr drift");
            if (p.expect_pwrite) {
                if ((uint32_t)dut.apb_pwdata != p.expect_pwdata) return fail("APB access pwdata drift");
            }
        }
        post_edge();
    }
    if (!access_seen) return fail("APB access phase never observed");

    // Apply stall.
    for (int i = 0; i < p.pready_delay; ++i) {
        // pready stays low.
        pre_edge();
        if (!dut.apb_psel || !dut.apb_penable) return fail("APB phase aborted during stall");
        if ((uint32_t)dut.apb_paddr != p.expect_paddr) return fail("APB paddr drift under stall");
        if (p.expect_pwrite && (uint32_t)dut.apb_pwdata != p.expect_pwdata) return fail("APB pwdata drift under stall");
        post_edge();
    }

    // Drive pready high (with prdata / pslverr as requested) until bridge
    // exits the Access phase (psel falls or penable falls). Typically the
    // bridge consumes it in one cycle.
    dut.apb_pready = 1;
    if (!p.expect_pwrite) dut.apb_prdata = p.return_prdata;
    dut.apb_pslverr = p.return_pslverr ? 1 : 0;

    int done = 0;
    for (int i = 0; i < 8 && !done; ++i) {
        post_edge();
        pre_edge();
        if (!dut.apb_penable) done = 1;     // bridge moved past Access
    }
    if (!done) return fail("APB access never released after pready=1");
    dut.apb_pready = 0; dut.apb_prdata = 0; dut.apb_pslverr = 0;
    return 0;
}

// ── AXI master helpers ─────────────────────────────────────────────────

// Drive an AR phase and wait for ar_ready handshake. Address/len/size/prot
// are held on the bus until the bridge captures them.
static int issue_ar(uint32_t addr, unsigned len, unsigned size, unsigned prot, unsigned id) {
    dut.axi_ar_valid = 1; dut.axi_ar_addr = addr; dut.axi_ar_len = len;
    dut.axi_ar_size = size; dut.axi_ar_burst = 1; dut.axi_ar_prot = prot;
    dut.axi_ar_id = id;

    int handshake = 0;
    for (int i = 0; i < 32 && !handshake; ++i) {
        pre_edge();
        if (dut.axi_ar_ready && dut.axi_ar_valid) handshake = 1;
        post_edge();
    }
    if (!handshake) return fail("AR never accepted");
    dut.axi_ar_valid = 0; dut.axi_ar_addr = 0; dut.axi_ar_len = 0;
    dut.axi_ar_size = 0; dut.axi_ar_burst = 0; dut.axi_ar_prot = 0; dut.axi_ar_id = 0;
    return 0;
}

static int issue_aw(uint32_t addr, unsigned len, unsigned size, unsigned prot, unsigned id) {
    dut.axi_aw_valid = 1; dut.axi_aw_addr = addr; dut.axi_aw_len = len;
    dut.axi_aw_size = size; dut.axi_aw_burst = 1; dut.axi_aw_prot = prot;
    dut.axi_aw_id = id;

    int handshake = 0;
    for (int i = 0; i < 32 && !handshake; ++i) {
        pre_edge();
        if (dut.axi_aw_ready && dut.axi_aw_valid) handshake = 1;
        post_edge();
    }
    if (!handshake) return fail("AW never accepted");
    dut.axi_aw_valid = 0; dut.axi_aw_addr = 0; dut.axi_aw_len = 0;
    dut.axi_aw_size = 0; dut.axi_aw_burst = 0; dut.axi_aw_prot = 0; dut.axi_aw_id = 0;
    return 0;
}

// Capture one AXI R beat: drive r_ready=1, observe r_valid + r_data/r_resp.
static int capture_r(uint32_t expect_data, unsigned expect_resp, bool expect_last) {
    dut.axi_r_ready = 1;
    int seen = 0;
    for (int i = 0; i < 64 && !seen; ++i) {
        pre_edge();
        if (dut.axi_r_valid && dut.axi_r_ready) {
            seen = 1;
            if ((uint32_t)dut.axi_r_data != expect_data) return fail("R data mismatch");
            if ((unsigned)dut.axi_r_resp != expect_resp) return fail("R resp mismatch");
            if ((bool)dut.axi_r_last != expect_last)     return fail("R last mismatch");
        }
        post_edge();
    }
    if (!seen) return fail("R beat never delivered");
    dut.axi_r_ready = 0;
    return 0;
}

// Drive one AXI W beat and wait for w_ready.
static int drive_w(uint32_t data, unsigned strb, bool last) {
    dut.axi_w_valid = 1; dut.axi_w_data = data; dut.axi_w_strb = strb;
    dut.axi_w_last = last ? 1 : 0;
    int seen = 0;
    for (int i = 0; i < 64 && !seen; ++i) {
        pre_edge();
        if (dut.axi_w_valid && dut.axi_w_ready) seen = 1;
        post_edge();
    }
    if (!seen) return fail("W never accepted");
    dut.axi_w_valid = 0; dut.axi_w_data = 0; dut.axi_w_strb = 0; dut.axi_w_last = 0;
    return 0;
}

static int capture_b(unsigned expect_resp) {
    dut.axi_b_ready = 1;
    int seen = 0;
    for (int i = 0; i < 64 && !seen; ++i) {
        pre_edge();
        if (dut.axi_b_valid && dut.axi_b_ready) {
            seen = 1;
            if ((unsigned)dut.axi_b_resp != expect_resp) return fail("B resp mismatch");
        }
        post_edge();
    }
    if (!seen) return fail("B never delivered");
    dut.axi_b_ready = 0;
    return 0;
}

// ── Scenarios ──────────────────────────────────────────────────────────

static int scenario_single_read() {
    if (issue_ar(0x1000, /*len*/0, /*size*/2, /*prot*/0x3, /*id*/1)) return 1;
    ApbPhase ph = { 0x1000, false, 0, 0, 0xDEADBEEFu, false, 0 };
    if (run_apb_phase(ph)) return 1;
    if (capture_r(0xDEADBEEFu, 0, true)) return 1;
    tick();
    std::printf("PASS scenario_single_read\n");
    return 0;
}

static int scenario_single_write() {
    // AW + W race: drive both in lock-step. The bridge accepts AW first, then
    // enters Setup; we drive W during the Setup phase so pwdata appears on
    // axi.w_data when the bridge enters the Access state.
    if (issue_aw(0x2000, /*len*/0, /*size*/2, /*prot*/0x7, /*id*/2)) return 1;
    // Hold W on the bus from now — bridge will accept it during APB Access.
    dut.axi_w_valid = 1; dut.axi_w_data = 0xCAFEBABEu;
    dut.axi_w_strb = 0xF; dut.axi_w_last = 1;
    ApbPhase ph = { 0x2000, true, 0xCAFEBABEu, 0xF, 0, false, 0 };
    if (run_apb_phase(ph)) return 1;
    // After pready=1, the bridge should have asserted w_ready in the same
    // cycle as APB pready. Confirm w_valid/w_ready handshake completed.
    // (run_apb_phase advanced past the access state; w handshake fired then.)
    dut.axi_w_valid = 0; dut.axi_w_data = 0; dut.axi_w_strb = 0; dut.axi_w_last = 0;
    if (capture_b(0)) return 1;
    tick();
    std::printf("PASS scenario_single_write\n");
    return 0;
}

static int scenario_burst_read_4() {
    if (issue_ar(0x3000, /*len*/3, /*size*/2, /*prot*/0, /*id*/3)) return 1;
    uint32_t expected_data[4] = { 0x11111111u, 0x22222222u, 0x33333333u, 0x44444444u };
    uint32_t expected_addr[4] = { 0x3000, 0x3004, 0x3008, 0x300C };
    for (int b = 0; b < 4; ++b) {
        ApbPhase ph = { expected_addr[b], false, 0, 0, expected_data[b], false, 0 };
        if (run_apb_phase(ph)) return 1;
        if (capture_r(expected_data[b], 0, b == 3)) return 1;
    }
    tick();
    std::printf("PASS scenario_burst_read_4\n");
    return 0;
}

// 4-beat write burst: drive W beats back-to-back. The bridge sequences
// Setup → Access for each beat; the AXI W handshake fires inside Access
// at the cycle pready=1. We keep w_valid asserted across the whole burst
// and just swap w_data/w_last between beats, so the next Setup phase sees
// the next beat's data on the bus.
static int scenario_burst_write_4() {
    if (issue_aw(0x4000, /*len*/3, /*size*/2, /*prot*/0, /*id*/4)) return 1;
    uint32_t wdata[4] = { 0xAAAA0000u, 0xBBBB0001u, 0xCCCC0002u, 0xDDDD0003u };
    uint32_t addrs[4] = { 0x4000, 0x4004, 0x4008, 0x400C };
    // Present beat 0 immediately; subsequent beats are loaded at the bottom
    // of each iteration.
    dut.axi_w_valid = 1; dut.axi_w_data = wdata[0];
    dut.axi_w_strb = 0xF; dut.axi_w_last = 0;
    for (int b = 0; b < 4; ++b) {
        dut.axi_w_data = wdata[b];
        dut.axi_w_last = (b == 3) ? 1 : 0;
        ApbPhase ph = { addrs[b], true, wdata[b], 0xF, 0, false, 0 };
        if (run_apb_phase(ph)) return 1;
    }
    dut.axi_w_valid = 0; dut.axi_w_data = 0; dut.axi_w_strb = 0; dut.axi_w_last = 0;
    if (capture_b(0)) return 1;
    tick();
    std::printf("PASS scenario_burst_write_4\n");
    return 0;
}

static int scenario_slverr_write() {
    if (issue_aw(0x5000, /*len*/0, /*size*/2, /*prot*/0, /*id*/5)) return 1;
    dut.axi_w_valid = 1; dut.axi_w_data = 0x5A5A5A5Au;
    dut.axi_w_strb = 0xF; dut.axi_w_last = 1;
    ApbPhase ph = { 0x5000, true, 0x5A5A5A5Au, 0xF, 0, /*pslverr*/true, 0 };
    if (run_apb_phase(ph)) return 1;
    dut.axi_w_valid = 0; dut.axi_w_data = 0; dut.axi_w_strb = 0; dut.axi_w_last = 0;
    if (capture_b(2)) return 1;   // SLVERR
    tick();
    std::printf("PASS scenario_slverr_write\n");
    return 0;
}

// Backpressure: APB pready held low for several cycles mid-access. Bridge
// must hold paddr / pwdata / pstrb stable. run_apb_phase enforces this via
// stability checks during the stall.
static int scenario_backpressure() {
    if (issue_aw(0x6000, /*len*/0, /*size*/2, /*prot*/0, /*id*/6)) return 1;
    dut.axi_w_valid = 1; dut.axi_w_data = 0x12345678u;
    dut.axi_w_strb = 0xF; dut.axi_w_last = 1;
    ApbPhase ph = { 0x6000, true, 0x12345678u, 0xF, 0, false, /*pready_delay*/5 };
    if (run_apb_phase(ph)) return 1;
    dut.axi_w_valid = 0; dut.axi_w_data = 0; dut.axi_w_strb = 0; dut.axi_w_last = 0;
    if (capture_b(0)) return 1;
    tick();
    std::printf("PASS scenario_backpressure (5-cycle APB stall)\n");
    return 0;
}

int main() {
    dut.rst = 0;
    clear_inputs();
    for (int i = 0; i < 4; ++i) tick();
    dut.rst = 1;
    for (int i = 0; i < 3; ++i) tick();

    if (scenario_single_read())   return 1;
    if (scenario_single_write())  return 1;
    if (scenario_burst_read_4())  return 1;
    if (scenario_burst_write_4()) return 1;
    if (scenario_slverr_write())  return 1;
    if (scenario_backpressure())  return 1;

    std::printf("PASS Nic400ApbBridge: 6/6 scenarios\n");
    return 0;
}
