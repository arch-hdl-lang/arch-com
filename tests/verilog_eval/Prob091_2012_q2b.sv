Wrote tests/verilog_eval/Prob091_2012_q2b.sv
 input logic w,
  output logic Y1,
  output logic Y3
);

  assign Y1 = y[0] & w;
  assign Y3 = y[1] & ~w | y[2] & ~w | y[4] & ~w | y[5] & ~w;

endmodule

