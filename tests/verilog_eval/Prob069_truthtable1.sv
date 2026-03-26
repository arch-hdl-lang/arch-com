Wrote tests/verilog_eval/Prob069_truthtable1.sv
ic x2,
  input logic x1,
  output logic f
);

  assign f = x2 & ~x3 | x3 & x1;

endmodule

