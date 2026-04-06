// Test: rst.asserted — polarity-abstracted reset expression.
// Active-high: rst.asserted emits `rst`
// Active-low:  rst_n.asserted emits `(!rst_n)`
// Allows polarity-independent seq bodies (e.g. reset none + manual if).
// domain SysDomain
//   freq_mhz: 100

module HighRstAsserted (
  input logic clk,
  input logic rst,
  input logic en,
  output logic [8-1:0] count
);

  logic [8-1:0] count_r;
  always_ff @(posedge clk) begin
    if (rst) begin
      count_r <= 0;
    end else if (en) begin
      count_r <= 8'(count_r + 1);
    end
  end
  assign count = count_r;

endmodule

module LowRstAsserted (
  input logic clk,
  input logic rst_n,
  input logic en,
  output logic [8-1:0] count
);

  logic [8-1:0] count_r;
  always_ff @(posedge clk) begin
    if ((!rst_n)) begin
      count_r <= 0;
    end else if (en) begin
      count_r <= 8'(count_r + 1);
    end
  end
  assign count = count_r;

endmodule

