#include "VNativeBoolNotProbe.h"
#include <cstdio>

static int pass = 0;
static int fail = 0;

#define CHECK(cond, msg, ...) do { \
    if (cond) { std::printf("  PASS: " msg "\n", ##__VA_ARGS__); ++pass; } \
    else { std::printf("  FAIL: " msg "\n", ##__VA_ARGS__); ++fail; } \
} while (0)

static void tick(VNativeBoolNotProbe& dut) {
    dut.clk = 0;
    dut.eval();
    dut.clk = 1;
    dut.eval();
    dut.clk = 0;
    dut.eval();
}

int main() {
    VNativeBoolNotProbe dut;

    dut.rst = 1;
    dut.busy_in = 0;
    dut.result_valid_in = 0;
    tick(dut);

    dut.rst = 0;
    dut.busy_in = 0;
    dut.result_valid_in = 0;
    tick(dut);
    CHECK(dut.result_valid_out == 0, "pipe_reg result_valid_out reset/sample is false");
    CHECK(dut.not_result_valid == 1, "not false is true on a Bool pipe_reg @0 read");
    CHECK(dut.idle_ampamp == 1, "symbolic && preserves not false && not false");
    CHECK(dut.idle_keyword == 1, "keyword and matches symbolic &&");
    CHECK(dut.busy_pipebar == 0, "symbolic || preserves false || false");
    CHECK(dut.busy_or_keyword == 0, "keyword or matches symbolic ||");

    dut.busy_in = 0;
    dut.result_valid_in = 1;
    tick(dut);
    CHECK(dut.result_valid_out == 1, "pipe_reg result_valid_out sampled true");
    CHECK(dut.not_result_valid == 0, "not true is false on a Bool pipe_reg @0 read");
    CHECK(dut.idle_ampamp == 0, "symbolic && observes not true as false");
    CHECK(dut.idle_keyword == 0, "keyword and matches symbolic && when false");
    CHECK(dut.busy_pipebar == 1, "symbolic || observes false || true as true");
    CHECK(dut.busy_or_keyword == 1, "keyword or matches symbolic || when true");

    std::printf("PASS native Bool not pipe_reg: %d pass / %d fail\n", pass, fail);
    return fail == 0 ? 0 : 1;
}
