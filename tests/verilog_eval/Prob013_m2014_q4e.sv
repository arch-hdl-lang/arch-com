module TopModule (
  input logic in1,
  input logic in2,
  output logic out_sig
);

  assign out_sig = (~(in1 | in2));

endmodule

