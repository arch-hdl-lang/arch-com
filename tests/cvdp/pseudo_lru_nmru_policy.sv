module pseudo_lru_nmru_policy #(
  parameter int NWAYS = 4,
  parameter int NINDEXES = 32
) (
  input logic clock,
  input logic reset,
  input logic [$clog2(NINDEXES)-1:0] index,
  input logic [$clog2(NWAYS)-1:0] way_select,
  input logic access,
  input logic hit,
  output logic [$clog2(NWAYS)-1:0] way_replace
);

  logic [NWAYS-1:0] recency [NINDEXES-1:0];
  logic [$clog2(NWAYS)-1:0] replace_way;
  logic only_one_zero;
  logic [NWAYS-1:0] cur_recency;
  logic [$clog2(NWAYS) + 1-1:0] zero_count;
  logic [$clog2(NINDEXES)-1:0] idx;
  assign idx = index;
  always_comb begin
    cur_recency = recency[idx];
    // Count zeros
    zero_count = 0;
    for (int i = 0; i <= NWAYS - 1; i++) begin
      if (~cur_recency[i]) begin
        zero_count = ($clog2(NWAYS) + 1)'(zero_count + 1);
      end
    end
    only_one_zero = zero_count == 1;
    // Find lowest-index zero bit (iterate from high to low, last write wins = lowest)
    replace_way = 0;
    for (int i = 0; i <= NWAYS - 1; i++) begin
      if (~cur_recency[NWAYS - 1 - i]) begin
        replace_way = $clog2(NWAYS)'(NWAYS - 1 - i);
      end
    end
  end
  assign way_replace = replace_way;
  always_ff @(posedge clock or posedge reset) begin
    if (reset) begin
      for (int __ri0 = 0; __ri0 < NINDEXES; __ri0++) begin
        recency[__ri0] <= 0;
      end
    end else begin
      if (access & hit) begin
        // On a hit: set the way_select recency bit
        if (only_one_zero) begin
          // LRU case: set way_select bit, clear all others
          recency[idx] <= NWAYS'($unsigned(1)) << way_select;
        end else begin
          recency[idx] <= recency[idx] | NWAYS'($unsigned(1)) << way_select;
        end
      end else if (access & ~hit) begin
        // On a miss: set the replace_way recency bit (replacement happened)
        if (only_one_zero) begin
          recency[idx] <= NWAYS'($unsigned(1)) << replace_way;
        end else begin
          recency[idx] <= recency[idx] | NWAYS'($unsigned(1)) << replace_way;
        end
      end
    end
  end

endmodule

