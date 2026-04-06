// Test for +: and -: part-select syntax
module PartSelectTest (
  input logic clk,
  input logic rst,
  input logic [32-1:0] data_in,
  input logic [5-1:0] offset,
  output logic [8-1:0] byte_out,
  output logic [8-1:0] byte_msb_out,
  output logic [8-1:0] hi_byte
);

  // +: extract: data_in[offset +: 8]
  // -: extract: data_in[offset -: 8]  (offset = MSB)
  // static +: extract: data_in[16 +: 8]
  assign byte_out = data_in[offset +: 8];
  assign byte_msb_out = data_in[offset -: 8];
  assign hi_byte = data_in[16 +: 8];

endmodule

