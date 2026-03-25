// Test: generate if with a compile-time true condition includes the port.
// domain SysDomain
//   freq_mhz: 100

module DebugModule (
  input logic clk,
  output logic [8-1:0] debug_out
);

  assign debug_out = 0;

endmodule

