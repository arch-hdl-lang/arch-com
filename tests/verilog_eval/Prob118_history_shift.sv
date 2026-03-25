// domain SysDomain

module TopModule (
  input logic clk,
  input logic areset,
  input logic predict_valid,
  input logic predict_taken,
  input logic train_mispredicted,
  input logic train_taken,
  input logic [32-1:0] train_history,
  output logic [32-1:0] predict_history
);

  always_ff @(posedge clk or posedge areset) begin
    if (areset) begin
      predict_history <= 0;
    end else begin
      if (train_mispredicted) begin
        predict_history <= {train_history[30:0], train_taken};
      end else if (predict_valid) begin
        predict_history <= {predict_history[30:0], predict_taken};
      end
    end
  end

endmodule

