// Test: active-low synchronous reset
// `reg on clk rising, rst low` should emit `if (!rst)` in the body.
// domain SysDomain
//   freq_mhz: 100

module ActiveLowRst (
  input logic clk,
  input logic rst_n,
  input logic en,
  output logic [8-1:0] count
);

  logic [8-1:0] count_r = 0;
  assign count = count_r;
  always_ff @(posedge clk) begin
    if ((!rst_n)) begin
      count_r <= 0;
    end else begin
      if (en) begin
        count_r <= (count_r + 1);
      end
    end
  end

endmodule

