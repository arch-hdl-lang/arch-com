module TopModule (
  input logic [4-1:0] x,
  output logic f
);

  assign f = x[2] & ~x[0] | x[3] & ~x[2] & x[1] & x[0];

endmodule

