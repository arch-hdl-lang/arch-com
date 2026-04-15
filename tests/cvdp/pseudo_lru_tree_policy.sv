module slot_select_pseudo_lru_tree #(
  parameter int NWAYS = 4,
  parameter int MAX_DEPTH = $clog2(NWAYS)
) (
  input logic [NWAYS - 1-1:0] array,
  output logic [$clog2(NWAYS)-1:0] index
);

  logic [$clog2(NWAYS)-1:0] idx;
  always_comb begin
    idx = 0;
    for (int depth = 0; depth <= MAX_DEPTH - 1; depth++) begin
      if (array[((1 << depth) - 1) + (NWAYS - 1)'($unsigned(idx))] == 0) begin
        idx = $clog2(NWAYS)'(32'($unsigned(idx)) << 1 | 1);
      end else begin
        idx = $clog2(NWAYS)'(32'($unsigned(idx)) << 1);
      end
    end
    // bit is 0 -> go right (LRU direction)
    // bit is 1 -> go left
  end
  assign index = idx;

endmodule

module pseudo_lru_tree_policy #(
  parameter int NWAYS = 4,
  parameter int NINDEXES = 32,
  parameter int NBITS_TREE = NWAYS - 1,
  parameter int MAX_DEPTH = $clog2(NWAYS)
) (
  input logic clock,
  input logic reset,
  input logic [$clog2(NINDEXES)-1:0] index,
  input logic [$clog2(NWAYS)-1:0] way_select,
  input logic access,
  input logic hit,
  output logic [$clog2(NWAYS)-1:0] way_replace
);

  logic [NINDEXES-1:0] [NBITS_TREE-1:0] recency;
  logic [NBITS_TREE-1:0] recency_updated;
  logic [$clog2(NWAYS)-1:0] pseudo_lru_slot;
  logic [$clog2(NINDEXES)-1:0] idx;
  assign idx = index;
  // Submodule: find pseudo-LRU slot from current recency tree
  slot_select_pseudo_lru_tree #(.NWAYS(NWAYS)) slot_select_unit (
    .array(recency[idx]),
    .index(pseudo_lru_slot)
  );
  assign way_replace = pseudo_lru_slot;
  // Compute recency_updated: mark the target way as MRU
  // On hit: target is way_select. On miss: target is pseudo_lru_slot.
  logic [$clog2(NWAYS)-1:0] target_way;
  logic [31:0] step;
  logic way_bit;
  always_comb begin
    if (hit) begin
      target_way = way_select;
    end else begin
      target_way = pseudo_lru_slot;
    end
    // Start with current recency and update the path for target_way
    recency_updated = recency[idx];
    step = 0;
    way_bit = 1'b0;
    for (int depth = 0; depth <= MAX_DEPTH - 1; depth++) begin
      way_bit = target_way[(MAX_DEPTH - 1) - depth];
      recency_updated[((1 << depth) - 1) + NBITS_TREE'(step)] = way_bit;
      step = step << 1 | 32'($unsigned(way_bit));
    end
    // Set the bit along the path to mark target_way as MRU
    // Advance step: follow the way bit
  end
  // Update recency array on access
  always_ff @(posedge clock or posedge reset) begin
    if (reset) begin
      for (int __ri0 = 0; __ri0 < NINDEXES; __ri0++) begin
        recency[__ri0] <= 0;
      end
    end else begin
      if (access) begin
        recency[idx] <= recency_updated;
      end
    end
  end

endmodule

