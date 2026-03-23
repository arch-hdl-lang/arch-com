// Basic latch-based clock gate (ASIC standard ICG)
module IcgCell (
  input logic clk_in,
  input logic enable,
  input logic test_en,
  output logic clk_out
);

  logic en_latched;
  always_latch if (!clk_in) en_latched <= enable | test_en;
  assign clk_out = clk_in & en_latched;

endmodule

