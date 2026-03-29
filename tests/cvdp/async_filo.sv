module async_filo #(
  parameter int DATA_WIDTH = 16,
  parameter int DEPTH = 8,
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

  // must equal log2(DEPTH)
  // Memory array
  logic [DATA_WIDTH-1:0] mem [0:DEPTH-1];
  // Write domain: monotonically increasing push count (ADDR_WIDTH+1 bits)
  logic [ADDR_WIDTH + 1-1:0] w_ptr;
  // Gray-encode w_ptr before crossing to read domain
  logic [ADDR_WIDTH + 1-1:0] w_ptr_gray;
  assign w_ptr_gray = w_ptr ^ w_ptr >> 1;
  // 2-flop sync of gray-coded w_ptr into read domain (safe: 1-bit-at-a-time)
  logic [ADDR_WIDTH + 1-1:0] rq1_wptr_gray;
  logic [ADDR_WIDTH + 1-1:0] rq2_wptr_gray;
  // Gray-decode rq2_wptr_gray back to binary in read domain
  // (hardcoded for ADDR_WIDTH=3, i.e. 4-bit counters)
  logic rq2_b3;
  assign rq2_b3 = rq2_wptr_gray[3];
  logic rq2_b2;
  assign rq2_b2 = rq2_b3 ^ rq2_wptr_gray[2];
  logic rq2_b1;
  assign rq2_b1 = rq2_b2 ^ rq2_wptr_gray[1];
  logic rq2_b0;
  assign rq2_b0 = rq2_b1 ^ rq2_wptr_gray[0];
  logic [4-1:0] rq2_wptr;
  assign rq2_wptr = {rq2_b3, rq2_b2, rq2_b1, rq2_b0};
  // Read domain: monotonically increasing pop count
  logic [ADDR_WIDTH + 1-1:0] r_pop_cnt;
  // Gray-encode r_pop_cnt before crossing to write domain
  logic [ADDR_WIDTH + 1-1:0] r_pop_cnt_gray;
  assign r_pop_cnt_gray = r_pop_cnt ^ r_pop_cnt >> 1;
  // 2-flop sync of gray-coded r_pop_cnt into write domain (safe: 1-bit-at-a-time)
  logic [ADDR_WIDTH + 1-1:0] wq1_rcnt_gray;
  logic [ADDR_WIDTH + 1-1:0] wq2_rcnt_gray;
  // Gray-decode wq2_rcnt_gray back to binary in write domain
  logic wq2_b3;
  assign wq2_b3 = wq2_rcnt_gray[3];
  logic wq2_b2;
  assign wq2_b2 = wq2_b3 ^ wq2_rcnt_gray[2];
  logic wq2_b1;
  assign wq2_b1 = wq2_b2 ^ wq2_rcnt_gray[1];
  logic wq2_b0;
  assign wq2_b0 = wq2_b1 ^ wq2_rcnt_gray[0];
  logic [4-1:0] wq2_rcnt;
  assign wq2_rcnt = {wq2_b3, wq2_b2, wq2_b1, wq2_b0};
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
  // Write domain: sync gray pop count in, push data
  always_ff @(posedge w_clk or posedge w_rst) begin
    if (w_rst) begin
      w_ptr <= 0;
      wq1_rcnt_gray <= 0;
      wq2_rcnt_gray <= 0;
    end else begin
      wq1_rcnt_gray <= r_pop_cnt_gray;
      wq2_rcnt_gray <= wq1_rcnt_gray;
      if (push & ~full_i) begin
        mem[w_ptr[ADDR_WIDTH - 1:0]] <= w_data;
        w_ptr <= (ADDR_WIDTH + 1)'(w_ptr + 1);
      end
    end
  end
  // Read domain: sync gray push count in, manage stack pointer
  always_ff @(posedge r_clk or posedge r_rst) begin
    if (r_rst) begin
      r_pop_cnt <= 0;
      r_ptr <= 0;
      rq1_wptr_gray <= 0;
      rq2_wptr_gray <= 0;
      was_empty <= 1;
    end else begin
      rq1_wptr_gray <= w_ptr_gray;
      rq2_wptr_gray <= rq1_wptr_gray;
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

