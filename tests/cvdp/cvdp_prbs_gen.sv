module cvdp_prbs_gen #(
  parameter int CHECK_MODE = 0,
  parameter int POLY_LENGTH = 31,
  parameter int POLY_TAP = 3,
  parameter int WIDTH = 16
) (
  input logic clk,
  input logic rst,
  input logic [WIDTH-1:0] data_in,
  output logic [WIDTH-1:0] data_out
);

  // PRBS shift register
  logic [POLY_LENGTH-1:0] prbs_reg = 0;
  logic [WIDTH-1:0] data_out_r = 0;
  // Current PRBS state (updated iteratively in comb for loop)
  logic [POLY_LENGTH-1:0] prbs_cur;
  logic [WIDTH-1:0] xor_bits;
  // All-ones constants
  logic [POLY_LENGTH-1:0] ones_poly;
  assign ones_poly = ~POLY_LENGTH'($unsigned(0));
  logic [WIDTH-1:0] ones_width;
  assign ones_width = ~WIDTH'($unsigned(0));
  always_comb begin
    prbs_cur = prbs_reg;
    for (int i = 0; i <= WIDTH - 1; i++) begin
      xor_bits[i] = prbs_cur[POLY_LENGTH - POLY_TAP] ^ prbs_cur[0];
      prbs_cur = {xor_bits[i], prbs_cur[POLY_LENGTH - 1:1]};
    end
    // Python model uses MSB-first indexing: tap at [poly_tap-1] and [poly_length-1]
    // In SV (LSB-first): these map to bit [POLY_LENGTH - POLY_TAP] and bit [0]
    // Shift: drop LSB (bit 0), insert xor at MSB
    data_out = data_out_r;
  end
  always_ff @(posedge clk) begin
    if (rst) begin
      prbs_reg <= ones_poly;
      data_out_r <= ones_width;
    end else begin
      prbs_reg <= prbs_cur;
      if (CHECK_MODE == 0) begin
        data_out_r <= xor_bits;
      end else begin
        data_out_r <= xor_bits ^ data_in;
      end
    end
  end

endmodule

