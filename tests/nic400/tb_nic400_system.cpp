// Integrated NIC-400 system smoke test.
//
// Verifies the full path CPU AHB master → Nic400AhbBridge → Nic400Fabric
// → Nic400ApbBridge → APB peripheral. The TB plays both the AHB master
// (driving NONSEQ + addr + HWRITE on ahb_h) and the APB peripheral
// (responding to psel/penable/pwrite on apb_p with prdata/pready/pslverr).
//
// What's tested
// ─────────────
//   1. Single-beat AHB write → APB write. AHB master drives a HBURST=
//      SINGLE write with HSIZE=word and HADDR pointing into slave 0's
//      region. Verify the APB peripheral sees psel=1, penable=1,
//      pwrite=1, paddr matches the AHB address, pwdata matches HWDATA.
//   2. Single-beat AHB read → APB read. AHB master drives NONSEQ +
//      HWRITE=0. TB-as-peripheral returns prdata on pready=1. Verify
//      AHB master sees HRDATA matching the returned value.
//   3. PMU exact-count: after S1 (write) and S2 (read), master 0's
//      ar/r/aw/w/b counters each equal 1; masters 1 and 2 stay at 0.
//
// Addressing
// ──────────
// The fabric routes by the top NS_W bits of (addr >> REGION_BITS=28).
// HADDR=0x0000_1000 selects slave 0 (bits [29:28] = 0). For tests
// targeting other slaves you'd use 0x1000_xxxx, 0x2000_xxxx, etc.

#include "VNic400System.h"
#include <cstdint>
#include <cstdio>

static VNic400System dut;
static uint64_t cycle = 0;

static void tick()      { dut.clk = 0; dut.eval(); dut.clk = 1; dut.eval(); cycle++; }
static void pre_edge()  { dut.clk = 0; dut.eval(); }
static void post_edge() { dut.clk = 1; dut.eval(); cycle++; }

static void clear_inputs() {
    dut.ahb_h_hsel = 0; dut.ahb_h_haddr = 0; dut.ahb_h_hwrite = 0;
    dut.ahb_h_hsize = 0; dut.ahb_h_hburst = 0; dut.ahb_h_hprot = 0;
    dut.ahb_h_htrans = 0; dut.ahb_h_hmastlock = 0; dut.ahb_h_hwdata = 0;
    dut.apb_p_prdata = 0; dut.apb_p_pready = 0; dut.apb_p_pslverr = 0;
}

static int fail(const char* m) {
    std::printf("FAIL %s (cycle=%llu)\n", m, (unsigned long long)cycle);
    return 1;
}

// Run an AHB single-beat write while playing the APB peripheral.
// Returns 0 on success.
static int do_ahb_write(uint32_t addr, uint32_t data,
                        uint32_t* captured_paddr, uint32_t* captured_pwdata) {
    // Drive AHB master: NONSEQ + HWRITE + addr.
    dut.ahb_h_hsel    = 1;
    dut.ahb_h_haddr   = addr;
    dut.ahb_h_hwrite  = 1;
    dut.ahb_h_hsize   = 2;       // 4 bytes
    dut.ahb_h_hburst  = 0;       // SINGLE
    dut.ahb_h_hprot   = 0;
    dut.ahb_h_htrans  = 2;       // NONSEQ
    dut.ahb_h_hmastlock = 0;
    dut.ahb_h_hwdata  = data;

    int phase_set = 0, phase_access = 0;
    *captured_paddr  = 0;
    *captured_pwdata = 0;

    for (int i = 0; i < 256; ++i) {
        // Drive pready as a 1-cycle response in the access phase.
        if (dut.apb_p_psel && dut.apb_p_penable && !phase_access) {
            dut.apb_p_pready  = 1;
            dut.apb_p_pslverr = 0;
            *captured_paddr  = (uint32_t)dut.apb_p_paddr;
            *captured_pwdata = (uint32_t)dut.apb_p_pwdata;
            phase_access = 1;
        } else if (dut.apb_p_psel && !dut.apb_p_penable) {
            // Setup phase: don't drive pready yet.
            dut.apb_p_pready = 0;
            phase_set = 1;
        } else {
            dut.apb_p_pready = 0;
        }
        pre_edge();
        if (phase_access) {
            // After access phase ack, drop AHB drives.
            dut.ahb_h_hsel = 0;
            dut.ahb_h_htrans = 0;
        }
        post_edge();
        if (phase_access && !dut.apb_p_psel) {
            // APB transaction complete from peripheral's view.
            dut.apb_p_pready = 0;
            return 0;
        }
    }
    return fail("AHB write never reached APB access phase + return");
}

// Run an AHB single-beat read while playing the APB peripheral.
static int do_ahb_read(uint32_t addr, uint32_t return_data,
                       uint32_t* captured_paddr, uint32_t* hrdata_seen) {
    dut.ahb_h_hsel    = 1;
    dut.ahb_h_haddr   = addr;
    dut.ahb_h_hwrite  = 0;
    dut.ahb_h_hsize   = 2;
    dut.ahb_h_hburst  = 0;
    dut.ahb_h_hprot   = 0;
    dut.ahb_h_htrans  = 2;
    dut.ahb_h_hmastlock = 0;
    dut.ahb_h_hwdata  = 0;

    int phase_access = 0;
    int hrdata_captured = 0;
    *captured_paddr = 0;
    *hrdata_seen    = 0;

    for (int i = 0; i < 256; ++i) {
        if (dut.apb_p_psel && dut.apb_p_penable && !phase_access) {
            dut.apb_p_prdata  = return_data;
            dut.apb_p_pready  = 1;
            dut.apb_p_pslverr = 0;
            *captured_paddr = (uint32_t)dut.apb_p_paddr;
            phase_access = 1;
        } else if (dut.apb_p_psel && !dut.apb_p_penable) {
            dut.apb_p_pready = 0;
        } else {
            dut.apb_p_pready = 0;
        }
        pre_edge();
        // Capture HRDATA on the master-side HREADY pulse, but only AFTER
        // the APB access has fired — hready is high in idle state too,
        // so we'd otherwise capture stale HRDATA on cycle 0.
        if (!hrdata_captured && phase_access && dut.ahb_h_hready
                                            && dut.ahb_h_hrdata != 0) {
            *hrdata_seen = (uint32_t)dut.ahb_h_hrdata;
            hrdata_captured = 1;
        }
        if (phase_access) {
            dut.ahb_h_hsel = 0;
            dut.ahb_h_htrans = 0;
        }
        post_edge();
        if (phase_access && !dut.apb_p_psel && hrdata_captured) {
            dut.apb_p_pready = 0;
            return 0;
        }
    }
    return fail("AHB read never reached APB access phase + HRDATA");
}

int main() {
    dut.rst = 0;
    clear_inputs();
    for (int i = 0; i < 4; ++i) tick();
    dut.rst = 1;
    for (int i = 0; i < 3; ++i) tick();

    // ── Scenario 1: AHB write → APB write ─────────────────────────────
    uint32_t addr1 = 0x0000'1000;  // bits [29:28] = 0 → slave 0
    uint32_t data1 = 0xCAFE'BABEu;
    uint32_t paddr_w = 0, pwdata_w = 0;
    if (do_ahb_write(addr1, data1, &paddr_w, &pwdata_w)) return 1;
    if (paddr_w != addr1) {
        std::printf("FAIL S1: APB paddr=0x%x, expected 0x%x\n", paddr_w, addr1);
        return 1;
    }
    if (pwdata_w != data1) {
        std::printf("FAIL S1: APB pwdata=0x%x, expected 0x%x\n", pwdata_w, data1);
        return 1;
    }
    std::printf("  OK [S1] AHB write 0x%x = 0x%x → APB paddr/pwdata match\n", addr1, data1);

    // ── Scenario 2: AHB read → APB read ───────────────────────────────
    uint32_t addr2 = 0x0000'2004;
    uint32_t prdata2 = 0xDEAD'BEEFu;
    uint32_t paddr_r = 0, hrdata = 0;
    if (do_ahb_read(addr2, prdata2, &paddr_r, &hrdata)) return 1;
    if (paddr_r != addr2) {
        std::printf("FAIL S2: APB paddr=0x%x, expected 0x%x\n", paddr_r, addr2);
        return 1;
    }
    if (hrdata != prdata2) {
        std::printf("FAIL S2: HRDATA=0x%x, expected 0x%x\n", hrdata, prdata2);
        return 1;
    }
    std::printf("  OK [S2] AHB read 0x%x ← APB prdata 0x%x → HRDATA matches\n", addr2, prdata2);

    // ── Scenario 3: PMU exact-count check on master 0 + idle on m1/m2 ─
    // After S1 (single-beat AHB write) the master-0 AW/W/B channels each
    // see exactly one handshake. After S2 (single-beat AHB read) the AR/R
    // channels each see one. Masters 1 and 2 are tied off and must stay
    // at zero on every channel.
    //
    // Note for future maintainers: handshake pulses on `ahb_axi.*_valid &&
    // *_ready` ARE observable by the PMU's `seq on clk rising` block, even
    // when produced by the AHB bridge's `wait 0+ cycle until …;` Mealy
    // fusion. The pulse is a comb signal that holds across the rising edge
    // until the consumer asserts `ready` — exactly when the sampling edge
    // fires — so the PMU's `if aw_event[i] cnt <= cnt +% 1;` increments
    // correctly. An earlier version of this TB skipped master-0 exact-
    // count checks with an "unobservable Mealy pulse" rationale; the true
    // cause of the zero counts was a sim_codegen bug (PR #442 /
    // de35faa) where param-sized `wire Vec<T, PARAM>` connections to
    // sub-inst Vec input ports emitted zero fan-out lines, leaving the
    // PMU's inputs default-constructed. Do not reintroduce that apology.
    for (int i = 0; i < 10; ++i) tick();   // settle PMU regs
    unsigned ar0 = (unsigned)dut.ar_count[0];
    unsigned r0  = (unsigned)dut.r_count[0];
    unsigned aw0 = (unsigned)dut.aw_count[0];
    unsigned w0  = (unsigned)dut.w_count[0];
    unsigned b0  = (unsigned)dut.b_count[0];
    if (ar0 != 1 || r0 != 1 || aw0 != 1 || w0 != 1 || b0 != 1) {
        std::printf("FAIL PMU m0: ar=%u r=%u aw=%u w=%u b=%u, expected all=1\n",
                    ar0, r0, aw0, w0, b0);
        return 1;
    }
    unsigned ar1 = (unsigned)dut.ar_count[1];
    unsigned ar2 = (unsigned)dut.ar_count[2];
    unsigned aw1 = (unsigned)dut.aw_count[1];
    unsigned aw2 = (unsigned)dut.aw_count[2];
    if (ar1 != 0 || ar2 != 0 || aw1 != 0 || aw2 != 0) {
        std::printf("FAIL PMU: idle masters non-zero (ar=[%u,%u] aw=[%u,%u])\n",
                    ar1, ar2, aw1, aw2);
        return 1;
    }
    std::printf("  OK [S3] PMU m0 ar=r=aw=w=b=1; idle masters 1/2 stay 0\n");

    std::printf("PASS Nic400System: AHB ↔ fabric ↔ APB end-to-end + PMU counts\n");
    return 0;
}
