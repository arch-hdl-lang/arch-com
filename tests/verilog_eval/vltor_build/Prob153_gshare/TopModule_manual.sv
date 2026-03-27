module TopModule (
  input logic clk,
  input logic areset,
  input logic predict_valid,
  input logic [6:0] predict_pc,
  input logic train_valid,
  input logic train_taken,
  input logic train_mispredicted,
  input logic [6:0] train_history,
  input logic [6:0] train_pc,
  output logic predict_taken,
  output logic [6:0] predict_history
);

  logic [1:0] pht [0:127];
  logic [6:0] ghr;

  wire [6:0] predict_index = ghr ^ predict_pc;
  wire [6:0] train_index = train_history ^ train_pc;

  // Gate outputs: X when predict_valid=0
  assign predict_taken = predict_valid ? pht[predict_index][1] : 1'bx;
  assign predict_history = predict_valid ? ghr : 7'bxxxxxxx;

  always_ff @(posedge clk or posedge areset) begin
    if (areset) begin
      for (int i = 0; i < 128; i++)
        pht[i] <= 2'b01;
      ghr <= 7'b0;
    end else begin
      if (predict_valid)
        ghr <= {ghr[5:0], pht[predict_index][1]};
      if (train_valid) begin
        if (pht[train_index] < 2'b11 && train_taken)
          pht[train_index] <= pht[train_index] + 1;
        else if (pht[train_index] > 2'b00 && !train_taken)
          pht[train_index] <= pht[train_index] - 1;
        if (train_mispredicted)
          ghr <= {train_history[5:0], train_taken};
      end
    end
  end

endmodule
