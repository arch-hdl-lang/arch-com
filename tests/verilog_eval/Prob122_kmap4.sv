Wrote tests/verilog_eval/Prob122_kmap4.sv
input logic a,
  input logic b,
  input logic c,
  input logic d,
  output logic out
);

  assign out = a ^ b ^ c ^ d;

endmodule

