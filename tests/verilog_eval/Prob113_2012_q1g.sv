Wrote tests/verilog_eval/Prob113_2012_q1g.sv
 output logic f
);

  assign f = ~x[3] & ~x[1] | x[2] & ~x[0] & (x[1] | x[3]) | x[2] & x[3] & x[1];

endmodule

