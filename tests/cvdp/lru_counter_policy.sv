module lru_counter_policy #(
  parameter int NWAYS = 4,
  parameter int NINDEXES = 32,
  parameter int CNT_W = $clog2(NWAYS),
  parameter int TOTAL_W = NWAYS * $clog2(NWAYS)
) (
  input logic clock,
  input logic reset,
  input logic [$clog2(NINDEXES)-1:0] index,
  input logic [$clog2(NWAYS)-1:0] way_select,
  input logic access,
  input logic hit,
  output logic [$clog2(NWAYS)-1:0] way_replace
);

  // Packed recency counters: each index has NWAYS counters, each CNT_W bits
  logic [TOTAL_W-1:0] recency [0:NINDEXES-1];
  // Combinational signals
  logic [$clog2(NWAYS)-1:0] lru_slot;
  logic [TOTAL_W-1:0] cur_rec;
  logic [CNT_W-1:0] accessed_cnt;
  logic [CNT_W-1:0] replace_cnt;
  logic [$clog2(NINDEXES)-1:0] idx;
  assign idx = index;
  logic [$clog2(NWAYS)-1:0] ws;
  assign ws = way_select;
  // Combinational: read current recency and find LRU slot (counter == 0)
  always_comb begin
    cur_rec = recency[idx];
    // Extract the accessed way's counter
    accessed_cnt = CNT_W'(cur_rec[ws * CNT_W +: CNT_W]);
    // Find LRU: way with counter == 0 (scan high to low, last-write-wins = lowest index)
    lru_slot = 0;
    for (int i = 0; i <= NWAYS - 1; i++) begin
      if (CNT_W'(cur_rec[(NWAYS - 1 - i) * CNT_W +: CNT_W]) == 0) begin
        lru_slot = $clog2(NWAYS)'(NWAYS - 1 - i);
      end
    end
    // Extract replaced way's counter (for miss path)
    replace_cnt = CNT_W'(cur_rec[lru_slot * CNT_W +: CNT_W]);
  end
  assign way_replace = lru_slot;
  // Sequential update with reset initialization
  always_ff @(posedge clock or posedge reset) begin
    if (reset) begin
      for (int __ri0 = 0; __ri0 < NINDEXES; __ri0++) begin
        recency[__ri0] <= 0;
      end
    end else begin
      if (reset) begin
        // Initialize: way i gets counter value i
        for (int n = 0; n <= NINDEXES - 1; n++) begin
          for (int w = 0; w <= NWAYS - 1; w++) begin
            recency[n][w * CNT_W +: CNT_W] <= CNT_W'(w);
          end
        end
      end else if (access) begin
        if (hit) begin
          // Hit: set accessed way to NWAYS-1, decrement those > accessed_cnt
          for (int i = 0; i <= NWAYS - 1; i++) begin
            if ($clog2(NWAYS)'(i) == ws) begin
              recency[idx][i * CNT_W +: CNT_W] <= CNT_W'(NWAYS - 1);
            end else if (CNT_W'(cur_rec[i * CNT_W +: CNT_W]) > accessed_cnt) begin
              recency[idx][i * CNT_W +: CNT_W] <= CNT_W'(cur_rec[i * CNT_W +: CNT_W] - 1);
            end
          end
        end else begin
          // Miss: replace lru_slot (counter==0), set to NWAYS-1, decrement those > replace_cnt
          for (int i = 0; i <= NWAYS - 1; i++) begin
            if ($clog2(NWAYS)'(i) == lru_slot) begin
              recency[idx][i * CNT_W +: CNT_W] <= CNT_W'(NWAYS - 1);
            end else if (CNT_W'(cur_rec[i * CNT_W +: CNT_W]) > replace_cnt) begin
              recency[idx][i * CNT_W +: CNT_W] <= CNT_W'(cur_rec[i * CNT_W +: CNT_W] - 1);
            end
          end
        end
      end
    end
  end

endmodule

