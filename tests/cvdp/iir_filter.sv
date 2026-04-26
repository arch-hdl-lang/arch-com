module iir_filter #(
  parameter logic signed [(7)*(16)-1:0] b = {(16)'(0), (16)'(0), (16)'(0), (16)'(0), (16)'(0), (16)'(0), (16)'(1)},
  parameter logic signed [(7)*(16)-1:0] a = {(16)'(0), (16)'(0), (16)'(0), (16)'(0), (16)'(0), (16)'(0), (16)'(0)}
) (
  input logic clk,
  input logic rst,
  input logic signed [15:0] x,
  output logic signed [15:0] y
);

  // Feedforward (b) coefficients, indexed b[0..6].
  // Feedback (a) coefficients, indexed a[1..6]; a[0] is unused.
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
  // from K cycles ago (y_reg = y at lag 0, y_pipe@1 = lag 1, …).
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
  // Per-tap multiply-accumulate. The K-th input tap multiplies b[K];
  // the K-th output tap (lag K, with lag 0 = y_reg) multiplies a[K].
  logic signed [6:0] [47:0] t;
  logic signed [6:0] [47:0] u;
  assign t[0] = 48'({{(48-$bits(x)){x[$bits(x)-1]}}, x} * {{(48-$bits(b[(0) * (16) +: (16)])){b[(0) * (16) +: (16)][$bits(b[(0) * (16) +: (16)])-1]}}, b[(0) * (16) +: (16)]});
  assign t[1] = 48'({{(48-$bits(x_pipe_stg1)){x_pipe_stg1[$bits(x_pipe_stg1)-1]}}, x_pipe_stg1} * {{(48-$bits(b[(1) * (16) +: (16)])){b[(1) * (16) +: (16)][$bits(b[(1) * (16) +: (16)])-1]}}, b[(1) * (16) +: (16)]});
  assign t[2] = 48'({{(48-$bits(x_pipe_stg2)){x_pipe_stg2[$bits(x_pipe_stg2)-1]}}, x_pipe_stg2} * {{(48-$bits(b[(2) * (16) +: (16)])){b[(2) * (16) +: (16)][$bits(b[(2) * (16) +: (16)])-1]}}, b[(2) * (16) +: (16)]});
  assign t[3] = 48'({{(48-$bits(x_pipe_stg3)){x_pipe_stg3[$bits(x_pipe_stg3)-1]}}, x_pipe_stg3} * {{(48-$bits(b[(3) * (16) +: (16)])){b[(3) * (16) +: (16)][$bits(b[(3) * (16) +: (16)])-1]}}, b[(3) * (16) +: (16)]});
  assign t[4] = 48'({{(48-$bits(x_pipe_stg4)){x_pipe_stg4[$bits(x_pipe_stg4)-1]}}, x_pipe_stg4} * {{(48-$bits(b[(4) * (16) +: (16)])){b[(4) * (16) +: (16)][$bits(b[(4) * (16) +: (16)])-1]}}, b[(4) * (16) +: (16)]});
  assign t[5] = 48'({{(48-$bits(x_pipe_stg5)){x_pipe_stg5[$bits(x_pipe_stg5)-1]}}, x_pipe_stg5} * {{(48-$bits(b[(5) * (16) +: (16)])){b[(5) * (16) +: (16)][$bits(b[(5) * (16) +: (16)])-1]}}, b[(5) * (16) +: (16)]});
  assign t[6] = 48'({{(48-$bits(x_pipe)){x_pipe[$bits(x_pipe)-1]}}, x_pipe} * {{(48-$bits(b[(6) * (16) +: (16)])){b[(6) * (16) +: (16)][$bits(b[(6) * (16) +: (16)])-1]}}, b[(6) * (16) +: (16)]});
  assign u[0] = $signed(48'($unsigned(0)));
  assign u[1] = 48'({{(48-$bits(y_reg)){y_reg[$bits(y_reg)-1]}}, y_reg} * {{(48-$bits(a[(1) * (16) +: (16)])){a[(1) * (16) +: (16)][$bits(a[(1) * (16) +: (16)])-1]}}, a[(1) * (16) +: (16)]});
  assign u[2] = 48'({{(48-$bits(y_pipe_stg1)){y_pipe_stg1[$bits(y_pipe_stg1)-1]}}, y_pipe_stg1} * {{(48-$bits(a[(2) * (16) +: (16)])){a[(2) * (16) +: (16)][$bits(a[(2) * (16) +: (16)])-1]}}, a[(2) * (16) +: (16)]});
  assign u[3] = 48'({{(48-$bits(y_pipe_stg2)){y_pipe_stg2[$bits(y_pipe_stg2)-1]}}, y_pipe_stg2} * {{(48-$bits(a[(3) * (16) +: (16)])){a[(3) * (16) +: (16)][$bits(a[(3) * (16) +: (16)])-1]}}, a[(3) * (16) +: (16)]});
  assign u[4] = 48'({{(48-$bits(y_pipe_stg3)){y_pipe_stg3[$bits(y_pipe_stg3)-1]}}, y_pipe_stg3} * {{(48-$bits(a[(4) * (16) +: (16)])){a[(4) * (16) +: (16)][$bits(a[(4) * (16) +: (16)])-1]}}, a[(4) * (16) +: (16)]});
  assign u[5] = 48'({{(48-$bits(y_pipe_stg4)){y_pipe_stg4[$bits(y_pipe_stg4)-1]}}, y_pipe_stg4} * {{(48-$bits(a[(5) * (16) +: (16)])){a[(5) * (16) +: (16)][$bits(a[(5) * (16) +: (16)])-1]}}, a[(5) * (16) +: (16)]});
  assign u[6] = 48'({{(48-$bits(y_pipe)){y_pipe[$bits(y_pipe)-1]}}, y_pipe} * {{(48-$bits(a[(6) * (16) +: (16)])){a[(6) * (16) +: (16)][$bits(a[(6) * (16) +: (16)])-1]}}, a[(6) * (16) +: (16)]});
  // a[0] unused; pad so indices align with b
  // Sum trees + final accumulator.
  logic signed [47:0] ff_b;
  assign ff_b = 48'(48'(48'(t[0] + t[1]) + 48'(t[2] + t[3])) + 48'(48'(t[4] + t[5]) + t[6]));
  logic signed [47:0] fb_a;
  assign fb_a = 48'(48'(48'(u[1] + u[2]) + 48'(u[3] + u[4])) + 48'(u[5] + u[6]));
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

