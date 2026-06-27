#include "VNativePkgStructFsmProbe.h"

#include <cstdint>
#include <iostream>

static void tick(VNativePkgStructFsmProbe& dut) {
  dut.clk = 0;
  dut.eval();
  dut.clk = 1;
  dut.eval();
  dut.clk = 0;
  dut.eval();
}

int main() {
  VNativePkgStructFsmProbe dut;

  dut.rst = 1;
  dut.fast_in = 0;
  dut.timer_in = 0;
  tick(dut);

  dut.rst = 0;
  dut.fast_in = 0x1234u;
  dut.timer_in = 0;
  dut.eval();
  if (dut.fast_seen != 0x1234u) {
    std::cerr << "fast_seen reset path got 0x" << std::hex << dut.fast_seen
              << ", expected 0x1234\n";
    return 1;
  }
  if (dut.timer_seen != 0) {
    std::cerr << "timer_seen should be low before timer input\n";
    return 1;
  }

  dut.timer_in = 1;
  dut.fast_in = 0x3456u;
  dut.eval();
  if (dut.fast_seen != 0x3456u) {
    std::cerr << "fast_seen active path got 0x" << std::hex << dut.fast_seen
              << ", expected 0x3456\n";
    return 1;
  }
  if (dut.timer_seen != 1) {
    std::cerr << "timer_seen should reflect timer input in Idle\n";
    return 1;
  }

  tick(dut);
  if (dut.timer_seen != 1) {
    std::cerr << "timer_seen should remain high after entering Seen\n";
    return 1;
  }

  dut.timer_in = 0;
  tick(dut);
  if (dut.timer_seen != 0) {
    std::cerr << "timer_seen should drop after returning to Idle\n";
    return 1;
  }

  std::cout << "PASS native package struct FSM coverage\n";
  return 0;
}
