#include "VNativeVecInstOutputPackedPortProbe.h"

#include <cstdint>
#include <iostream>

int main() {
  VNativeVecInstOutputPackedPortProbe dut;

  dut.eval();
  if (dut.packed_out != 2u) {
    std::cerr << "packed_out got " << dut.packed_out << ", expected 2\n";
    return 1;
  }

  std::cout << "PASS native Vec inst output packed port\n";
  return 0;
}
