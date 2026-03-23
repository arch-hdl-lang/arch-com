module TopModule (
  input logic [8-1:0] in_sig,
  output logic parity
);

  assign parity = (((((((in_sig[0] ^ in_sig[1]) ^ in_sig[2]) ^ in_sig[3]) ^ in_sig[4]) ^ in_sig[5]) ^ in_sig[6]) ^ in_sig[7]);

endmodule

