// VerilogEval Prob075: 2-bit saturating counter (branch predictor)
// domain SysDomain

module TopModule (
  input logic clk,
  input logic areset,
  input logic train_valid,
  input logic train_taken,
  output logic [2-1:0] state_sig
);

  logic [2-1:0] cnt = 1;
  always_ff @(posedge clk or posedge areset) begin
    if (areset) begin
      cnt <= 1;
    end else begin
      if (train_valid) begin
        if (((cnt < 3) & train_taken)) begin
          cnt <= 2'((cnt + 1));
        end else if (((cnt > 0) & (~train_taken))) begin
          cnt <= 2'((cnt - 1));
        end
      end
    end
  end
  assign state_sig = cnt;

endmodule

