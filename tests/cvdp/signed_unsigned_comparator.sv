module signed_unsigned_comparator #(
  parameter int WIDTH = 5
) (
  input logic [WIDTH-1:0] i_A,
  input logic [WIDTH-1:0] i_B,
  input logic i_enable,
  input logic i_mode,
  output logic o_greater,
  output logic o_less,
  output logic o_equal
);

  logic gt;
  logic lt;
  logic eq;
  always_comb begin
    if (i_mode) begin
      // Signed mode
      gt = $signed(i_A) > $signed(i_B) ? 1'd1 : 1'd0;
      lt = $signed(i_A) < $signed(i_B) ? 1'd1 : 1'd0;
      eq = i_A == i_B ? 1'd1 : 1'd0;
    end else begin
      // Magnitude (unsigned) mode
      gt = i_A > i_B ? 1'd1 : 1'd0;
      lt = i_A < i_B ? 1'd1 : 1'd0;
      eq = i_A == i_B ? 1'd1 : 1'd0;
    end
    o_greater = i_enable & gt;
    o_less = i_enable & lt;
    o_equal = i_enable & eq;
  end

endmodule

