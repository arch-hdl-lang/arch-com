module TopModule (
  input logic [4-1:0] in_sig,
  output logic out_and,
  output logic out_or,
  output logic out_xor
);

  assign out_and = (((in_sig[0] & in_sig[1]) & in_sig[2]) & in_sig[3]);
  assign out_or = (((in_sig[0] | in_sig[1]) | in_sig[2]) | in_sig[3]);
  assign out_xor = (((in_sig[0] ^ in_sig[1]) ^ in_sig[2]) ^ in_sig[3]);

endmodule

