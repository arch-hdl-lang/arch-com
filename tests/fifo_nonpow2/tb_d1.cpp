#include "VFifoCapD1.h"
#include "fifo_cap_tb.h"

int main(int argc, char** argv) {
    Verilated::commandArgs(argc, argv);
    VFifoCapD1* dut = new VFifoCapD1;
    int rc = run_fifo_cap_test(dut, 1);
    delete dut;
    return rc;
}
