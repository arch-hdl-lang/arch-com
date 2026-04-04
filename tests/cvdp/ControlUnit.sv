module ControlUnit (
  input logic clk,
  input logic rst,
  input logic hit,
  input logic miss,
  input logic ready,
  output logic tlb_write_enable,
  output logic flsh
);

  assign tlb_write_enable = miss;
  assign flsh = 1'b0;

endmodule

