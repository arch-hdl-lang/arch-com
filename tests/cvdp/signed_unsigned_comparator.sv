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

  always_comb begin
    if (!i_enable) begin
      o_greater = 1'b0;
      o_less = 1'b0;
      o_equal = 1'b0;
    end else if (!i_mode) begin
      o_greater = i_A > i_B;
      o_less = i_A < i_B;
      o_equal = i_A == i_B;
    end else begin
      o_greater = $signed(i_A) > $signed(i_B);
      o_less = $signed(i_A) < $signed(i_B);
      o_equal = $signed(i_A) == $signed(i_B);
    end
  end

endmodule

