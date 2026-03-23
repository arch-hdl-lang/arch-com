// VerilogEval Prob014: AND gate
module TopModule (
  input logic a,
  input logic b,
  output logic out_sig
);

  assign out_sig = (a & b);

endmodule

