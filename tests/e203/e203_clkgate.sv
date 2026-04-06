// E203 Clock Gate Cell — wraps ARCH clkgate construct with RealBench port names.
module E203Icg (
  input logic clk_in,
  input logic enable,
  input logic test_en,
  output logic clk_out
);

  logic en_latched;
  always_latch if (!clk_in) en_latched = enable | test_en;
  assign clk_out = clk_in & en_latched;

endmodule

module e203_clkgate (
  input logic clk_in,
  input logic test_mode,
  input logic clock_en,
  output logic clk_out
);

  E203Icg icg (
    .clk_in(clk_in),
    .enable(clock_en),
    .test_en(test_mode),
    .clk_out(clk_out)
  );

endmodule

