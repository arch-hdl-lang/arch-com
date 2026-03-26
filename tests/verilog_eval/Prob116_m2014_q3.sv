Wrote tests/verilog_eval/Prob116_m2014_q3.sv
 output logic f
);

  assign f = x[2] & ~x[0] | x[3] & ~x[2] & x[1] & x[0];

endmodule

