module TopModule (
  input logic x3,
  input logic x2,
  input logic x1,
  output logic f
);

  assign f = ((x2 & (~x3)) | (x3 & x1));

endmodule

