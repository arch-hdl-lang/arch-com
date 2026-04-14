#include "Vtb_counter_check.h"
#include "verilated.h"
int main(int argc, char** argv) {
    Verilated::commandArgs(argc, argv);
    Vtb_counter_check* tb = new Vtb_counter_check;
    while (!Verilated::gotFinish()) {
        tb->eval();
        Verilated::timeInc(1);
    }
    tb->final();
    delete tb;
    return 0;
}
