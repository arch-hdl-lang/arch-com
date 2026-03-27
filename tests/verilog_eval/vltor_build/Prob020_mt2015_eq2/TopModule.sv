module TopModule (
  input logic [2-1:0] A,
  input logic [2-1:0] B,
  output logic z
);

  assign z = A == B;

endmodule

