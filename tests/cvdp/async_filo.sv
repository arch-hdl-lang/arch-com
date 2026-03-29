module async_filo #(
  parameter int DATA_WIDTH = 16,
  parameter [3:0] DEPTH = 8,
  parameter int ADDR_WIDTH = 3
) (
  input logic w_clk,
  input logic w_rst,
  input logic push,
  input logic r_rst,
  input logic r_clk,
  input logic pop,
  input logic [DATA_WIDTH-1:0] w_data,
  output logic [DATA_WIDTH-1:0] r_data,
  output logic r_empty,
  output logic w_full
);

  // max 15; width matches ADDR_WIDTH+1
  // must equal log2(DEPTH)
  // Memory array
  logic [DATA_WIDTH-1:0] mem [0:DEPTH-1];
  // Write domain: monotonically increasing push count (ADDR_WIDTH+1 bits)
  logic [ADDR_WIDTH + 1-1:0] w_ptr;
  // Read domain: monotonically increasing pop count
  logic [ADDR_WIDTH + 1-1:0] r_pop_cnt;
  // Gray-code CDC: w_ptr (WDomain) → RDomain
  logic [ADDR_WIDTH + 1-1:0] rq2_wptr;
  WToRGraySync #(.WIDTH(ADDR_WIDTH + 1)) wptr_sync (
    .src_clk(w_clk),
    .dst_clk(r_clk),
    .rst(r_rst),
    .data_in(w_ptr),
    .data_out(rq2_wptr)
  );
  // Gray-code CDC: r_pop_cnt (RDomain) → WDomain
  logic [ADDR_WIDTH + 1-1:0] wq2_rcnt;
  RToWGraySync #(.WIDTH(ADDR_WIDTH + 1)) rptr_sync (
    .src_clk(r_clk),
    .dst_clk(w_clk),
    .rst(w_rst),
    .data_in(r_pop_cnt),
    .data_out(wq2_rcnt)
  );
  // Stack pointer and empty-edge detection (read domain only — no CDC needed)
  logic [ADDR_WIDTH + 1-1:0] r_ptr;
  logic was_empty;
  logic full_i;
  logic empty_i;
  assign full_i = w_ptr - wq2_rcnt == DEPTH;
  assign empty_i = rq2_wptr == r_pop_cnt;
  assign r_data = mem[r_ptr[ADDR_WIDTH - 1:0]];
  assign r_empty = empty_i;
  assign w_full = full_i;
  // Write domain: push data
  always_ff @(posedge w_clk or posedge w_rst) begin
    if (w_rst) begin
      w_ptr <= 0;
    end else begin
      if (push & ~full_i) begin
        mem[w_ptr[ADDR_WIDTH - 1:0]] <= w_data;
        w_ptr <= (ADDR_WIDTH + 1)'(w_ptr + 1);
      end
    end
  end
  // Read domain: manage stack pointer
  always_ff @(posedge r_clk or posedge r_rst) begin
    if (r_rst) begin
      r_pop_cnt <= 0;
      r_ptr <= 0;
      was_empty <= 1;
    end else begin
      was_empty <= empty_i;
      if (was_empty & ~empty_i) begin
        r_ptr <= (ADDR_WIDTH + 1)'(rq2_wptr - 1);
      end else if (pop & ~empty_i) begin
        r_pop_cnt <= (ADDR_WIDTH + 1)'(r_pop_cnt + 1);
        r_ptr <= (ADDR_WIDTH + 1)'(r_ptr - 1);
      end
    end
  end

endmodule

