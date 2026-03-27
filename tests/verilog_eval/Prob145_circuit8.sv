// VerilogEval Prob145: p is positive-level latch of a, q captures a on negedge
module TopModule (
  input logic clock,
  input logic a,
  output logic p,
  output logic q
);

  logic p_r;
  logic q_r;
  always_latch begin
    if (clock) begin
      p_r = a;
    end
  end
  always_ff @(negedge clock) begin
    q_r <= a;
  end
  assign p = p_r;
  assign q = q_r;

endmodule

