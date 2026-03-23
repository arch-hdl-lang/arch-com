module TopModule (
  input logic a,
  input logic b,
  input logic c,
  input logic d,
  input logic e,
  output logic [25-1:0] out_sig
);

  assign out_sig[24] = ((~a) ^ a);
  assign out_sig[23] = ((~a) ^ b);
  assign out_sig[22] = ((~a) ^ c);
  assign out_sig[21] = ((~a) ^ d);
  assign out_sig[20] = ((~a) ^ e);
  assign out_sig[19] = ((~b) ^ a);
  assign out_sig[18] = ((~b) ^ b);
  assign out_sig[17] = ((~b) ^ c);
  assign out_sig[16] = ((~b) ^ d);
  assign out_sig[15] = ((~b) ^ e);
  assign out_sig[14] = ((~c) ^ a);
  assign out_sig[13] = ((~c) ^ b);
  assign out_sig[12] = ((~c) ^ c);
  assign out_sig[11] = ((~c) ^ d);
  assign out_sig[10] = ((~c) ^ e);
  assign out_sig[9] = ((~d) ^ a);
  assign out_sig[8] = ((~d) ^ b);
  assign out_sig[7] = ((~d) ^ c);
  assign out_sig[6] = ((~d) ^ d);
  assign out_sig[5] = ((~d) ^ e);
  assign out_sig[4] = ((~e) ^ a);
  assign out_sig[3] = ((~e) ^ b);
  assign out_sig[2] = ((~e) ^ c);
  assign out_sig[1] = ((~e) ^ d);
  assign out_sig[0] = ((~e) ^ e);

endmodule

