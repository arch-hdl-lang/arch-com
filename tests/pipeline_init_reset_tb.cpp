#include "VPipelineInitReset.h"

#include <cstdio>

static bool check_value(const char* name, unsigned actual, unsigned expected) {
    if (actual == expected) {
        return true;
    }
    std::printf("FAIL: %s = %u, expected %u\n", name, actual, expected);
    return false;
}

int main() {
    VPipelineInitReset dut;
    dut.clk = 0;
    dut.rst_sync = 0;
    dut.rst_async_n = 1;
    dut.data_in = 42;
    dut.eval();

    if (!check_value("reset_only initial", dut.reset_only_out, 0) ||
        !check_value("no_reset initial", dut.no_reset_out, 0) ||
        !check_value("init_only initial", dut.init_only_out, 5) ||
        !check_value("init_and_reset initial", dut.init_and_reset_out, 7) ||
        !check_value("expression initial", dut.expr_init_and_reset_out, 11)) {
        return 1;
    }

    // An asynchronous reset must take effect without a rising clock edge.
    dut.rst_async_n = 0;
    dut.eval();
    if (!check_value("async reset", dut.init_and_reset_out, 9)) {
        return 1;
    }

    // Load ordinary data on a rising edge after releasing async reset.
    dut.rst_async_n = 1;
    dut.clk = 1;
    dut.eval();
    if (!check_value("loaded reset_only", dut.reset_only_out, 42) ||
        !check_value("loaded no_reset", dut.no_reset_out, 42) ||
        !check_value("loaded init_only", dut.init_only_out, 42) ||
        !check_value("loaded init_and_reset", dut.init_and_reset_out, 42) ||
        !check_value("loaded expression", dut.expr_init_and_reset_out, 42)) {
        return 1;
    }

    // A synchronous reset waits for the next rising edge. Reset-free regs
    // continue to update, while expression reset values keep their meaning.
    dut.clk = 0;
    dut.eval();
    dut.rst_sync = 1;
    dut.data_in = 21;
    dut.eval();
    if (!check_value("sync reset before edge", dut.reset_only_out, 42) ||
        !check_value("expression reset before edge", dut.expr_init_and_reset_out, 42)) {
        return 1;
    }
    dut.clk = 1;
    dut.eval();
    if (!check_value("sync reset_only", dut.reset_only_out, 3) ||
        !check_value("sync expression reset", dut.expr_init_and_reset_out, 12) ||
        !check_value("no_reset during sync reset", dut.no_reset_out, 21) ||
        !check_value("init_only during sync reset", dut.init_only_out, 21) ||
        !check_value("async reg during sync reset", dut.init_and_reset_out, 21)) {
        return 1;
    }

    std::printf("PASS pipeline init/reset behavior\n");
    return 0;
}
