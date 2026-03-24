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

  logic [32-1:0] hist;
  always_ff @(posedge clk or posedge areset) begin
    if (areset) begin
      hist <= 0;
    end else begin
      if (train_mispredicted) begin
        hist <= {train_history[30:0], train_taken};
      end else if (predict_valid) begin
        hist <= {hist[30:0], predict_taken};
      end
    end
  end
  assign predict_history = hist;

endmodule

