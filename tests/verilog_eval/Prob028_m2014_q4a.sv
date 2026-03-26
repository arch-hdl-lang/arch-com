Wrote tests/verilog_eval/Prob028_m2014_q4a.sv
le (
  input logic d,
  input logic ena,
  output logic q
);

  logic q_r;
  always_latch begin
    if (ena) begin
      q_r <= d;
    end
  end
  assign q = q_r;

endmodule

