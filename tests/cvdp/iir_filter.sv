module iir_filter #(
  parameter int b0 = 1,
  parameter int b1 = 0,
  parameter int b2 = 0,
  parameter int b3 = 0,
  parameter int b4 = 0,
  parameter int b5 = 0,
  parameter int b6 = 0,
  parameter int a1 = 0,
  parameter int a2 = 0,
  parameter int a3 = 0,
  parameter int a4 = 0,
  parameter int a5 = 0,
  parameter int a6 = 0
) (
  input logic clk,
  input logic rst,
  input logic signed [15:0] x,
  output logic signed [15:0] y
);

  logic signed [15:0] x1;
  logic signed [15:0] x2;
  logic signed [15:0] x3;
  logic signed [15:0] x4;
  logic signed [15:0] x5;
  logic signed [15:0] x6;
  logic signed [15:0] y1;
  logic signed [15:0] y2;
  logic signed [15:0] y3;
  logic signed [15:0] y4;
  logic signed [15:0] y5;
  logic signed [15:0] y6;
  logic signed [15:0] y_reg;
  // b*x terms
  logic signed [47:0] t0;
  assign t0 = 48'({{(48-$bits(x)){x[$bits(x)-1]}}, x} * $signed(48'($unsigned(b0))));
  logic signed [47:0] t1;
  assign t1 = 48'({{(48-$bits(x1)){x1[$bits(x1)-1]}}, x1} * $signed(48'($unsigned(b1))));
  logic signed [47:0] t2;
  assign t2 = 48'({{(48-$bits(x2)){x2[$bits(x2)-1]}}, x2} * $signed(48'($unsigned(b2))));
  logic signed [47:0] t3;
  assign t3 = 48'({{(48-$bits(x3)){x3[$bits(x3)-1]}}, x3} * $signed(48'($unsigned(b3))));
  logic signed [47:0] t4;
  assign t4 = 48'({{(48-$bits(x4)){x4[$bits(x4)-1]}}, x4} * $signed(48'($unsigned(b4))));
  logic signed [47:0] t5;
  assign t5 = 48'({{(48-$bits(x5)){x5[$bits(x5)-1]}}, x5} * $signed(48'($unsigned(b5))));
  logic signed [47:0] t6;
  assign t6 = 48'({{(48-$bits(x6)){x6[$bits(x6)-1]}}, x6} * $signed(48'($unsigned(b6))));
  // a*y terms
  logic signed [47:0] u1;
  assign u1 = 48'({{(48-$bits(y1)){y1[$bits(y1)-1]}}, y1} * $signed(48'($unsigned(a1))));
  logic signed [47:0] u2;
  assign u2 = 48'({{(48-$bits(y2)){y2[$bits(y2)-1]}}, y2} * $signed(48'($unsigned(a2))));
  logic signed [47:0] u3;
  assign u3 = 48'({{(48-$bits(y3)){y3[$bits(y3)-1]}}, y3} * $signed(48'($unsigned(a3))));
  logic signed [47:0] u4;
  assign u4 = 48'({{(48-$bits(y4)){y4[$bits(y4)-1]}}, y4} * $signed(48'($unsigned(a4))));
  logic signed [47:0] u5;
  assign u5 = 48'({{(48-$bits(y5)){y5[$bits(y5)-1]}}, y5} * $signed(48'($unsigned(a5))));
  logic signed [47:0] u6;
  assign u6 = 48'({{(48-$bits(y6)){y6[$bits(y6)-1]}}, y6} * $signed(48'($unsigned(a6))));
  // sum feedforward
  logic signed [47:0] ff01;
  assign ff01 = 48'(t0 + t1);
  logic signed [47:0] ff23;
  assign ff23 = 48'(t2 + t3);
  logic signed [47:0] ff45;
  assign ff45 = 48'(t4 + t5);
  logic signed [47:0] ff_b;
  assign ff_b = 48'(48'(ff01 + ff23) + 48'(ff45 + t6));
  // sum feedback
  logic signed [47:0] fb12;
  assign fb12 = 48'(u1 + u2);
  logic signed [47:0] fb34;
  assign fb34 = 48'(u3 + u4);
  logic signed [47:0] fb56;
  assign fb56 = 48'(u5 + u6);
  logic signed [47:0] fb_a;
  assign fb_a = 48'(48'(fb12 + fb34) + fb56);
  // final accumulator
  logic signed [47:0] acc;
  assign acc = 48'(ff_b - fb_a);
  logic signed [15:0] zero16;
  assign zero16 = $signed(16'($unsigned(0)));
  assign y = y_reg;
  always_ff @(posedge clk) begin
    if (rst) begin
      x1 <= zero16;
      x2 <= zero16;
      x3 <= zero16;
      x4 <= zero16;
      x5 <= zero16;
      x6 <= zero16;
      y1 <= zero16;
      y2 <= zero16;
      y3 <= zero16;
      y4 <= zero16;
      y5 <= zero16;
      y6 <= zero16;
      y_reg <= zero16;
    end else begin
      x1 <= x;
      x2 <= x1;
      x3 <= x2;
      x4 <= x3;
      x5 <= x4;
      x6 <= x5;
      y_reg <= 16'(acc);
      y1 <= 16'(acc);
      y2 <= y1;
      y3 <= y2;
      y4 <= y3;
      y5 <= y4;
      y6 <= y5;
    end
  end

endmodule

