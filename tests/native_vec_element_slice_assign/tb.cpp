#include "VNativeVecElementSliceAssignProbe.h"

#include <cstdint>
#include <iostream>

int main() {
  VNativeVecElementSliceAssignProbe dut;

  dut.clk_i = 0;
  dut.rst_i = 1;
  dut.lo_i = 0;
  dut.hi_i = 0;
  dut.eval();
  dut.clk_i = 1;
  dut.eval();

  dut.clk_i = 0;
  dut.rst_i = 0;
  dut.lo_i = 0x89ABCDEFu;
  dut.hi_i = 0x12345678u;
  dut.eval();
  dut.clk_i = 1;
  dut.eval();

  const uint64_t expected = 0x1234567889ABCDEFULL;
  if (dut.word_o != expected) {
    std::cerr << std::hex << "word_o got 0x" << dut.word_o
              << ", expected 0x" << expected << "\n";
    return 1;
  }

  std::cout << "PASS native Vec element slice assign\n";
  return 0;
}
