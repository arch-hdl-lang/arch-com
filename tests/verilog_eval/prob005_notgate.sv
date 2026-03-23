// VerilogEval Prob005: NOT gate
module TopModule (
  input logic in_sig,
  output logic out_sig
);

  assign out_sig = (~in_sig);

endmodule

