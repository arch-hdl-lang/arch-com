// VerilogEval Prob145: p=a when clock high, p holds on negedge; q captures p on negedge
// domain SysDomain

module TopModule (
  input logic clock,
  input logic a,
  output logic p,
  output logic q
);

  logic p_r;
  logic q_r;
  always_ff @(negedge clock) begin
    p_r <= a;
    q_r <= a;
  end
  always_comb begin
    if (clock) begin
      p = a;
    end else begin
      p = p_r;
    end
    q = q_r;
  end

endmodule

