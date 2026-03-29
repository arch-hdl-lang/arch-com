module leading_zero_cnt #(
  parameter int DATA_WIDTH = 32,
  parameter int REVERSE = 0,
  parameter int OUT_WIDTH = $clog2(DATA_WIDTH)
) (
  input logic [DATA_WIDTH-1:0] data,
  output logic [OUT_WIDTH-1:0] leading_zeros,
  output logic all_zeros
);

  logic [OUT_WIDTH-1:0] result;
  logic found;
  always_comb begin
    // Find first set bit from LSB (trailing zero count for REVERSE=1)
    result = 0;
    found = 1'b0;
    for (int i = 0; i <= DATA_WIDTH - 1; i++) begin
      if (~found & data[i +: 1]) begin
        result = OUT_WIDTH'(i);
        found = 1'b1;
      end
    end
    leading_zeros = result;
    all_zeros = data == 0;
  end

endmodule

module cache_mshr #(
  parameter int MSHR_SIZE = 32,
  parameter int CS_LINE_ADDR_WIDTH = 10,
  parameter int WORD_SEL_WIDTH = 4,
  parameter int WORD_SIZE = 4,
  parameter int MSHR_ADDR_WIDTH = $clog2(MSHR_SIZE),
  parameter int TAG_WIDTH = 32 - CS_LINE_ADDR_WIDTH - $clog2(WORD_SIZE) - WORD_SEL_WIDTH,
  parameter int CS_WORD_WIDTH = WORD_SIZE * 8,
  parameter int DATA_WIDTH = WORD_SEL_WIDTH + WORD_SIZE + CS_WORD_WIDTH + TAG_WIDTH
) (
  input logic clk,
  input logic reset,
  input logic allocate_valid,
  input logic [CS_LINE_ADDR_WIDTH-1:0] allocate_addr,
  input logic allocate_rw,
  input logic [DATA_WIDTH-1:0] allocate_data,
  output logic [MSHR_ADDR_WIDTH-1:0] allocate_id,
  output logic allocate_pending,
  output logic [MSHR_ADDR_WIDTH-1:0] allocate_previd,
  output logic allocate_ready,
  input logic finalize_valid,
  input logic [MSHR_ADDR_WIDTH-1:0] finalize_id
);

  // Allocate interface
  // Finalize interface
  // Entry metadata registers
  logic entry_valid [0:MSHR_SIZE-1];
  logic [CS_LINE_ADDR_WIDTH-1:0] entry_addr [0:MSHR_SIZE-1];
  logic entry_write [0:MSHR_SIZE-1];
  logic entry_has_next [0:MSHR_SIZE-1];
  logic [MSHR_ADDR_WIDTH-1:0] entry_next_idx [0:MSHR_SIZE-1];
  // Data RAM
  logic [DATA_WIDTH-1:0] data_mem [0:MSHR_SIZE-1];
  // Wires for LZC inputs/outputs
  logic [MSHR_SIZE-1:0] valid_inv;
  logic [MSHR_SIZE-1:0] match_no_next;
  logic [MSHR_ADDR_WIDTH-1:0] alloc_idx;
  logic full_flag;
  logic [MSHR_ADDR_WIDTH-1:0] prev_idx;
  logic prev_all_zeros;
  // Compute LZC inputs
  always_comb begin
    for (int i = 0; i <= MSHR_SIZE - 1; i++) begin
      valid_inv[i +: 1] = ~entry_valid[i];
    end
    for (int i = 0; i <= MSHR_SIZE - 1; i++) begin
      if (entry_valid[i] & entry_addr[i] == allocate_addr & ~entry_has_next[i]) begin
        match_no_next[i +: 1] = 1'b1;
      end else begin
        match_no_next[i +: 1] = 1'b0;
      end
    end
  end
  // LZC for finding first available slot
  leading_zero_cnt #(.DATA_WIDTH(MSHR_SIZE), .OUT_WIDTH(MSHR_ADDR_WIDTH)) allocate_lzc (
    .data(valid_inv),
    .leading_zeros(alloc_idx),
    .all_zeros(full_flag)
  );
  // LZC for finding previous entry with matching address and no next
  leading_zero_cnt #(.DATA_WIDTH(MSHR_SIZE), .OUT_WIDTH(MSHR_ADDR_WIDTH)) prev_lzc (
    .data(match_no_next),
    .leading_zeros(prev_idx),
    .all_zeros(prev_all_zeros)
  );
  // allocate_ready is combinational: not full
  assign allocate_ready = ~full_flag;
  // Wire for pending detection
  logic has_pending;
  assign has_pending = ~prev_all_zeros;
  // Register outputs and update state
  always_ff @(posedge clk) begin
    if (reset) begin
      allocate_id <= 0;
      allocate_pending <= 1'b0;
      allocate_previd <= 0;
      for (int __ri0 = 0; __ri0 < MSHR_SIZE; __ri0++) begin
        data_mem[__ri0] <= 0;
      end
      for (int __ri0 = 0; __ri0 < MSHR_SIZE; __ri0++) begin
        entry_addr[__ri0] <= 0;
      end
      for (int __ri0 = 0; __ri0 < MSHR_SIZE; __ri0++) begin
        entry_has_next[__ri0] <= 1'b0;
      end
      for (int __ri0 = 0; __ri0 < MSHR_SIZE; __ri0++) begin
        entry_next_idx[__ri0] <= 0;
      end
      for (int __ri0 = 0; __ri0 < MSHR_SIZE; __ri0++) begin
        entry_valid[__ri0] <= 1'b0;
      end
      for (int __ri0 = 0; __ri0 < MSHR_SIZE; __ri0++) begin
        entry_write[__ri0] <= 1'b0;
      end
    end else begin
      // Register the combinational outputs
      allocate_id <= alloc_idx;
      allocate_pending <= has_pending;
      allocate_previd <= prev_idx;
      // Allocate: mark entry valid, store metadata
      if (allocate_valid & ~full_flag) begin
        entry_valid[alloc_idx] <= 1'b1;
        entry_addr[alloc_idx] <= allocate_addr;
        entry_write[alloc_idx] <= allocate_rw;
        entry_has_next[alloc_idx] <= 1'b0;
        entry_next_idx[alloc_idx] <= 0;
        data_mem[alloc_idx] <= allocate_data;
        // If pending, update previous entry's next pointer
        if (has_pending) begin
          entry_has_next[prev_idx] <= 1'b1;
          entry_next_idx[prev_idx] <= alloc_idx;
        end
      end
      // Finalize: invalidate entry
      if (finalize_valid) begin
        entry_valid[finalize_id] <= 1'b0;
        entry_has_next[finalize_id] <= 1'b0;
      end
    end
  end

endmodule

