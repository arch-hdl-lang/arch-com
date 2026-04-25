// Phase A smoke test: a `cam` construct parses and type-checks.
// Codegen (Phase B) and sim (Phase C) are not yet implemented.
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
    end else if (write_valid) begin
      if (write_set) begin
        entry_valid_r[write_idx] <= 1'b1;
        entry_key_r[write_idx] <= write_key;
      end else begin
        entry_valid_r[write_idx] <= 1'b0;
      end
    end
  end
  
endmodule

