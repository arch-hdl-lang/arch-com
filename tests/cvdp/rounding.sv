module rounding #(
  parameter int WIDTH = 24
) (
  input logic [WIDTH-1:0] in_data,
  input logic sign,
  input logic roundin,
  input logic stickyin,
  input logic [2:0] rm,
  output logic [WIDTH-1:0] out_data,
  output logic inexact,
  output logic cout,
  output logic r_up
);

  logic is_inexact;
  assign is_inexact = roundin | stickyin;
  logic all_ones;
  assign all_ones = &in_data;
  logic [WIDTH + 1-1:0] inc;
  assign inc = (WIDTH + 1)'((WIDTH + 1)'($unsigned(in_data)) + 1);
  logic do_round_up;
  always_comb begin
    if (rm == 0) begin
      do_round_up = roundin & (stickyin | in_data[0:0]);
    end else if (rm == 1) begin
      do_round_up = 1'b0;
    end else if (rm == 2) begin
      do_round_up = ~sign & is_inexact;
    end else if (rm == 3) begin
      do_round_up = sign & is_inexact & ~all_ones;
    end else if (rm == 4) begin
      do_round_up = roundin;
    end else begin
      do_round_up = 1'b0;
    end
    if (do_round_up) begin
      out_data = inc[WIDTH - 1:0];
      cout = all_ones;
    end else begin
      out_data = in_data;
      cout = 1'b0;
    end
    inexact = is_inexact;
    r_up = do_round_up;
  end

endmodule

