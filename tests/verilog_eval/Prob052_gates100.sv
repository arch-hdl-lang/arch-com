Wrote tests/verilog_eval/Prob052_gates100.sv
,
  output logic out_and,
  output logic out_or,
  output logic out_xor
);

  assign out_and = &in;
  assign out_or = |in;
  assign out_xor = ^in;

endmodule

