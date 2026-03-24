// VerilogEval Prob040: Decade counter (0-9), sync reset
// domain SysDomain

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
      if ((count_r == 9)) begin
        count_r <= 0;
      end else begin
        count_r <= 4'((count_r + 1));
      end
    end
  end
  assign q = count_r;

endmodule

