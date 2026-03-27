#include "Vtb.h"
#include "verilated.h"
int main(int argc, char** argv) {
    const std::unique_ptr<VerilatedContext> ctx{new VerilatedContext};
    ctx->commandArgs(argc, argv);
    const std::unique_ptr<Vtb> top{new Vtb{ctx.get()}};
    while (!ctx->gotFinish() && ctx->time() < 100000) {
        top->eval();
        ctx->timeInc(1);
    }
    top->final();
    return 0;
}
