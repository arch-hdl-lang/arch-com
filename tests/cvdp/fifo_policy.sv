module fifo_policy #(
  parameter int NWAYS = 4,
  parameter int NINDEXES = 32,
  parameter int WAY_W = $clog2(NWAYS)
) (
  input logic clock,
  input logic reset,
  input logic [$clog2(NINDEXES)-1:0] index,
  input logic [WAY_W-1:0] way_select,
  input logic access,
  input logic hit,
  output logic [WAY_W-1:0] way_replace
);

  logic [WAY_W-1:0] fifo_array [NINDEXES-1:0];
  logic [WAY_W-1:0] cur_val;
  logic [WAY_W-1:0] next_val;
  logic [$clog2(NINDEXES)-1:0] idx;
  assign idx = index;
  always_comb begin
    cur_val = fifo_array[idx];
    way_replace = cur_val;
    if (cur_val == WAY_W'(NWAYS - 1)) begin
      next_val = 0;
    end else begin
      next_val = WAY_W'(cur_val + 1);
    end
  end
  always_ff @(posedge clock or posedge reset) begin
    if (reset) begin
      for (int __ri0 = 0; __ri0 < NINDEXES; __ri0++) begin
        fifo_array[__ri0] <= 0;
      end
    end else begin
      if (access & ~hit) begin
        fifo_array[idx] <= next_val;
      end
    end
  end

endmodule

