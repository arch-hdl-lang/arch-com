Wrote tests/verilog_eval/Prob024_hadd.sv
 input logic a,
  input logic b,
  output logic sum,
  output logic cout
);

  assign sum = a ^ b;
  assign cout = a & b;

endmodule

