module TopModule (
  input logic in1,
  input logic in2,
  input logic in3,
  output logic out_sig
);

  assign out_sig = ((~(in1 ^ in2)) ^ in3);

endmodule

