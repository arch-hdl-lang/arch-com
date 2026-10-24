#include "VFifoCapD3L1.h"
#include "fifo_cap_tb.h"

int main(int argc, char** argv) {
    Verilated::commandArgs(argc, argv);
    VFifoCapD3L1* dut = new VFifoCapD3L1;
    int rc = run_fifo_cap_test(dut, 3);
    delete dut;
    return rc;
}
