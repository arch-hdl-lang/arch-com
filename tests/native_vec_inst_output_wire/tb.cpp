#include "VNativeVecInstOutputWireProbe.h"

#include <cstdint>
#include <iostream>

int main() {
  VNativeVecInstOutputWireProbe dut;

  dut.select_in = 0;
  dut.eval();
  if (dut.selected_out != 37u) {
    std::cerr << "select 0 got " << dut.selected_out << ", expected 37\n";
    return 1;
  }

  dut.select_in = 1;
  dut.eval();
  if (dut.selected_out != 99u) {
    std::cerr << "select 1 got " << dut.selected_out << ", expected 99\n";
    return 1;
  }

  std::cout << "PASS native Vec inst output wire\n";
  return 0;
}
