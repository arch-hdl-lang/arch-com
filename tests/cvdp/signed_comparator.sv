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
    if (~i_enable) begin
      o_greater = 1'b0;
      o_less = 1'b0;
      o_equal = 1'b0;
    end else if (i_mode) begin
      if ($signed(i_A) > $signed(i_B)) begin
        o_greater = 1'b1;
        o_less = 1'b0;
        o_equal = 1'b0;
      end else if ($signed(i_A) < $signed(i_B)) begin
        o_greater = 1'b0;
        o_less = 1'b1;
        o_equal = 1'b0;
      end else begin
        o_greater = 1'b0;
        o_less = 1'b0;
        o_equal = 1'b1;
      end
    end else if (i_A > i_B) begin
      o_greater = 1'b1;
      o_less = 1'b0;
      o_equal = 1'b0;
    end else if (i_A < i_B) begin
      o_greater = 1'b0;
      o_less = 1'b1;
      o_equal = 1'b0;
    end else begin
      o_greater = 1'b0;
      o_less = 1'b0;
      o_equal = 1'b1;
    end
  end

endmodule

