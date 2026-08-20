// ARCH sim testbench for e203_biu — E203 bus interface unit.
// Tests: reset state, LSU>IFU priority arbitration, the 1-deep command
// pipeline register (accept -> registered command to downstream next cycle,
// clear on downstream handshake), region splitting to PPI/CLINT/PLIC/FIO/MEM
// by upper-16-bit region indication with enable gating, response routing back
// to the issuing initiator via the tgt_*_r registers, downstream
// backpressure, the zero-cycle IFU-to-peripheral error response, and
// biu_active.
//
// NOTE: this replaces a stale tb (VBiu.h) that targeted an earlier,
// simplified revision of this fixture. The fixture was rewritten against the
// real E203 RTL (sirv_gnrl_icb_arbt + buffer + splt inline) and renamed to
// `e203_biu`; the old tb has not compiled since. The fixture's response mux
// registers were also renamed sel_*_r -> tgt_*_r on current main.
//
// Run with:
//   arch sim tests/e203/e203_biu.arch --tb tests/e203/e203_biu_tb.cpp

#include "Ve203_biu.h"
#include <cstdio>
#include <cstdint>

static int fail_count = 0;

#define CHECK(cond, fmt, ...) do { \
    if (!(cond)) { \
        printf("  FAIL: " fmt "\n", ##__VA_ARGS__); \
        fail_count++; \
    } \
} while(0)

static Ve203_biu* dut;

// Region maps (upper 16 bits): PPI 0x4000, CLINT 0x0200, PLIC 0x0C00,
// FIO 0x1000. Anything else (with mem enabled) -> MEM.
static const uint32_t PPI_BASE   = 0x40000000u;
static const uint32_t CLINT_BASE = 0x02000000u;
static const uint32_t PLIC_BASE  = 0x0C000000u;
static const uint32_t FIO_BASE   = 0x10000000u;
static const uint32_t MEM_ADDR   = 0x80001000u;

static void tick() {
    dut->clk = 0; dut->eval();
    dut->clk = 1; dut->eval();
}

static void clear_lsu_cmd() {
    dut->lsu2biu_icb_cmd_valid = 0;
    dut->lsu2biu_icb_cmd_addr = 0;
    dut->lsu2biu_icb_cmd_read = 0;
    dut->lsu2biu_icb_cmd_wdata = 0;
    dut->lsu2biu_icb_cmd_wmask = 0;
    dut->lsu2biu_icb_cmd_burst = 0;
    dut->lsu2biu_icb_cmd_beat = 0;
    dut->lsu2biu_icb_cmd_lock = 0;
    dut->lsu2biu_icb_cmd_excl = 0;
    dut->lsu2biu_icb_cmd_size = 0;
}

static void clear_ifu_cmd() {
    dut->ifu2biu_icb_cmd_valid = 0;
    dut->ifu2biu_icb_cmd_addr = 0;
    dut->ifu2biu_icb_cmd_read = 0;
    dut->ifu2biu_icb_cmd_wdata = 0;
    dut->ifu2biu_icb_cmd_wmask = 0;
    dut->ifu2biu_icb_cmd_burst = 0;
    dut->ifu2biu_icb_cmd_beat = 0;
    dut->ifu2biu_icb_cmd_lock = 0;
    dut->ifu2biu_icb_cmd_excl = 0;
    dut->ifu2biu_icb_cmd_size = 0;
}

static void clear_downstream() {
    dut->ppi_icb_cmd_ready = 0;
    dut->ppi_icb_rsp_valid = 0;
    dut->ppi_icb_rsp_err = 0;
    dut->ppi_icb_rsp_excl_ok = 0;
    dut->ppi_icb_rsp_rdata = 0;
    dut->clint_icb_cmd_ready = 0;
    dut->clint_icb_rsp_valid = 0;
    dut->clint_icb_rsp_err = 0;
    dut->clint_icb_rsp_excl_ok = 0;
    dut->clint_icb_rsp_rdata = 0;
    dut->plic_icb_cmd_ready = 0;
    dut->plic_icb_rsp_valid = 0;
    dut->plic_icb_rsp_err = 0;
    dut->plic_icb_rsp_excl_ok = 0;
    dut->plic_icb_rsp_rdata = 0;
    dut->fio_icb_cmd_ready = 0;
    dut->fio_icb_rsp_valid = 0;
    dut->fio_icb_rsp_err = 0;
    dut->fio_icb_rsp_excl_ok = 0;
    dut->fio_icb_rsp_rdata = 0;
    dut->mem_icb_cmd_ready = 0;
    dut->mem_icb_rsp_valid = 0;
    dut->mem_icb_rsp_err = 0;
    dut->mem_icb_rsp_excl_ok = 0;
    dut->mem_icb_rsp_rdata = 0;
}

static void reset() {
    dut->rst_n = 0;
    dut->ppi_region_indic = PPI_BASE;
    dut->clint_region_indic = CLINT_BASE;
    dut->plic_region_indic = PLIC_BASE;
    dut->fio_region_indic = FIO_BASE;
    dut->ppi_icb_enable = 1;
    dut->clint_icb_enable = 1;
    dut->plic_icb_enable = 1;
    dut->fio_icb_enable = 1;
    dut->mem_icb_enable = 1;
    dut->lsu2biu_icb_rsp_ready = 1;
    dut->ifu2biu_icb_rsp_ready = 1;
    clear_lsu_cmd();
    clear_ifu_cmd();
    clear_downstream();
    for (int i = 0; i < 3; i++) tick();
    dut->rst_n = 1;
    dut->eval();
}

int main() {
    dut = new Ve203_biu;

    // ── Test 1: Reset / idle state ───────────────────────────────────
    printf("Test 1: Reset state\n");
    reset();
    CHECK(dut->ppi_icb_cmd_valid == 0 && dut->clint_icb_cmd_valid == 0 &&
          dut->plic_icb_cmd_valid == 0 && dut->fio_icb_cmd_valid == 0 &&
          dut->mem_icb_cmd_valid == 0, "no downstream cmd at idle");
    CHECK(dut->lsu2biu_icb_rsp_valid == 0, "no lsu rsp at idle, got %d", dut->lsu2biu_icb_rsp_valid);
    CHECK(dut->ifu2biu_icb_rsp_valid == 0, "no ifu rsp at idle, got %d", dut->ifu2biu_icb_rsp_valid);
    CHECK(dut->biu_active == 0, "biu inactive at idle, got %d", dut->biu_active);
    CHECK(dut->lsu2biu_icb_cmd_ready == 0 && dut->ifu2biu_icb_cmd_ready == 0,
          "no ready with no request, got %d/%d",
          dut->lsu2biu_icb_cmd_ready, dut->ifu2biu_icb_cmd_ready);

    // ── Test 2: LSU read -> MEM, full transaction ────────────────────
    printf("Test 2: LSU read -> MEM\n");
    dut->lsu2biu_icb_cmd_valid = 1;
    dut->lsu2biu_icb_cmd_addr = MEM_ADDR;
    dut->lsu2biu_icb_cmd_read = 1;
    dut->lsu2biu_icb_cmd_size = 2;
    dut->mem_icb_cmd_ready = 1;
    dut->eval();
    CHECK(dut->lsu2biu_icb_cmd_ready == 1, "lsu granted with mem ready, got %d",
          dut->lsu2biu_icb_cmd_ready);
    CHECK(dut->ifu2biu_icb_cmd_ready == 0, "ifu blocked while lsu wins, got %d",
          dut->ifu2biu_icb_cmd_ready);
    CHECK(dut->biu_active == 1, "active with pending request, got %d", dut->biu_active);
    CHECK(dut->mem_icb_cmd_valid == 0, "cmd is pipelined — nothing downstream yet, got %d",
          dut->mem_icb_cmd_valid);
    tick();                             // command accepted into pipeline reg
    clear_lsu_cmd();
    dut->eval();
    // Registered command now presented to MEM only.
    CHECK(dut->mem_icb_cmd_valid == 1, "mem cmd valid from pipeline reg, got %d",
          dut->mem_icb_cmd_valid);
    CHECK(dut->ppi_icb_cmd_valid == 0 && dut->clint_icb_cmd_valid == 0 &&
          dut->plic_icb_cmd_valid == 0 && dut->fio_icb_cmd_valid == 0,
          "other targets stay quiet");
    CHECK(dut->mem_icb_cmd_addr == MEM_ADDR, "mem addr, got 0x%08x", dut->mem_icb_cmd_addr);
    CHECK(dut->mem_icb_cmd_read == 1, "mem read flag, got %d", dut->mem_icb_cmd_read);
    CHECK(dut->mem_icb_cmd_size == 2, "mem size, got %d", dut->mem_icb_cmd_size);
    CHECK(dut->biu_active == 1, "active with cmd in flight, got %d", dut->biu_active);
    tick();                             // mem consumes the command (ready held high)
    dut->mem_icb_cmd_ready = 0;
    dut->eval();
    CHECK(dut->mem_icb_cmd_valid == 0, "cmd reg clears after downstream handshake, got %d",
          dut->mem_icb_cmd_valid);
    // MEM responds; response routes to the LSU.
    dut->mem_icb_rsp_valid = 1;
    dut->mem_icb_rsp_rdata = 0xDEADBEEFu;
    dut->eval();
    CHECK(dut->lsu2biu_icb_rsp_valid == 1, "lsu rsp valid, got %d", dut->lsu2biu_icb_rsp_valid);
    CHECK(dut->ifu2biu_icb_rsp_valid == 0, "ifu rsp stays quiet, got %d", dut->ifu2biu_icb_rsp_valid);
    CHECK(dut->lsu2biu_icb_rsp_rdata == 0xDEADBEEFu, "lsu rdata, got 0x%08x",
          dut->lsu2biu_icb_rsp_rdata);
    CHECK(dut->lsu2biu_icb_rsp_err == 0, "no err on clean rsp, got %d", dut->lsu2biu_icb_rsp_err);
    CHECK(dut->mem_icb_rsp_ready == 1, "mem rsp ready follows lsu rsp ready, got %d",
          dut->mem_icb_rsp_ready);
    tick();                             // response handshake clears out_flag
    dut->mem_icb_rsp_valid = 0;
    dut->mem_icb_rsp_rdata = 0;
    dut->eval();
    CHECK(dut->biu_active == 0, "inactive after transaction drains, got %d", dut->biu_active);

    // ── Test 3: LSU write -> PPI with error response ─────────────────
    printf("Test 3: LSU write -> PPI, err rsp\n");
    reset();
    dut->lsu2biu_icb_cmd_valid = 1;
    dut->lsu2biu_icb_cmd_addr = PPI_BASE + 0x24;
    dut->lsu2biu_icb_cmd_read = 0;
    dut->lsu2biu_icb_cmd_wdata = 0xA5A5A5A5u;
    dut->lsu2biu_icb_cmd_wmask = 0xF;
    dut->ppi_icb_cmd_ready = 1;
    dut->eval();
    CHECK(dut->lsu2biu_icb_cmd_ready == 1, "lsu granted with ppi ready, got %d",
          dut->lsu2biu_icb_cmd_ready);
    tick();
    clear_lsu_cmd();
    dut->eval();
    CHECK(dut->ppi_icb_cmd_valid == 1, "ppi cmd valid, got %d", dut->ppi_icb_cmd_valid);
    CHECK(dut->mem_icb_cmd_valid == 0, "mem stays quiet, got %d", dut->mem_icb_cmd_valid);
    CHECK(dut->ppi_icb_cmd_addr == PPI_BASE + 0x24, "ppi addr, got 0x%08x", dut->ppi_icb_cmd_addr);
    CHECK(dut->ppi_icb_cmd_read == 0, "ppi write flag, got %d", dut->ppi_icb_cmd_read);
    CHECK(dut->ppi_icb_cmd_wdata == 0xA5A5A5A5u, "ppi wdata, got 0x%08x", dut->ppi_icb_cmd_wdata);
    CHECK(dut->ppi_icb_cmd_wmask == 0xF, "ppi wmask, got 0x%x", dut->ppi_icb_cmd_wmask);
    tick();                             // ppi consumes
    dut->ppi_icb_cmd_ready = 0;
    dut->ppi_icb_rsp_valid = 1;
    dut->ppi_icb_rsp_err = 1;
    dut->eval();
    CHECK(dut->lsu2biu_icb_rsp_valid == 1, "lsu rsp valid from ppi, got %d",
          dut->lsu2biu_icb_rsp_valid);
    CHECK(dut->lsu2biu_icb_rsp_err == 1, "err routes to lsu, got %d", dut->lsu2biu_icb_rsp_err);
    CHECK(dut->ppi_icb_rsp_ready == 1, "ppi rsp ready, got %d", dut->ppi_icb_rsp_ready);
    dut->ppi_icb_rsp_valid = 0;
    dut->ppi_icb_rsp_err = 0;
    dut->eval();

    // ── Test 4: IFU fetch -> MEM, response routes to IFU ─────────────
    printf("Test 4: IFU fetch -> MEM\n");
    reset();
    dut->ifu2biu_icb_cmd_valid = 1;
    dut->ifu2biu_icb_cmd_addr = MEM_ADDR + 0x40;
    dut->ifu2biu_icb_cmd_read = 1;
    dut->mem_icb_cmd_ready = 1;
    dut->eval();
    CHECK(dut->ifu2biu_icb_cmd_ready == 1, "ifu granted with no lsu contender, got %d",
          dut->ifu2biu_icb_cmd_ready);
    CHECK(dut->ifu2biu_icb_rsp_err == 0, "mem fetch is not an error, got %d",
          dut->ifu2biu_icb_rsp_err);
    tick();
    clear_ifu_cmd();
    dut->eval();
    CHECK(dut->mem_icb_cmd_valid == 1, "mem cmd from ifu fetch, got %d", dut->mem_icb_cmd_valid);
    CHECK(dut->mem_icb_cmd_addr == MEM_ADDR + 0x40, "mem addr, got 0x%08x", dut->mem_icb_cmd_addr);
    tick();                             // mem consumes
    dut->mem_icb_cmd_ready = 0;
    dut->mem_icb_rsp_valid = 1;
    dut->mem_icb_rsp_rdata = 0x00000013u;   // nop
    dut->eval();
    CHECK(dut->ifu2biu_icb_rsp_valid == 1, "ifu rsp valid, got %d", dut->ifu2biu_icb_rsp_valid);
    CHECK(dut->lsu2biu_icb_rsp_valid == 0, "lsu rsp stays quiet, got %d",
          dut->lsu2biu_icb_rsp_valid);
    CHECK(dut->ifu2biu_icb_rsp_rdata == 0x00000013u, "ifu rdata, got 0x%08x",
          dut->ifu2biu_icb_rsp_rdata);
    CHECK(dut->mem_icb_rsp_ready == 1, "mem rsp ready follows ifu rsp ready, got %d",
          dut->mem_icb_rsp_ready);
    dut->mem_icb_rsp_valid = 0;
    dut->mem_icb_rsp_rdata = 0;
    dut->eval();

    // ── Test 5: LSU wins arbitration over IFU ────────────────────────
    printf("Test 5: LSU > IFU arbitration\n");
    reset();
    dut->lsu2biu_icb_cmd_valid = 1;
    dut->lsu2biu_icb_cmd_addr = MEM_ADDR;
    dut->lsu2biu_icb_cmd_read = 0;
    dut->lsu2biu_icb_cmd_wdata = 0x11111111u;
    dut->ifu2biu_icb_cmd_valid = 1;
    dut->ifu2biu_icb_cmd_addr = MEM_ADDR + 0x100;
    dut->ifu2biu_icb_cmd_read = 1;
    dut->mem_icb_cmd_ready = 1;
    dut->eval();
    CHECK(dut->lsu2biu_icb_cmd_ready == 1, "LSU gets the grant, got %d", dut->lsu2biu_icb_cmd_ready);
    CHECK(dut->ifu2biu_icb_cmd_ready == 0, "IFU must be blocked, got %d", dut->ifu2biu_icb_cmd_ready);
    tick();
    clear_lsu_cmd();
    clear_ifu_cmd();
    dut->eval();
    CHECK(dut->mem_icb_cmd_addr == MEM_ADDR, "LSU address was accepted, got 0x%08x",
          dut->mem_icb_cmd_addr);
    CHECK(dut->mem_icb_cmd_read == 0, "LSU write was accepted, got read=%d", dut->mem_icb_cmd_read);
    tick();                             // drain
    dut->mem_icb_cmd_ready = 0;
    dut->eval();

    // ── Test 6: IFU access to peripheral space -> zero-cycle error ───
    printf("Test 6: IFU -> PPI error\n");
    reset();
    dut->ifu2biu_icb_cmd_valid = 1;
    dut->ifu2biu_icb_cmd_addr = PPI_BASE + 0x10;
    dut->ifu2biu_icb_cmd_read = 1;
    dut->ppi_icb_cmd_ready = 1;
    dut->eval();
    CHECK(dut->ifu2biu_icb_rsp_valid == 1, "zero-cycle error rsp valid, got %d",
          dut->ifu2biu_icb_rsp_valid);
    CHECK(dut->ifu2biu_icb_rsp_err == 1, "error flagged, got %d", dut->ifu2biu_icb_rsp_err);
    CHECK(dut->ifu2biu_icb_cmd_ready == 0, "errored fetch is not accepted, got %d",
          dut->ifu2biu_icb_cmd_ready);
    CHECK(dut->lsu2biu_icb_rsp_valid == 0, "lsu rsp stays quiet, got %d",
          dut->lsu2biu_icb_rsp_valid);
    tick();
    dut->eval();
    CHECK(dut->ppi_icb_cmd_valid == 0, "request must NOT reach the peripheral, got %d",
          dut->ppi_icb_cmd_valid);
    clear_ifu_cmd();
    dut->ppi_icb_cmd_ready = 0;
    dut->eval();

    // ── Test 7: Region split to CLINT / PLIC / FIO ───────────────────
    printf("Test 7: CLINT/PLIC/FIO decode\n");
    struct { uint32_t addr; const char* name; } cases[3] = {
        { CLINT_BASE + 0x4000, "clint" },
        { PLIC_BASE  + 0x2000, "plic"  },
        { FIO_BASE   + 0x0800, "fio"   },
    };
    for (int i = 0; i < 3; i++) {
        reset();
        dut->lsu2biu_icb_cmd_valid = 1;
        dut->lsu2biu_icb_cmd_addr = cases[i].addr;
        dut->lsu2biu_icb_cmd_read = 1;
        dut->clint_icb_cmd_ready = 1;
        dut->plic_icb_cmd_ready = 1;
        dut->fio_icb_cmd_ready = 1;
        dut->eval();
        CHECK(dut->lsu2biu_icb_cmd_ready == 1, "%s: lsu granted, got %d", cases[i].name,
              dut->lsu2biu_icb_cmd_ready);
        tick();
        clear_lsu_cmd();
        dut->eval();
        int clint_v = dut->clint_icb_cmd_valid, plic_v = dut->plic_icb_cmd_valid,
            fio_v = dut->fio_icb_cmd_valid;
        CHECK(clint_v == (i == 0), "%s: clint_v=%d", cases[i].name, clint_v);
        CHECK(plic_v == (i == 1), "%s: plic_v=%d", cases[i].name, plic_v);
        CHECK(fio_v == (i == 2), "%s: fio_v=%d", cases[i].name, fio_v);
        CHECK(dut->ppi_icb_cmd_valid == 0 && dut->mem_icb_cmd_valid == 0,
              "%s: ppi/mem quiet", cases[i].name);
    }

    // ── Test 8: Enable gating — disabled FIO region falls to MEM ─────
    printf("Test 8: fio_icb_enable=0 falls through to MEM\n");
    reset();
    dut->fio_icb_enable = 0;
    dut->lsu2biu_icb_cmd_valid = 1;
    dut->lsu2biu_icb_cmd_addr = FIO_BASE + 0x800;
    dut->lsu2biu_icb_cmd_read = 1;
    dut->mem_icb_cmd_ready = 1;
    dut->eval();
    CHECK(dut->lsu2biu_icb_cmd_ready == 1, "granted via mem, got %d", dut->lsu2biu_icb_cmd_ready);
    tick();
    clear_lsu_cmd();
    dut->eval();
    CHECK(dut->mem_icb_cmd_valid == 1, "mem takes the disabled-fio address, got %d",
          dut->mem_icb_cmd_valid);
    CHECK(dut->fio_icb_cmd_valid == 0, "fio stays quiet when disabled, got %d",
          dut->fio_icb_cmd_valid);

    // ── Test 9: Downstream backpressure blocks acceptance ────────────
    printf("Test 9: downstream backpressure\n");
    reset();
    dut->lsu2biu_icb_cmd_valid = 1;
    dut->lsu2biu_icb_cmd_addr = MEM_ADDR;
    dut->lsu2biu_icb_cmd_read = 1;
    dut->mem_icb_cmd_ready = 0;         // mem not ready
    dut->eval();
    CHECK(dut->lsu2biu_icb_cmd_ready == 0, "not granted while mem stalls, got %d",
          dut->lsu2biu_icb_cmd_ready);
    tick();
    dut->eval();
    CHECK(dut->mem_icb_cmd_valid == 0, "nothing accepted into the pipeline, got %d",
          dut->mem_icb_cmd_valid);
    dut->mem_icb_cmd_ready = 1;
    dut->eval();
    CHECK(dut->lsu2biu_icb_cmd_ready == 1, "granted once mem frees up, got %d",
          dut->lsu2biu_icb_cmd_ready);
    tick();
    clear_lsu_cmd();
    dut->eval();
    CHECK(dut->mem_icb_cmd_valid == 1, "command lands after stall, got %d", dut->mem_icb_cmd_valid);

    printf("\n=== e203_biu: %d failure(s) ===\n", fail_count);
    return fail_count ? 1 : 0;
}
