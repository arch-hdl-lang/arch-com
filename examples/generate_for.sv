// Test: generate_for loop expanding ports
// `generate_for i in 0..1` creates two copies of req_i and gnt_i.
// domain SysDomain
//   freq_mhz: 100

module GenDemo (
  input logic clk,
  input logic rst,
  input logic req_0,
  output logic gnt_0,
  input logic req_1,
  output logic gnt_1
);

  assign gnt_0 = req_0;
  assign gnt_1 = req_1;

endmodule

