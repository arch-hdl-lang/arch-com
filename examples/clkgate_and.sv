// AND-based clock gate (FPGA)
module IcgCellAnd (
  input logic clk_in,
  input logic enable,
  output logic clk_out
);

  assign clk_out = clk_in & (enable);

endmodule

