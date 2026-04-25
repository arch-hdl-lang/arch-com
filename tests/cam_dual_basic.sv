// Cam v2 smoke test: dual-write port. Both write ports can fire in the
// same cycle; if they target the same index, port 2 wins (last-write).
module Mshr_Addr_Cam_Dual #(
  parameter int DEPTH = 16,
  parameter int KEY_W = 8
) (
  input logic clk,
  input logic rst,
  input logic write_valid,
  input logic [3:0] write_idx,
  input logic [7:0] write_key,
  input logic write_set,
  input logic write2_valid,
  input logic [3:0] write2_idx,
  input logic [7:0] write2_key,
  input logic write2_set,
  input logic [7:0] search_key,
  output logic [15:0] search_mask,
  output logic search_any,
  output logic [3:0] search_first
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

// Write port 1 (e.g. finalize/clear path)
// Write port 2 (e.g. allocate/insert path) — wins on same-index conflict
