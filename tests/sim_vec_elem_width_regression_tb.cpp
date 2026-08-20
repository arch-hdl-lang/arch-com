// Vec-element width-inference regression: concat of Vec elements must
// shift by the declared element width (32 / 64 bits), not by
// widths[name]/count (8 / 4 pre-fix), and a runtime bit-select on a
// Vec<UInt<64>,_> element must accept legal bit indices up to 63.
#include "Vsim_vec_elem_width_regression.h"
#include <cstdint>
#include <cstdio>
static Vsim_vec_elem_width_regression dut;
static int pass = 0, fail = 0;
#define CHECK(c, m, ...) do { if (c) { printf("  PASS: " m "\n", ##__VA_ARGS__); ++pass; } \
                              else  { printf("  FAIL: " m "\n", ##__VA_ARGS__); ++fail; } } while (0)

static void tick() {
    dut.clk = 0; dut.eval();
    dut.clk = 1; dut.eval();
}

static void write_lo(unsigned idx, uint32_t val) {
    dut.wr_en = 1; dut.wr_idx = idx; dut.wr_lo = val; tick(); dut.wr_en = 0;
}

int main() {
    dut.clk = 0; dut.rst_n = 1;
    dut.wr_en = 0; dut.wr_idx = 0; dut.wr_lo = 0;
    dut.wr_idx_w = 0; dut.wr_hi = 0; dut.bit_sel = 0;
    dut.eval();

    // v[0] and v[1] get distinct byte patterns; {v[1], v[0]} must place
    // v[1] at bits [63:32].
    dut.wr_en = 1;
    dut.wr_idx = 0; dut.wr_lo = 0xAAAA5555u;
    dut.wr_idx_w = 0; dut.wr_hi = 0x123456789ABCDEF0ull;
    tick();
    dut.wr_idx = 1; dut.wr_lo = 0xDEADBEEFu;
    dut.wr_idx_w = 1; dut.wr_hi = 0x0FEDCBA987654321ull;
    tick();
    dut.wr_en = 0; dut.eval();

    CHECK(dut.rd_pair == 0xDEADBEEFAAAA5555ull,
          "{v[1], v[0]} concat shifts by 32 (got 0x%016llx)",
          (unsigned long long)dut.rd_pair);

    // {w[1], w[0]} is 128 bits: w[0] in words 0..1, w[1] in words 2..3.
    uint64_t cat_lo = (uint64_t)dut.rd_cat._data[0]
                    | ((uint64_t)dut.rd_cat._data[1] << 32);
    uint64_t cat_hi = (uint64_t)dut.rd_cat._data[2]
                    | ((uint64_t)dut.rd_cat._data[3] << 32);
    CHECK(cat_lo == 0x123456789ABCDEF0ull,
          "{w[1], w[0]} low 64 bits = w[0] (got 0x%016llx)",
          (unsigned long long)cat_lo);
    CHECK(cat_hi == 0x0FEDCBA987654321ull,
          "{w[1], w[0]} high 64 bits = w[1] (got 0x%016llx)",
          (unsigned long long)cat_hi);

    // Runtime bit-select on a 64-bit Vec element: indices >= 4 are legal
    // (pre-fix the _ARCH_BCHK bound was the bogus inferred width 4, so
    // bit 60 aborted the sim).
    dut.bit_sel = 60; dut.eval();   // w[0] bit 60 of 0x123456789ABCDEF0 = 1
    CHECK(dut.rd_bit == 1, "w[0][60] reads bit 60 (got %u)", (unsigned)dut.rd_bit);
    dut.bit_sel = 63; dut.eval();   // bit 63 = 0
    CHECK(dut.rd_bit == 0, "w[0][63] reads bit 63 (got %u)", (unsigned)dut.rd_bit);

    // Overwrite v[1] only; concat must track it.
    write_lo(1, 0x01020304u);
    dut.eval();
    CHECK(dut.rd_pair == 0x01020304AAAA5555ull,
          "concat tracks v[1] rewrite (got 0x%016llx)",
          (unsigned long long)dut.rd_pair);

    printf("=== %d pass / %d fail ===\n", pass, fail);
    return fail == 0 ? 0 : 1;
}
