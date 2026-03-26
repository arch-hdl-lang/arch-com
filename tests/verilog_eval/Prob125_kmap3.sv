Wrote tests/verilog_eval/Prob125_kmap3.sv
t logic b,
  input logic c,
  input logic d,
  output logic out
);

  assign out = ~b & c | a & ~d | a & c;

endmodule

