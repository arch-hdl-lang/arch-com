// VerilogEval Prob153: Gshare branch predictor
// domain SysDomain

module TopModule (
  input logic clk,
  input logic areset,
  input logic predict_valid,
  input logic [7-1:0] predict_pc,
  input logic train_valid,
  input logic train_taken,
  input logic train_mispredicted,
  input logic [7-1:0] train_history,
  input logic [7-1:0] train_pc,
  output logic predict_taken,
  output logic [7-1:0] predict_history
);

  // 128-entry PHT, 2-bit saturating counters, init to weakly not-taken (01)
  logic [2-1:0] pht [0:128-1];
  logic [7-1:0] ghr;
  logic [7-1:0] predict_idx;
  logic [7-1:0] train_idx;
  logic [2-1:0] train_cnt;
  logic [2-1:0] train_new;
  always_comb begin
    predict_idx = predict_pc ^ ghr;
    train_idx = train_pc ^ train_history;
    if (predict_valid) begin
      predict_taken = pht[predict_idx][1];
      predict_history = ghr;
    end else begin
      predict_taken = 0;
      predict_history = 0;
    end
    train_cnt = pht[train_idx];
    train_new = train_cnt;
    if (train_taken & train_cnt != 3) begin
      train_new = 2'(train_cnt + 1);
    end else if (~train_taken & train_cnt != 0) begin
      train_new = 2'(train_cnt - 1);
    end
  end
  // Gate outputs: 0 when predict_valid=0 (Verilator 2-state compatibility)
  // Compute updated counter for training
  always_ff @(posedge clk or posedge areset) begin
    if (areset) begin
      ghr <= 0;
      pht <= '{default: 1};
    end else begin
      if (train_valid) begin
        pht[train_idx] <= train_new;
      end
      if (predict_valid) begin
        ghr <= {ghr[5:0], pht[predict_idx][1]};
      end
      if (train_valid & train_mispredicted) begin
        ghr <= {train_history[5:0], train_taken};
      end
    end
  end

endmodule

// Train PHT
// Update GHR: predict shifts in predicted taken
// Misprediction recovery overwrites GHR (last assignment wins)
