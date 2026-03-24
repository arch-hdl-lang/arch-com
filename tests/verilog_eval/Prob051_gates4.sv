module TopModule (
  input logic [4-1:0] in,
  output logic out_and,
  output logic out_or,
  output logic out_xor
);

  assign out_and = (((in[0] & in[1]) & in[2]) & in[3]);
  assign out_or = (((in[0] | in[1]) | in[2]) | in[3]);
  assign out_xor = (((in[0] ^ in[1]) ^ in[2]) ^ in[3]);

endmodule

