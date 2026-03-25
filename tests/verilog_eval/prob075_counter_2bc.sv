// VerilogEval Prob075: 2-bit saturating counter (branch predictor)
// domain SysDomain

module TopModule (
  input logic clk,
  input logic areset,
  input logic train_valid,
  input logic train_taken,
  output logic [2-1:0] state
);

  always_ff @(posedge clk or posedge areset) begin
    if (areset) begin
      state <= 1;
    end else begin
      if (train_valid) begin
        if (state < 3 & train_taken) begin
          state <= 2'(state + 1);
        end else if (state > 0 & ~train_taken) begin
          state <= 2'(state - 1);
        end
      end
    end
  end

endmodule

