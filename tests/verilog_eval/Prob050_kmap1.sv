module TopModule (
  input logic a,
  input logic b,
  input logic c,
  output logic out_sig
);

  assign out_sig = ((a | b) | c);

endmodule

