Wrote tests/verilog_eval/Prob135_m2014_q6b.sv
input logic w,
  output logic Y1
);

  assign Y1 = y == 1 | y == 5 | w & (y == 2 | y == 4) ? 1 : 0;

endmodule

