// Latch-based ICG for AXI DMA channel clock gating.
// enable=1 → clock passes; enable=0 → clock gated (DMA channel halted).
// Emits: always_latch if (!clk_in) en_latched = enable;
//        assign clk_out = clk_in & en_latched;
module ClkGateDma (
  input logic clk_in,
  input logic enable,
  output logic clk_out
);

  logic en_latched;
  always_latch if (!clk_in) en_latched = enable;
  assign clk_out = clk_in & en_latched;

endmodule

