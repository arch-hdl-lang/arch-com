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
      if (array[(1 << depth) - 1 + (NWAYS - 1)'($unsigned(idx))] == 0) begin
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

