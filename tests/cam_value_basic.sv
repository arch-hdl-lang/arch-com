// Cam v3 smoke test: optional value-payload bundle.
//
// Adds VAL_W param + write_value + read_value ports. Each entry now
// stores (valid, key, value) instead of just (valid, key); the read
// side returns the value at search_first directly.
module Tag_Value_Cam #(
  parameter int DEPTH = 8,
  parameter int KEY_W = 16,
  parameter int VAL_W = 32
) (
  input logic clk,
  input logic rst,
  input logic write_valid,
  input logic [2:0] write_idx,
  input logic [15:0] write_key,
  input logic [31:0] write_value,
  input logic write_set,
  input logic [15:0] search_key,
  output logic [7:0] search_mask,
  output logic search_any,
  output logic [2:0] search_first,
  output logic [31:0] read_value
);

  logic [DEPTH-1:0]      entry_valid_r;
  logic [KEY_W-1:0]      entry_key_r [DEPTH];
  logic [VAL_W-1:0]      entry_value_r [DEPTH];
  
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
  
  assign read_value = entry_value_r[search_first];
  
  always_ff @(posedge clk) begin
    if (rst) begin
      entry_valid_r <= '0;
    end else begin
      if (write_valid) begin
        if (write_set) begin
          entry_valid_r[write_idx] <= 1'b1;
          entry_key_r[write_idx] <= write_key;
          entry_value_r[write_idx] <= write_value;
        end else begin
          entry_valid_r[write_idx] <= 1'b0;
        end
      end
    end
  end
  
endmodule

