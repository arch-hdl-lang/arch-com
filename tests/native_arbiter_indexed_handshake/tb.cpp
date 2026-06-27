#include "VNativeArbiterIndexedHandshakeProbe.h"

#include <cstdint>
#include <iostream>

int main() {
  VNativeArbiterIndexedHandshakeProbe dut;

  dut.clk_i = 0;
  dut.rst_i = 0;
  dut.req0_i = 1;
  dut.req1_i = 0;
  dut.req2_i = 1;
  dut.req3_i = 0;
  dut.eval();

  if (!dut.grant_valid_o) {
    std::cerr << "grant_valid_o was low\n";
    return 1;
  }
  if (dut.grant_idx_o != 0u) {
    std::cerr << "grant_idx_o got " << dut.grant_idx_o << ", expected 0\n";
    return 1;
  }
  if (!dut.ready0_o || dut.ready1_o || dut.ready2_o || dut.ready3_o) {
    std::cerr << "ready bits got "
              << static_cast<unsigned>(dut.ready0_o)
              << static_cast<unsigned>(dut.ready1_o)
              << static_cast<unsigned>(dut.ready2_o)
              << static_cast<unsigned>(dut.ready3_o)
              << ", expected 1000\n";
    return 1;
  }

  std::cout << "PASS native arbiter indexed handshake\n";
  return 0;
}
