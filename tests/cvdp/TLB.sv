// Address-CAM helper for the TLB virtual-tag lookup. Caller owns
// `valid_bits` externally so a single-cycle flush is possible without
// multi-cycle cam writes (cam v1 has one write per cycle).
module Tlb_Cam #(
  parameter int DEPTH = 4,
  parameter int KEY_W = 8
) (
  input logic clk,
  input logic rst,
  input logic write_valid,
  input logic [1:0] write_idx,
  input logic [7:0] write_key,
  input logic write_set,
  input logic [7:0] search_key,
  output logic [3:0] search_mask,
  output logic search_any,
  output logic [1:0] search_first
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
    end
  end
  
endmodule

module TLB #(
  parameter int TLB_SIZE = 4,
  parameter int ADDR_WIDTH = 8,
  parameter int PAGE_WIDTH = 8
) (
  input logic clk,
  input logic rst,
  input logic [ADDR_WIDTH-1:0] virtual_address,
  input logic tlb_write_enable,
  input logic flsh,
  input logic [PAGE_WIDTH-1:0] page_table_entry,
  output logic [PAGE_WIDTH-1:0] physical_address,
  output logic hit,
  output logic miss
);

  // External valid bits — separate from the cam's internal valid so a
  // single-cycle flush stays single-cycle (cam v1 has one write port,
  // and clearing N entries would otherwise take N cycles).
  logic [TLB_SIZE-1:0] valid_bits;
  logic [1:0] replacement_idx;
  // Per-entry physical pages — addressed by the priority-encoded match
  // index. The cam stores the virtual_address keys.
  logic [TLB_SIZE-1:0] [PAGE_WIDTH-1:0] physical_pages;
  logic [TLB_SIZE-1:0] cam_search_mask;
  logic cam_search_any;
  // unused; required to bind cam port
  logic [1:0] cam_search_first;
  // unused; required to bind cam port
  logic [1:0] hit_idx;
  logic any_match;
  Tlb_Cam #(.DEPTH(TLB_SIZE), .KEY_W(ADDR_WIDTH)) tlb_cam (
    .clk(clk),
    .rst(rst),
    .write_valid(tlb_write_enable & ~flsh),
    .write_idx(replacement_idx),
    .write_key(virtual_address),
    .write_set(1'b1),
    .search_key(virtual_address),
    .search_mask(cam_search_mask),
    .search_any(cam_search_any),
    .search_first(cam_search_first)
  );
  // A "real" match is one the cam reports AND whose external valid bit
  // is set. Flush only clears valid_bits, not the cam's internal state,
  // so the AND below is what makes flush a single-cycle operation.
  logic [TLB_SIZE-1:0] effective_match;
  assign effective_match = valid_bits & cam_search_mask;
  // Priority encoder for first effective match (LSB-first).
  always_comb begin
    hit_idx = 0;
    any_match = 1'b0;
    for (int i = 0; i <= TLB_SIZE - 1; i++) begin
      if (~any_match) begin
        if (effective_match >> i & 1) begin
          hit_idx = 2'(i);
          any_match = 1'b1;
        end
      end
    end
  end
  always_comb begin
    if (any_match) begin
      hit = 1'b1;
      miss = 1'b0;
      physical_address = physical_pages[hit_idx];
    end else begin
      hit = 1'b0;
      miss = 1'b1;
      physical_address = page_table_entry;
    end
  end
  always_ff @(posedge clk) begin
    if (rst) begin
      for (int __ri0 = 0; __ri0 < TLB_SIZE; __ri0++) begin
        physical_pages[__ri0] <= 0;
      end
      replacement_idx <= 0;
      valid_bits <= 0;
    end else begin
      if (flsh) begin
        valid_bits <= 0;
      end else if (tlb_write_enable) begin
        physical_pages[replacement_idx] <= page_table_entry;
        valid_bits <= valid_bits | TLB_SIZE'($unsigned(1)) << replacement_idx;
        replacement_idx <= (2 > 1 ? 2 : 1)'(replacement_idx + 1);
      end
    end
  end
  // synopsys translate_off
  // Auto-generated safety assertions (bounds / divide-by-zero)
  _auto_bound_vec_0: assert property (@(posedge clk) disable iff (rst) (replacement_idx) < (TLB_SIZE))
    else $fatal(1, "BOUNDS VIOLATION: TLB._auto_bound_vec_0");
  // synopsys translate_on

endmodule

