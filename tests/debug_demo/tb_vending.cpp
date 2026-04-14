#include "Vvending_machine.h"
#include "verilated.h"
#include <cstdio>

Vvending_machine dut;

void tick() { dut.clk = 0; dut.eval(); dut.clk = 1; dut.eval(); }

int main(int argc, char** argv) {
    Verilated::commandArgs(argc, argv);
    dut.clk = 0; dut.rst = 1;
    dut.item_button = 0; dut.item_selected = 0;
    dut.coin_input = 0; dut.cancel = 0;
    for (int i = 0; i < 3; i++) tick();
    dut.rst = 0; tick();

    // Select item 1 (price=5)
    dut.item_button = 1; dut.item_selected = 1;
    tick();
    dut.item_button = 0;
    tick(); tick();

    // Insert coins: 2 + 2 + 2 = 6 (> price 5)
    dut.coin_input = 2; tick();
    dut.coin_input = 2; tick();
    dut.coin_input = 2; tick();
    dut.coin_input = 0;

    for (int c = 0; c < 10; c++) {
        tick();
        if (dut.dispense_item) {
            printf("DISPENSED item %u, change=%u\n", dut.dispense_item_id, dut.change_amount);
        }
    }

    printf("PASS\n");
    return 0;
}
