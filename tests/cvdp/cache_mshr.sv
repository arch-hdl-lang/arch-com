module cache_mshr #(
  parameter int MSHR_SIZE = 32,
  parameter int CS_LINE_ADDR_WIDTH = 10,
  parameter int WORD_SEL_WIDTH = 4,
  parameter int WORD_SIZE = 4,
  localparam int MSHR_ADDR_WIDTH = $clog2(MSHR_SIZE),
  localparam int TAG_WIDTH = 32 - CS_LINE_ADDR_WIDTH - $clog2(WORD_SIZE) - WORD_SEL_WIDTH,
  localparam int CS_WORD_WIDTH = WORD_SIZE * 8,
  localparam int DATA_WIDTH = WORD_SEL_WIDTH + WORD_SIZE + CS_WORD_WIDTH + TAG_WIDTH
) (
  input logic clk,
  input logic reset,
  input logic fill_valid,
  input logic [MSHR_ADDR_WIDTH-1:0] fill_id,
  output logic [CS_LINE_ADDR_WIDTH-1:0] fill_addr,
  output logic dequeue_valid,
  output logic [CS_LINE_ADDR_WIDTH-1:0] dequeue_addr,
  output logic dequeue_rw,
  output logic [DATA_WIDTH-1:0] dequeue_data,
  output logic [MSHR_ADDR_WIDTH-1:0] dequeue_id,
  input logic dequeue_ready,
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

  // Memory fill interface
  // Dequeue interface
  // Allocate interface
  // Finalize interface
  // Entry metadata registers
  logic entry_valid [MSHR_SIZE-1:0];
  logic [CS_LINE_ADDR_WIDTH-1:0] entry_addr [MSHR_SIZE-1:0];
  logic entry_write [MSHR_SIZE-1:0];
  logic entry_has_next [MSHR_SIZE-1:0];
  logic [MSHR_ADDR_WIDTH-1:0] entry_next_idx [MSHR_SIZE-1:0];
  // Data RAM
  logic [DATA_WIDTH-1:0] data_mem [MSHR_SIZE-1:0];
  // Registered outputs for allocate
  logic [MSHR_ADDR_WIDTH-1:0] allocate_id_r;
  logic allocate_pending_r;
  logic [MSHR_ADDR_WIDTH-1:0] allocate_previd_r;
  // Dequeue state registers
  logic dq_valid_r;
  logic [MSHR_ADDR_WIDTH-1:0] dq_idx_r;
  // Wires for priority encoder
  logic [MSHR_ADDR_WIDTH-1:0] alloc_idx;
  logic full_flag;
  logic [MSHR_ADDR_WIDTH-1:0] prev_idx;
  logic prev_all_zeros;
  // Priority encoder: find first free slot (LSB-first)
  always_comb begin
    alloc_idx = 0;
    full_flag = 1'b1;
    for (int i = 0; i <= MSHR_SIZE - 1; i++) begin
      if (~full_flag == 1'b0) begin
        if (~entry_valid[i]) begin
          alloc_idx = MSHR_ADDR_WIDTH'(i);
          full_flag = 1'b0;
        end
      end
    end
  end
  // Priority encoder: find first matching entry with no next (LSB-first)
  always_comb begin
    prev_idx = 0;
    prev_all_zeros = 1'b1;
    for (int i = 0; i <= MSHR_SIZE - 1; i++) begin
      if (~prev_all_zeros == 1'b0) begin
        if (entry_valid[i] & entry_addr[i] == allocate_addr & ~entry_has_next[i]) begin
          prev_idx = MSHR_ADDR_WIDTH'(i);
          prev_all_zeros = 1'b0;
        end
      end
    end
  end
  // allocate_ready is combinational: not full
  assign allocate_ready = ~full_flag;
  // Pending detection
  logic has_pending;
  assign has_pending = ~prev_all_zeros;
  // Register outputs and update state
  always_ff @(posedge clk) begin
    if (reset) begin
      allocate_id_r <= 0;
      allocate_pending_r <= 0;
      allocate_previd_r <= 0;
      for (int __ri0 = 0; __ri0 < MSHR_SIZE; __ri0++) begin
        data_mem[__ri0] <= 0;
      end
      dq_idx_r <= 0;
      dq_valid_r <= 0;
      for (int __ri0 = 0; __ri0 < MSHR_SIZE; __ri0++) begin
        entry_addr[__ri0] <= 0;
      end
      for (int __ri0 = 0; __ri0 < MSHR_SIZE; __ri0++) begin
        entry_has_next[__ri0] <= 0;
      end
      for (int __ri0 = 0; __ri0 < MSHR_SIZE; __ri0++) begin
        entry_next_idx[__ri0] <= 0;
      end
      for (int __ri0 = 0; __ri0 < MSHR_SIZE; __ri0++) begin
        entry_valid[__ri0] <= 0;
      end
      for (int __ri0 = 0; __ri0 < MSHR_SIZE; __ri0++) begin
        entry_write[__ri0] <= 0;
      end
    end else begin
      // Register the combinational outputs
      allocate_id_r <= alloc_idx;
      allocate_pending_r <= has_pending;
      allocate_previd_r <= prev_idx;
      // Dequeue FSM: fill_valid starts traversal, then walk linked list
      if (fill_valid) begin
        dq_valid_r <= 1'b1;
        dq_idx_r <= fill_id;
      end else if (dq_valid_r & dequeue_ready) begin
        if (entry_has_next[dq_idx_r]) begin
          dq_idx_r <= entry_next_idx[dq_idx_r];
        end else begin
          dq_valid_r <= 1'b0;
        end
      end
      // Finalize: invalidate entry (placed before allocate so allocate wins on conflict)
      if (finalize_valid) begin
        entry_valid[finalize_id] <= 1'b0;
        entry_has_next[finalize_id] <= 1'b0;
      end
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
    end
  end
  // Output assignments
  assign allocate_id = allocate_id_r;
  assign allocate_pending = allocate_pending_r;
  assign allocate_previd = allocate_previd_r;
  // Fill address output
  assign fill_addr = entry_addr[fill_id];
  // Dequeue outputs: driven from registered index
  assign dequeue_valid = dq_valid_r;
  assign dequeue_addr = entry_addr[dq_idx_r];
  assign dequeue_rw = entry_write[dq_idx_r];
  assign dequeue_data = data_mem[dq_idx_r];
  assign dequeue_id = dq_idx_r;

endmodule

