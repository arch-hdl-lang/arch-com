// Address-CAM helper used by cache_mshr to find the tail of the chain
// for a given line address. Maintains (valid, addr) per slot; the cam's
// dual-write port commits allocate (port 2, wins on conflict) and
// finalize (port 1) on the same edge.
module Mshr_Addr_Cam #(
  parameter int DEPTH = 32,
  parameter int KEY_W = 10
) (
  input logic clk,
  input logic rst,
  input logic write_valid,
  input logic [4:0] write_idx,
  input logic [9:0] write_key,
  input logic write_set,
  input logic write2_valid,
  input logic [4:0] write2_idx,
  input logic [9:0] write2_key,
  input logic write2_set,
  input logic [9:0] search_key,
  output logic [31:0] search_mask,
  output logic search_any,
  output logic [4:0] search_first
);

  logic [DEPTH-1:0]      entry_valid_r;
  logic [KEY_W-1:0]      entry_key_r [DEPTH];
  
  always_comb begin
    for (int i = 0; i < DEPTH; i++) begin
      search_mask[i] = entry_valid_r[i] && (entry_key_r[i] == search_key);
    end
  end
  assign search_any = |search_mask;
  
  always_comb begin
    search_first = '0;
    for (int i = DEPTH-1; i >= 0; i--) begin
      if (search_mask[i]) search_first = i[$clog2(DEPTH)-1:0];
    end
  end
  
  always_ff @(posedge clk) begin
    if (rst) begin
      entry_valid_r <= '0;
    end else begin
      if (write_valid) begin
        if (write_set) begin
          entry_valid_r[write_idx] <= 1'b1;
          entry_key_r[write_idx] <= write_key;
        end else begin
          entry_valid_r[write_idx] <= 1'b0;
        end
      end
      if (write2_valid) begin
        if (write2_set) begin
          entry_valid_r[write2_idx] <= 1'b1;
          entry_key_r[write2_idx] <= write2_key;
        end else begin
          entry_valid_r[write2_idx] <= 1'b0;
        end
      end
    end
  end
  
endmodule

// Port 1: finalize (clear by id)
// Port 2: allocate (insert at alloc_idx with allocate_addr) — wins on same-idx conflict
module cache_mshr #(
  parameter int MSHR_SIZE = 32,
  parameter int CS_LINE_ADDR_WIDTH = 10,
  parameter int WORD_SEL_WIDTH = 4,
  parameter int WORD_SIZE = 4,
  localparam int MSHR_ADDR_WIDTH = $clog2(MSHR_SIZE),
  localparam int TAG_WIDTH = ((32 - CS_LINE_ADDR_WIDTH) - $clog2(WORD_SIZE)) - WORD_SEL_WIDTH,
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
  logic [MSHR_SIZE-1:0] entry_valid;
  logic [MSHR_SIZE-1:0] [CS_LINE_ADDR_WIDTH-1:0] entry_addr;
  logic [MSHR_SIZE-1:0] entry_write;
  logic [MSHR_SIZE-1:0] entry_has_next;
  logic [MSHR_SIZE-1:0] [MSHR_ADDR_WIDTH-1:0] entry_next_idx;
  // Data RAM
  logic [MSHR_SIZE-1:0] [DATA_WIDTH-1:0] data_mem;
  // Registered outputs for allocate
  logic [MSHR_ADDR_WIDTH-1:0] allocate_id_r;
  logic allocate_pending_r;
  logic [MSHR_ADDR_WIDTH-1:0] allocate_previd_r;
  // Dequeue state registers
  logic dq_valid_r;
  logic [MSHR_ADDR_WIDTH-1:0] dq_idx_r;
  // Wires for the free-slot priority encoder
  logic [MSHR_ADDR_WIDTH-1:0] alloc_idx;
  logic full_flag;
  // Wires for the address-CAM lookup chain
  logic [MSHR_SIZE-1:0] addr_search_mask;
  logic addr_search_any;
  // unused; required to bind cam port
  logic [MSHR_ADDR_WIDTH-1:0] addr_search_first;
  // unused; required to bind cam port
  logic [MSHR_SIZE-1:0] has_next_mask;
  logic [MSHR_SIZE-1:0] tail_mask;
  logic [MSHR_ADDR_WIDTH-1:0] prev_idx;
  logic prev_all_zeros;
  // Address CAM: maintains (valid, addr) per slot. Port 1 = finalize,
  // port 2 = allocate (wins on same-idx conflict, matching the original
  // last-assignment-wins seq semantics).
  Mshr_Addr_Cam #(.DEPTH(MSHR_SIZE), .KEY_W(CS_LINE_ADDR_WIDTH)) addr_cam (
    .clk(clk),
    .rst(reset),
    .write_valid(finalize_valid),
    .write_idx(finalize_id),
    .write_key(CS_LINE_ADDR_WIDTH'($unsigned(0))),
    .write_set(1'b0),
    .write2_valid(allocate_valid & ~full_flag),
    .write2_idx(alloc_idx),
    .write2_key(allocate_addr),
    .write2_set(1'b1),
    .search_key(allocate_addr),
    .search_mask(addr_search_mask),
    .search_any(addr_search_any),
    .search_first(addr_search_first)
  );
  // ignored on clear; explicit width to silence iverilog literal-prune warning
  // Free-slot priority encoder: find first ~entry_valid (LSB-first).
  // Not a CAM lookup (no key matching), so still hand-rolled.
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
  // Pack entry_has_next into a UInt mask so we can AND it with the
  // CAM's search_mask. The tail of a chain is an entry that matches
  // the address AND has no next pointer.
  always_comb begin
    has_next_mask = 0;
    for (int i = 0; i <= MSHR_SIZE - 1; i++) begin
      if (entry_has_next[i]) begin
        has_next_mask = has_next_mask | MSHR_SIZE'($unsigned(1)) << i;
      end
    end
  end
  logic [MSHR_SIZE-1:0] tail_mask_w;
  assign tail_mask_w = addr_search_mask & ~has_next_mask;
  assign tail_mask = tail_mask_w;
  // Priority-encode tail_mask to get prev_idx (LSB-first).
  always_comb begin
    prev_idx = 0;
    prev_all_zeros = 1'b1;
    for (int i = 0; i <= MSHR_SIZE - 1; i++) begin
      if (~prev_all_zeros == 1'b0) begin
        if (tail_mask >> i & 1) begin
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
  // synopsys translate_off
  // Auto-generated safety assertions (bounds / divide-by-zero)
  _auto_bound_vec_0: assert property (@(posedge clk) disable iff (reset) (dq_idx_r) < (MSHR_SIZE))
    else $fatal(1, "BOUNDS VIOLATION: cache_mshr._auto_bound_vec_0");
  _auto_bound_vec_1: assert property (@(posedge clk) disable iff (reset) (finalize_id) < (MSHR_SIZE))
    else $fatal(1, "BOUNDS VIOLATION: cache_mshr._auto_bound_vec_1");
  _auto_bound_vec_2: assert property (@(posedge clk) disable iff (reset) (alloc_idx) < (MSHR_SIZE))
    else $fatal(1, "BOUNDS VIOLATION: cache_mshr._auto_bound_vec_2");
  _auto_bound_vec_3: assert property (@(posedge clk) disable iff (reset) (prev_idx) < (MSHR_SIZE))
    else $fatal(1, "BOUNDS VIOLATION: cache_mshr._auto_bound_vec_3");
  // synopsys translate_on

endmodule

