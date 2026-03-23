module TopModule (
  input logic a,
  input logic b,
  input logic c,
  input logic d,
  output logic out_sig
);

  assign out_sig = ((((~b) & c) | (a & (~d))) | (a & c));

endmodule

