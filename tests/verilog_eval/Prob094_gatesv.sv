module TopModule (
  input logic [4-1:0] in_sig,
  output logic [4-1:0] out_both,
  output logic [4-1:0] out_any,
  output logic [4-1:0] out_different
);

  assign out_both[0] = (in_sig[0] & in_sig[1]);
  assign out_both[1] = (in_sig[1] & in_sig[2]);
  assign out_both[2] = (in_sig[2] & in_sig[3]);
  assign out_both[3] = 0;
  assign out_any[0] = 0;
  assign out_any[1] = (in_sig[1] | in_sig[0]);
  assign out_any[2] = (in_sig[2] | in_sig[1]);
  assign out_any[3] = (in_sig[3] | in_sig[2]);
  assign out_different[0] = (in_sig[0] ^ in_sig[1]);
  assign out_different[1] = (in_sig[1] ^ in_sig[2]);
  assign out_different[2] = (in_sig[2] ^ in_sig[3]);
  assign out_different[3] = (in_sig[3] ^ in_sig[0]);

endmodule

