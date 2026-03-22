// Verilator testbench for E203 IfuIfetch — cross-check
#include "VIfuIfetch.h"
#include "verilated.h"
#include <cstdio>

static int errors = 0;
static int test_num = 0;

#define CHECK(cond, ...) do { \
    test_num++; \
    if (!(cond)) { errors++; printf("FAIL test %d: ", test_num); printf(__VA_ARGS__); printf("\n"); } \
    else { printf("PASS test %d\n", test_num); } \
} while(0)

static void tick(VIfuIfetch* m) {
    m->clk = 0; m->eval();
    m->clk = 1; m->eval();
}

int main(int argc, char** argv) {
    Verilated::commandArgs(argc, argv);
    auto m = new VIfuIfetch;
    m->clk = 0; m->rst = 0;  // async low = asserted
    m->req_ready = 0; m->rsp_valid = 0; m->rsp_instr = 0; m->rsp_err = 0;
    m->redirect = 0; m->redirect_pc = 0; m->o_ready = 0;
    m->eval();

    // Reset 2 cycles
    tick(m); tick(m);

    // Release reset
    m->rst = 1;

    // Cycle 1: Idle → WaitGnt
    tick(m);
    CHECK(m->req_valid == 1, "c1: req_valid");
    CHECK(m->req_addr == 0x80000000u, "c1: req_addr=0x%08X", m->req_addr);

    // Cycle 2: grant → WaitRsp
    m->req_ready = 1; tick(m); m->req_ready = 0;
    CHECK(m->rsp_ready == 1, "c2: rsp_ready");

    // Cycle 3: response → WaitGnt with PC+4
    m->rsp_valid = 1; m->rsp_instr = 0xDEADBEEF; m->rsp_err = 0;
    tick(m); m->rsp_valid = 0;
    CHECK(m->req_valid == 1, "c3: req_valid");
    CHECK(m->req_addr == 0x80000004u, "c3: req_addr=0x%08X", m->req_addr);

    // Cycle 4: redirect during WaitGnt
    m->req_ready = 0;
    m->redirect = 1; m->redirect_pc = 0x00001000;
    tick(m); m->redirect = 0;
    // In Abort
    CHECK(m->rsp_ready == 1, "c4: Abort rsp_ready");

    // Cycle 5: Abort → WaitGnt with redirected PC
    tick(m);
    CHECK(m->req_valid == 1, "c5: req_valid");
    CHECK(m->req_addr == 0x00001000u, "c5: req_addr=0x%08X", m->req_addr);

    // Cycle 6-7: normal fetch
    m->req_ready = 1; tick(m); m->req_ready = 0;
    m->rsp_valid = 1; m->rsp_instr = 0x13; m->rsp_err = 0;
    tick(m); m->rsp_valid = 0;
    CHECK(m->req_addr == 0x00001004u, "c7: req_addr=0x%08X", m->req_addr);

    // Cycle 8: redirect during WaitRsp
    m->req_ready = 1; tick(m); m->req_ready = 0;
    m->redirect = 1; m->redirect_pc = 0xFFFF0000;
    tick(m); m->redirect = 0;
    tick(m);
    CHECK(m->req_addr == 0xFFFF0000u, "c10: req_addr=0x%08X", m->req_addr);

    printf("\n=== IfuIfetch Verilator: %d tests, %d errors ===\n", test_num, errors);
    delete m;
    return errors ? 1 : 0;
}
