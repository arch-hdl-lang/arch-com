// 8-way pseudo-LRU tree update and victim selection (combinational).
//
// Tree bit topology (7 nodes for 8 ways):
//   node 0: root         — ways {0-3} left, {4-7} right
//   node 1: left child   — ways {0-1} left, {2-3} right
//   node 2: right child  — ways {4-5} left, {6-7} right
//   node 3: leaf LL      — way 0 left, way 1 right
//   node 4: leaf LR      — way 2 left, way 3 right
//   node 5: leaf RL      — way 4 left, way 5 right
//   node 6: leaf RR      — way 6 left, way 7 right
//
// Bit convention: 0 = victim on right subtree, 1 = victim on left subtree.
// Victim selection: follow 0-pointers (go right). Update: set path bits to
// point AWAY from the accessed way (mark it MRU).
module ModuleLruUpdate (
  input logic [7-1:0] tree_in,
  input logic [3-1:0] access_way,
  input logic access_en,
  output logic [7-1:0] tree_out,
  output logic [3-1:0] victim_way
);

  // current LRU tree for the accessed set
  // way that was just hit/filled
  // enable tree update (false = read-only)
  // updated tree to write back
  // LRU (oldest) way to evict on miss
  // ── Victim selection ─────────────────────────────────────────────────────
  // Traverse the tree following bit=0 (right/high-way direction) to find LRU.
  logic [3-1:0] idx;
  always_comb begin
    idx = 0;
    for (int depth = 0; depth <= 2; depth++) begin
      if (tree_in[(1 << depth) - 1 + 7'($unsigned(idx))] == 0) begin
        idx = 3'(32'($unsigned(idx)) << 1 | 1);
      end else begin
        idx = 3'(32'($unsigned(idx)) << 1);
      end
    end
  end
  assign victim_way = idx;
  // ── Tree update ──────────────────────────────────────────────────────────
  // For each tree level, set the node bit to the corresponding bit of access_way
  // (bit=1 means "MRU is on right, victim on left" — pointing away from MRU).
  logic [7-1:0] updated;
  logic [32-1:0] step;
  logic way_bit;
  always_comb begin
    updated = tree_in;
    step = 0;
    way_bit = 1'b0;
    for (int depth = 0; depth <= 2; depth++) begin
      way_bit = access_way[2 - depth];
      updated[(1 << depth) - 1 + 7'(step)] = way_bit;
      step = step << 1 | 32'($unsigned(way_bit));
    end
  end
  assign tree_out = access_en ? updated : tree_in;

endmodule

