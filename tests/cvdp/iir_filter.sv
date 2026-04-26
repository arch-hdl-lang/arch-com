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

  // x_pipe@0 = x (current input), x_pipe@K = x delayed by K cycles.
  logic signed [15:0] x_pipe_stg1;
  logic signed [15:0] x_pipe_stg2;
  logic signed [15:0] x_pipe_stg3;
  logic signed [15:0] x_pipe_stg4;
  logic signed [15:0] x_pipe_stg5;
  logic signed [15:0] x_pipe;
  always_ff @(posedge clk) begin
    if (rst) begin
      x_pipe_stg1 <= '0;
      x_pipe_stg2 <= '0;
      x_pipe_stg3 <= '0;
      x_pipe_stg4 <= '0;
      x_pipe_stg5 <= '0;
      x_pipe <= '0;
    end else begin
      x_pipe_stg1 <= x;
      x_pipe_stg2 <= x_pipe_stg1;
      x_pipe_stg3 <= x_pipe_stg2;
      x_pipe_stg4 <= x_pipe_stg3;
      x_pipe_stg5 <= x_pipe_stg4;
      x_pipe <= x_pipe_stg5;
    end
  end
  // y_reg captures the current filter output. y_pipe@K reads y_reg
  // from K cycles ago (so y_reg itself = y at lag 0, y_pipe@1 = lag 1, …).
  logic signed [15:0] y_reg;
  logic signed [15:0] y_pipe_stg1;
  logic signed [15:0] y_pipe_stg2;
  logic signed [15:0] y_pipe_stg3;
  logic signed [15:0] y_pipe_stg4;
  logic signed [15:0] y_pipe;
  always_ff @(posedge clk) begin
    if (rst) begin
      y_pipe_stg1 <= '0;
      y_pipe_stg2 <= '0;
      y_pipe_stg3 <= '0;
      y_pipe_stg4 <= '0;
      y_pipe <= '0;
    end else begin
      y_pipe_stg1 <= y_reg;
      y_pipe_stg2 <= y_pipe_stg1;
      y_pipe_stg3 <= y_pipe_stg2;
      y_pipe_stg4 <= y_pipe_stg3;
      y_pipe <= y_pipe_stg4;
    end
  end
  // b*x terms — the K-th tap multiplied by b_K.
  logic signed [47:0] t0;
  assign t0 = 48'({{(48-$bits(x)){x[$bits(x)-1]}}, x} * $signed(48'($unsigned(b0))));
  logic signed [47:0] t1;
  assign t1 = 48'({{(48-$bits(x_pipe_stg1)){x_pipe_stg1[$bits(x_pipe_stg1)-1]}}, x_pipe_stg1} * $signed(48'($unsigned(b1))));
  logic signed [47:0] t2;
  assign t2 = 48'({{(48-$bits(x_pipe_stg2)){x_pipe_stg2[$bits(x_pipe_stg2)-1]}}, x_pipe_stg2} * $signed(48'($unsigned(b2))));
  logic signed [47:0] t3;
  assign t3 = 48'({{(48-$bits(x_pipe_stg3)){x_pipe_stg3[$bits(x_pipe_stg3)-1]}}, x_pipe_stg3} * $signed(48'($unsigned(b3))));
  logic signed [47:0] t4;
  assign t4 = 48'({{(48-$bits(x_pipe_stg4)){x_pipe_stg4[$bits(x_pipe_stg4)-1]}}, x_pipe_stg4} * $signed(48'($unsigned(b4))));
  logic signed [47:0] t5;
  assign t5 = 48'({{(48-$bits(x_pipe_stg5)){x_pipe_stg5[$bits(x_pipe_stg5)-1]}}, x_pipe_stg5} * $signed(48'($unsigned(b5))));
  logic signed [47:0] t6;
  assign t6 = 48'({{(48-$bits(x_pipe)){x_pipe[$bits(x_pipe)-1]}}, x_pipe} * $signed(48'($unsigned(b6))));
  // a*y terms — y1 = y_reg, y2..y6 = y_pipe@1..@5.
  logic signed [47:0] u1;
  assign u1 = 48'({{(48-$bits(y_reg)){y_reg[$bits(y_reg)-1]}}, y_reg} * $signed(48'($unsigned(a1))));
  logic signed [47:0] u2;
  assign u2 = 48'({{(48-$bits(y_pipe_stg1)){y_pipe_stg1[$bits(y_pipe_stg1)-1]}}, y_pipe_stg1} * $signed(48'($unsigned(a2))));
  logic signed [47:0] u3;
  assign u3 = 48'({{(48-$bits(y_pipe_stg2)){y_pipe_stg2[$bits(y_pipe_stg2)-1]}}, y_pipe_stg2} * $signed(48'($unsigned(a3))));
  logic signed [47:0] u4;
  assign u4 = 48'({{(48-$bits(y_pipe_stg3)){y_pipe_stg3[$bits(y_pipe_stg3)-1]}}, y_pipe_stg3} * $signed(48'($unsigned(a4))));
  logic signed [47:0] u5;
  assign u5 = 48'({{(48-$bits(y_pipe_stg4)){y_pipe_stg4[$bits(y_pipe_stg4)-1]}}, y_pipe_stg4} * $signed(48'($unsigned(a5))));
  logic signed [47:0] u6;
  assign u6 = 48'({{(48-$bits(y_pipe)){y_pipe[$bits(y_pipe)-1]}}, y_pipe} * $signed(48'($unsigned(a6))));
  // Sum feedforward.
  logic signed [47:0] ff01;
  assign ff01 = 48'(t0 + t1);
  logic signed [47:0] ff23;
  assign ff23 = 48'(t2 + t3);
  logic signed [47:0] ff45;
  assign ff45 = 48'(t4 + t5);
  logic signed [47:0] ff_b;
  assign ff_b = 48'(48'(ff01 + ff23) + 48'(ff45 + t6));
  // Sum feedback.
  logic signed [47:0] fb12;
  assign fb12 = 48'(u1 + u2);
  logic signed [47:0] fb34;
  assign fb34 = 48'(u3 + u4);
  logic signed [47:0] fb56;
  assign fb56 = 48'(u5 + u6);
  logic signed [47:0] fb_a;
  assign fb_a = 48'(48'(fb12 + fb34) + fb56);
  // Final accumulator.
  logic signed [47:0] acc;
  assign acc = 48'(ff_b - fb_a);
  assign y = y_reg;
  always_ff @(posedge clk) begin
    if (rst) begin
      y_reg <= 0;
    end else begin
      y_reg <= 16'(acc);
    end
  end

endmodule

