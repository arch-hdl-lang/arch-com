// VerilogEval Prob061: Shift register stage with load
// domain SysDomain

module TopModule (
  input logic clk,
  input logic w,
  input logic R,
  input logic E,
  input logic L,
  output logic Q
);

  logic q_r;
  always_ff @(posedge clk) begin
    if (L) begin
      q_r <= R;
    end else if (E) begin
      q_r <= w;
    end
  end
  assign Q = q_r;

endmodule

