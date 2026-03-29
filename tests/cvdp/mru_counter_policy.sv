module mru_counter_policy #(
  parameter int NWAYS = 4,
  parameter int NINDEXES = 32,
  parameter int WAY_W = $clog2(NWAYS),
  parameter int REC_W = NWAYS * WAY_W
) (
  input logic clock,
  input logic reset,
  input logic [$clog2(NINDEXES)-1:0] index,
  input logic [WAY_W-1:0] way_select,
  input logic access,
  input logic hit,
  output logic [WAY_W-1:0] way_replace
);

  logic [REC_W-1:0] recency [0:NINDEXES-1];
  logic [WAY_W-1:0] mru_slot;
  logic [REC_W-1:0] cur_rec;
  logic [WAY_W-1:0] old_val;
  logic [REC_W-1:0] next_rec;
  logic [WAY_W-1:0] max_val;
  assign max_val = WAY_W'(NWAYS - 1);
  logic [$clog2(NINDEXES)-1:0] idx;
  assign idx = index;
  logic [WAY_W-1:0] ws;
  assign ws = way_select;
  always_comb begin
    cur_rec = recency[idx];
    // Find the way with counter == NWAYS-1 (the MRU way for replacement)
    mru_slot = 0;
    for (int i = 0; i <= NWAYS - 1; i++) begin
      if (cur_rec[i * WAY_W +: WAY_W] == max_val) begin
        mru_slot = WAY_W'(i);
      end
    end
    // Get old counter of the way being updated
    if (hit) begin
      old_val = WAY_W'(cur_rec[ws * WAY_W +: WAY_W]);
    end else begin
      old_val = WAY_W'(cur_rec[mru_slot * WAY_W +: WAY_W]);
    end
    // Build next recency
    next_rec = cur_rec;
    for (int j = 0; j <= NWAYS - 1; j++) begin
      if (hit) begin
        if (WAY_W'(j) == ws) begin
          next_rec[j * WAY_W +: WAY_W] = max_val;
        end else if (cur_rec[j * WAY_W +: WAY_W] > old_val) begin
          next_rec[j * WAY_W +: WAY_W] = WAY_W'(cur_rec[j * WAY_W +: WAY_W] - 1);
        end
      end else if (WAY_W'(j) == mru_slot) begin
        next_rec[j * WAY_W +: WAY_W] = max_val;
      end else if (cur_rec[j * WAY_W +: WAY_W] > old_val) begin
        next_rec[j * WAY_W +: WAY_W] = WAY_W'(cur_rec[j * WAY_W +: WAY_W] - 1);
      end
    end
  end
  assign way_replace = mru_slot;
  always_ff @(posedge clock) begin
    if (reset) begin
      for (int i = 0; i <= NINDEXES - 1; i++) begin
        for (int n = 0; n <= NWAYS - 1; n++) begin
          recency[i][n * WAY_W +: WAY_W] <= WAY_W'(n);
        end
      end
    end else if (access) begin
      recency[idx] <= next_rec;
    end
  end

endmodule

