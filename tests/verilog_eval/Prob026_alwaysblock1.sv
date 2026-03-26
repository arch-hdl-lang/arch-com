Wrote tests/verilog_eval/Prob026_alwaysblock1.sv
 b,
  output logic out_assign,
  output logic out_alwaysblock
);

  assign out_assign = a & b;
  assign out_alwaysblock = a & b;

endmodule

