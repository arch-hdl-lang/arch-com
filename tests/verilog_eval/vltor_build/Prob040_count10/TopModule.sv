// VerilogEval Prob040: Decade counter (0-9), sync reset
module TopModule (
  input logic clk,
  input logic reset,
  output logic [4-1:0] q
);

  logic [4-1:0] count_r = 0;
  always_ff @(posedge clk) begin
    if (reset) begin
      count_r <= 0;
    end else begin
      count_r <= count_r == 9 ? 0 : 4'(count_r + 1);
    end
  end
  assign q = count_r;

endmodule

