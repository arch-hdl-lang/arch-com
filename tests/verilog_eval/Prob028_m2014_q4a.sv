// VerilogEval Prob028: D latch
module TopModule (
  input logic d,
  input logic ena,
  output logic q
);

  logic q_r;
  always_latch begin
    if (ena) begin
      q_r = d;
    end
  end
  assign q = q_r;

endmodule

