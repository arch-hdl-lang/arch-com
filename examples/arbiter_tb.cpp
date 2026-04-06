#include "VBusArbiter.h"
#include "verilated.h"
#include <cstdio>

int main(int argc, char** argv) {
    Verilated::commandArgs(argc, argv);
    VBusArbiter* dut = new VBusArbiter;

    int errors = 0;

    auto tick = [&]() {
        dut->clk = 0; dut->eval();
        dut->clk = 1; dut->eval();
    };

    // Reset
    dut->clk = 0;
    dut->rst = 1;
    dut->request_valid = 0;
    dut->eval();
    tick(); tick();
    dut->rst = 0;

    // ── Test 1: single requester 0 ────────────────────────────────────────────
    dut->request_valid = 0b0001;  // only req 0
    dut->eval();
    {
        if (!dut->grant_valid) {
            printf("FAIL: single req0: grant_valid should be 1\n");
            errors++;
        } else if (dut->grant_requester != 0) {
            printf("FAIL: single req0: expected grant_requester=0, got %d\n", (int)dut->grant_requester);
            errors++;
        } else {
            printf("PASS: single requester 0 granted\n");
        }
    }
    tick();

    // ── Test 2: single requester 2 ────────────────────────────────────────────
    dut->request_valid = 0b0100;  // only req 2
    dut->eval();
    {
        if (!dut->grant_valid) {
            printf("FAIL: single req2: grant_valid should be 1\n");
            errors++;
        } else if (dut->grant_requester != 2) {
            printf("FAIL: single req2: expected grant_requester=2, got %d\n", (int)dut->grant_requester);
            errors++;
        } else {
            printf("PASS: single requester 2 granted\n");
        }
    }
    tick();

    // ── Test 3: no request ───────────────────────────────────────────────────
    dut->request_valid = 0;
    dut->eval();
    {
        if (dut->grant_valid) {
            printf("FAIL: no request: grant_valid should be 0\n");
            errors++;
        } else {
            printf("PASS: no grant when no request\n");
        }
    }

    dut->final();
    delete dut;

    if (errors == 0) { printf("\nALL TESTS PASSED\n"); return 0; }
    else             { printf("\n%d TESTS FAILED\n", errors); return 1; }
}
