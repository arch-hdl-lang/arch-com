// ARCH sim testbench for e203_soc_top — E203 SoC integration.
//
// e203_soc_top instantiates fifteen blocks: e203_core_top plus the ITCM and
// DTCM controller/RAM pairs, the CLINT timer behind an ICB-to-register adapter,
// the SRAM fabric (external port + core MEM ICB -> arbiter -> sram_ctrl), the
// FIO block, the PPI ICB-to-APB bridge with GPIO/UART/SPI behind it, the
// interrupt controller and the debug module. All of those have their own leaf
// testbenches, so this one covers the SoC's own wiring: the address map, the
// externally drivable fabric paths, and the reset state of the pins.
//
// Tests: reset state of every output pin; the ITCM loader writing through the
// ext2itcm ICB into the ITCM RAM and the core's boot fetch reading it back; the
// external ICB master's write/read round trip through the arbiter and SRAM
// controller; the debug APB port's register file and its halt request reaching
// the core; and the peripheral pin defaults.
//
// NOTE: this replaces a stale tb that predates the PR #843 rewiring of these
// fixtures against the real ICB fabric and has not compiled since.
//
// Run with (all fifteen sub-designs plus the core hierarchy):
//   arch sim tests/e203/e203_soc_top.arch tests/e203/e203_core_top.arch ... \
//            --tb tests/e203/e203_soc_top_tb.cpp
// (the full list is the arch_files entry for e203__e203_soc_top in
//  tests/arch_sim_manifest.json)
//
// ── KNOWN ISSUE 1: the core's fetch pipeline deadlocks on the first ICB
// response, so the CPU never executes an instruction. The cause is inside
// e203_ifu_top — the ift2icb bridge's `ifu_req_ready = icb_cmd_ready &
// ~buf_full` versus ifetch's `ifu_rsp2ir_ready = ... & ifu_req_ready ...`, so
// once the bridge's one-entry response buffer fills nothing drains it. Written
// up in full in e203_ifu_top_tb.cpp (KNOWN ISSUE 1) and reported separately.
//
// Everything the CPU would otherwise reach is therefore unreachable from
// inside this SoC, which shapes what the tests below can claim:
//   * The PPI ICB is driven only by the core's BIU, so the APB bridge and the
//     GPIO/UART/SPI register files can never be written. Test 7 checks the pin
//     defaults and pins the fact that gpio_in alone cannot raise gpio_irq (the
//     rise/fall interrupt-enable registers are APB-writable only).
//   * The CLINT ICB is likewise core-only, and e203_clint_timer resets
//     mtimecmp to 0xFFFF_FFFF_FFFF_FFFF, so tmr_irq cannot be made to fire.
//     Test 8 pins it low rather than asserting a propagation that no stimulus
//     can produce.
//   * The core's MEM ICB never requests, so the SRAM arbiter is never actually
//     contended; Test 4 exercises the external master alone.
// The ITCM loader and the external SRAM port are driven from top-level pins and
// are fully exercised (Tests 2 and 4).
//
// ── KNOWN ISSUE 2: e203_sram_ctrl ignores icb_cmd_wmask — it drives the SRAM's
// write enable from `~icb_cmd_read` and passes the full 32-bit wdata, with no
// per-byte masking. A partial-width store therefore overwrites the whole word.
// Test 5 pins the actual behavior. Same family as the already-filed arch#869
// (dtcm wr_be zeroes unwritten lanes).
//
// ── KNOWN ISSUE 3 (arch#800): pc_rtvec is dead inside e203_ifu_ifetch, so the
// core boots from 0x0. The SoC memory map puts the ITCM at 0x0000_xxxx, so the
// boot fetch does land in the ITCM; all address expectations below assume the
// 0x0 boot address.
//
// ── KNOWN ISSUE 4: e203_exu_decode and e203_exu_alu disagree on the dec_info
// bus layout (see e203_exu_top_tb.cpp, KNOWN ISSUE 1). Unreachable here behind
// KNOWN ISSUE 1; noted so it is not rediscovered.

#include "Ve203_soc_top.h"
#include <cstdio>
#include <cstdint>

static int fail_count = 0;

#define CHECK(cond, fmt, ...) do { \
    if (!(cond)) { \
        printf("  FAIL: " fmt "\n", ##__VA_ARGS__); \
        fail_count++; \
    } \
} while(0)

static Ve203_soc_top* dut;

// RV32 `add x3, x1, x2` and `add x4, x1, x2` — two distinguishable words, used
// to prove the ITCM loader's address decode and the 64-bit fetch packing.
static const uint32_t ITCM_WORD0 = 0x002081B3u;
static const uint32_t ITCM_WORD1 = 0x00208233u;

// The sim emitter runs a fixed two comb passes per eval(). This hierarchy is
// six levels deep in places (soc -> core -> ifu_top -> ifetch -> minidec ->
// decode), so settle generously before sampling combinational outputs.
static void settle() { for (int i = 0; i < 6; i++) dut->eval(); }

static void tick() {
    dut->clk = 0; settle();
    dut->clk = 1; settle();
}

// Drive every input to a defined value and hold reset for 3 ticks. Note that
// timer_rst is the SoC's one active-high sync reset (the CLINT timer island),
// so it is driven inverted relative to rst_n.
static void reset() {
    dut->rst_n = 0;
    dut->timer_rst = 1;
    dut->clk = 0;
    dut->pc_rtvec = 0x80000000;
    dut->itcm_wr_en = 0;
    dut->itcm_wr_addr = 0;
    dut->itcm_wr_data = 0;
    dut->ext_cmd_valid = 0;
    dut->ext_cmd_addr = 0;
    dut->ext_cmd_wdata = 0;
    dut->ext_cmd_wmask = 0xF;
    dut->ext_cmd_read = 1;
    dut->gpio_in = 0;
    dut->uart_rx = 1;
    dut->spi_miso = 0;
    dut->fio_in_0 = 0;
    dut->fio_in_1 = 0;
    dut->dbg_psel = 0;
    dut->dbg_penable = 0;
    dut->dbg_paddr = 0;
    dut->dbg_pwdata = 0;
    dut->dbg_pwrite = 0;
    for (int i = 0; i < 3; i++) tick();
    dut->rst_n = 1;
    dut->timer_rst = 0;
    settle();
}

// One ITCM loader write: itcm_wr_addr is a *word* address, which the SoC's
// ext2itcm adapter shifts into a byte address before handing it to the ITCM
// controller's external ICB port.
static void itcm_write(uint16_t word_addr, uint32_t data) {
    dut->itcm_wr_en = 1;
    dut->itcm_wr_addr = word_addr;
    dut->itcm_wr_data = data;
    settle();
    tick();
    dut->itcm_wr_en = 0;
    settle();
}

// One transaction on the external SRAM ICB port. Returns the read data
// captured in the cycle the response is valid.
static uint32_t ext_txn(uint32_t byte_addr, uint32_t wdata, uint8_t read, uint8_t wmask) {
    dut->ext_cmd_valid = 1;
    dut->ext_cmd_addr = byte_addr;
    dut->ext_cmd_wdata = wdata;
    dut->ext_cmd_read = read;
    dut->ext_cmd_wmask = wmask;
    settle();
    CHECK(dut->ext_cmd_ready == 1, "ext_cmd_ready should be 1 with an idle SRAM fabric, got %d",
          dut->ext_cmd_ready);
    tick();
    dut->ext_cmd_valid = 0;
    settle();
    CHECK(dut->ext_rsp_valid == 1, "ext_rsp_valid should be 1 one cycle after the command, got %d",
          dut->ext_rsp_valid);
    CHECK(dut->ext_rsp_err == 0, "ext_rsp_err should be 0 for an in-range access, got %d",
          dut->ext_rsp_err);
    uint32_t rdata = dut->ext_rsp_rdata;
    tick();
    settle();
    return rdata;
}

// One APB transfer on the debug port (setup phase, then enable phase).
static void dbg_apb_write(uint32_t addr, uint32_t data) {
    dut->dbg_psel = 1; dut->dbg_penable = 0;
    dut->dbg_paddr = addr; dut->dbg_pwdata = data; dut->dbg_pwrite = 1;
    settle();
    tick();
    dut->dbg_penable = 1;
    settle();
    tick();
    dut->dbg_psel = 0; dut->dbg_penable = 0; dut->dbg_pwrite = 0;
    settle();
}

static uint32_t dbg_apb_read(uint32_t addr) {
    dut->dbg_psel = 1; dut->dbg_penable = 0;
    dut->dbg_paddr = addr; dut->dbg_pwrite = 0;
    settle();
    tick();
    dut->dbg_penable = 1;
    settle();
    uint32_t v = dut->dbg_prdata;
    tick();
    dut->dbg_psel = 0; dut->dbg_penable = 0;
    settle();
    return v;
}

int main() {
    dut = new Ve203_soc_top;

    // ── Test 1: Reset state of the pins ──────────────────────────────
    printf("Test 1: Reset state\n");
    reset();
    CHECK(dut->inspect_pc == 0x0, "inspect_pc should be 0 after reset, got 0x%08x", dut->inspect_pc);
    CHECK(dut->core_wfi == 0, "core_wfi should be 0 after reset, got %d", dut->core_wfi);
    CHECK(dut->gpio_out == 0, "gpio_out should be 0 after reset, got 0x%08x", dut->gpio_out);
    CHECK(dut->gpio_oe == 0, "gpio_oe should be 0 after reset (all pins inputs), got 0x%08x", dut->gpio_oe);
    CHECK(dut->gpio_irq == 0, "gpio_irq should be 0 after reset, got %d", dut->gpio_irq);
    CHECK(dut->uart_tx == 1, "uart_tx idles high, got %d", dut->uart_tx);
    CHECK(dut->uart_irq == 0, "uart_irq should be 0 after reset, got %d", dut->uart_irq);
    CHECK(dut->spi_sclk == 0, "spi_sclk should be 0 after reset, got %d", dut->spi_sclk);
    CHECK(dut->spi_cs_n == 1, "spi_cs_n idles high (deselected), got %d", dut->spi_cs_n);
    CHECK(dut->spi_irq == 0, "spi_irq should be 0 after reset, got %d", dut->spi_irq);
    CHECK(dut->fio_out_0 == 0, "fio_out_0 should be 0 after reset, got 0x%08x", dut->fio_out_0);
    CHECK(dut->fio_out_1 == 0, "fio_out_1 should be 0 after reset, got 0x%08x", dut->fio_out_1);
    CHECK(dut->dbg_prdata == 0, "dbg_prdata should be 0 with no APB select, got 0x%08x", dut->dbg_prdata);
    CHECK(dut->ext_rsp_valid == 0, "ext_rsp_valid should be 0 after reset, got %d", dut->ext_rsp_valid);
    // The boot fetch is already on the ITCM bus: the map puts the ITCM at
    // 0x0000_xxxx and KNOWN ISSUE 3 means the core boots there.
    CHECK(dut->_let_ifu2itcm_icb_cmd_valid == 1, "the boot fetch should target the ITCM, got cmd_valid %d",
          dut->_let_ifu2itcm_icb_cmd_valid);
    CHECK(dut->_let_ppi_icb_cmd_valid == 0, "no PPI traffic expected after reset, got %d",
          dut->_let_ppi_icb_cmd_valid);
    CHECK(dut->_let_clint_icb_cmd_valid == 0, "no CLINT traffic expected after reset, got %d",
          dut->_let_clint_icb_cmd_valid);
    CHECK(dut->_let_fio_icb_cmd_valid == 0, "no FIO traffic expected after reset, got %d",
          dut->_let_fio_icb_cmd_valid);
    CHECK(dut->_let_mem_icb_cmd_valid == 0, "no MEM traffic expected after reset, got %d",
          dut->_let_mem_icb_cmd_valid);

    // ── Test 2: ITCM loader writes land in the ITCM RAM ──────────────
    // itcm_wr_* -> ext2itcm address adapter -> e203_itcm_ctrl -> e203_itcm_ram,
    // read back out through the core's 64-bit instruction-fetch ICB. Two
    // different words prove the word-address decode and the 64-bit packing
    // (word 1 in the upper half, word 0 in the lower).
    printf("Test 2: ITCM loader -> ITCM RAM -> fetch read-back\n");
    reset();
    itcm_write(0, ITCM_WORD0);
    itcm_write(1, ITCM_WORD1);
    settle();
    CHECK(dut->_let_ifu2itcm_icb_rsp_rdata == (((uint64_t)ITCM_WORD1 << 32) | ITCM_WORD0),
          "the ITCM should return {word1,word0} = 0x%08x%08x, got 0x%016llx",
          ITCM_WORD1, ITCM_WORD0, (unsigned long long)dut->_let_ifu2itcm_icb_rsp_rdata);
    CHECK(dut->_let_ifu2itcm_icb_cmd_valid == 1, "the fetch should still be offered, got cmd_valid %d",
          dut->_let_ifu2itcm_icb_cmd_valid);
    CHECK(dut->_let_ifu2itcm_icb_cmd_addr == 0x0002, "the boot fetch address should be 0x0002, got 0x%04x",
          dut->_let_ifu2itcm_icb_cmd_addr);

    // A later word must not disturb the first two.
    itcm_write(2, 0xA5A5A5A5u);
    settle();
    CHECK(dut->_let_ifu2itcm_icb_rsp_rdata == (((uint64_t)ITCM_WORD1 << 32) | ITCM_WORD0),
          "words 0/1 must survive a write to word 2, got 0x%016llx",
          (unsigned long long)dut->_let_ifu2itcm_icb_rsp_rdata);

    // ── Test 3: Boot fetch handshake, then the core locks up ─────────
    // The ITCM controller answers the fetch on its own; after that response the
    // pipeline never releases the ift2icb buffer (KNOWN ISSUE 1).
    printf("Test 3: Boot fetch + core lockup (KNOWN ISSUE 1)\n");
    tick(); settle();
    CHECK(dut->_let_ifu2itcm_icb_rsp_valid == 1, "the ITCM should answer the fetch, got rsp_valid %d",
          dut->_let_ifu2itcm_icb_rsp_valid);
    CHECK(dut->inspect_pc == 0x2, "the pc should have advanced to 0x2, got 0x%08x", dut->inspect_pc);
    for (int i = 0; i < 8; i++) {
        tick(); settle();
        CHECK(dut->inspect_pc == 0x2,
              "KNOWN ISSUE 1: the pc stays frozen at 0x2 (cycle %d), got 0x%08x", i, dut->inspect_pc);
        CHECK(dut->_let_ifu2itcm_icb_cmd_valid == 0,
              "KNOWN ISSUE 1: fetching never resumes (cycle %d), got cmd_valid %d",
              i, dut->_let_ifu2itcm_icb_cmd_valid);
        CHECK(dut->_let_ppi_icb_cmd_valid == 0, "no PPI traffic while locked (cycle %d)", i);
        CHECK(dut->_let_clint_icb_cmd_valid == 0, "no CLINT traffic while locked (cycle %d)", i);
        CHECK(dut->_let_mem_icb_cmd_valid == 0, "no MEM traffic while locked (cycle %d)", i);
    }

    // ── Test 4: External ICB master -> arbiter -> SRAM controller ────
    // ext_cmd_* -> e203_icb_arbt (m1) -> e203_sram_ctrl -> SramBank, and back
    // out on ext_rsp_*. Two addresses, so a read cannot pass by returning
    // whatever was last written.
    printf("Test 4: External ICB write/read round trip through the SRAM fabric\n");
    reset();
    ext_txn(0x40, 0xDEADBEEFu, 0, 0xF);
    ext_txn(0x44, 0x12345678u, 0, 0xF);
    {
        uint32_t v = ext_txn(0x40, 0, 1, 0xF);
        CHECK(v == 0xDEADBEEFu, "SRAM read at 0x40 should return 0xDEADBEEF, got 0x%08x", v);
        v = ext_txn(0x44, 0, 1, 0xF);
        CHECK(v == 0x12345678u, "SRAM read at 0x44 should return 0x12345678, got 0x%08x", v);
        v = ext_txn(0x48, 0, 1, 0xF);
        CHECK(v == 0, "an untouched SRAM word should read 0, got 0x%08x", v);
    }

    // ── Test 5: SRAM write mask is ignored (KNOWN ISSUE 2) ───────────
    printf("Test 5: SRAM byte-write mask (KNOWN ISSUE 2)\n");
    ext_txn(0x40, 0xFFFFFFFFu, 0, 0x1);      // only byte 0 requested
    {
        uint32_t v = ext_txn(0x40, 0, 1, 0xF);
        CHECK(v == 0xFFFFFFFFu,
              "KNOWN ISSUE 2: e203_sram_ctrl ignores wmask, so the whole word is overwritten; "
              "expected 0xFFFFFFFF, got 0x%08x", v);
    }

    // ── Test 6: Debug APB port and the halt request to the core ──────
    // dbg_p* -> e203_debug_module register file -> dbg_prdata, and its halt_req
    // output into the core's dbg_halt_r / dbg_irq_r inputs.
    printf("Test 6: Debug APB register file + halt request\n");
    reset();
    CHECK(dbg_apb_read(0x10) == 0, "dmcontrol should read 0 out of reset");
    CHECK(dut->_let_dbg_halt_req == 0, "halt_req should be 0 out of reset, got %d", dut->_let_dbg_halt_req);
    // dmcontrol: bit31 = haltreq, bit0 = dmactive. halt_req is their AND.
    dbg_apb_write(0x10, 0x80000001u);
    settle();
    CHECK(dbg_apb_read(0x10) == 0x80000001u, "dmcontrol should read back haltreq|dmactive");
    CHECK(dut->_let_dbg_halt_req == 1, "halt_req should reach the core once haltreq and dmactive are set, got %d",
          dut->_let_dbg_halt_req);
    // dmactive low gates the halt request even with haltreq still set.
    dbg_apb_write(0x10, 0x80000000u);
    settle();
    CHECK(dut->_let_dbg_halt_req == 0, "halt_req should be gated by dmactive, got %d", dut->_let_dbg_halt_req);
    // data0 is a plain read/write register.
    dbg_apb_write(0x04, 0x12345678u);
    CHECK(dbg_apb_read(0x04) == 0x12345678u, "data0 should read back what was written");
    CHECK(dbg_apb_read(0x99) == 0, "an unmapped debug register should read 0");

    // ── Test 7: Peripheral pins and the unreachable PPI path ─────────
    // The GPIO interrupt-enable registers are APB-writable only, and the APB
    // bridge sits behind the core's PPI ICB, so gpio_in alone cannot raise
    // gpio_irq. Pinned as observed rather than asserting an unreachable path.
    printf("Test 7: Peripheral pins (PPI path unreachable — KNOWN ISSUE 1)\n");
    reset();
    dut->gpio_in = 0x00000000;
    settle(); tick(); settle();
    dut->gpio_in = 0xFFFFFFFFu;             // a rising edge on every pin
    settle();
    for (int i = 0; i < 3; i++) {
        tick(); settle();
        CHECK(dut->gpio_irq == 0,
              "KNOWN ISSUE 1: gpio_irq cannot fire with the enables unwritten (cycle %d), got %d",
              i, dut->gpio_irq);
        CHECK(dut->gpio_out == 0, "gpio_out stays 0 with no APB write (cycle %d), got 0x%08x",
              i, dut->gpio_out);
        CHECK(dut->gpio_oe == 0, "gpio_oe stays 0 with no APB write (cycle %d), got 0x%08x",
              i, dut->gpio_oe);
        CHECK(dut->uart_tx == 1, "uart_tx stays idle high (cycle %d), got %d", i, dut->uart_tx);
        CHECK(dut->spi_cs_n == 1, "spi_cs_n stays deselected (cycle %d), got %d", i, dut->spi_cs_n);
    }
    // FIO outputs are register-mapped behind the core's FIO ICB, so the input
    // pins cannot reach them either.
    dut->fio_in_0 = 0xCAFEBABEu;
    dut->fio_in_1 = 0x5A5A5A5Au;
    settle(); tick(); settle();
    CHECK(dut->fio_out_0 == 0, "fio_out_0 is register-driven, not a pin passthrough, got 0x%08x",
          dut->fio_out_0);
    CHECK(dut->fio_out_1 == 0, "fio_out_1 is register-driven, not a pin passthrough, got 0x%08x",
          dut->fio_out_1);

    // ── Test 8: CLINT timer interrupt stays low (KNOWN ISSUE 1) ──────
    // mtimecmp resets to all ones and is writable only through the core's CLINT
    // ICB, so no stimulus available here can make tmr_irq assert.
    printf("Test 8: CLINT timer interrupt (unprogrammable — KNOWN ISSUE 1)\n");
    reset();
    for (int i = 0; i < 16; i++) {
        tick(); settle();
        CHECK(dut->_let_tmr_irq_w == 0,
              "KNOWN ISSUE 1: mtimecmp cannot be programmed, so tmr_irq stays 0 (cycle %d), got %d",
              i, dut->_let_tmr_irq_w);
    }
    CHECK(dut->core_wfi == 0, "core_wfi should still be 0 with no instruction executed, got %d",
          dut->core_wfi);

    printf("\n=== e203_soc_top: %d failure(s) ===\n", fail_count);
    return fail_count ? 1 : 0;
}
