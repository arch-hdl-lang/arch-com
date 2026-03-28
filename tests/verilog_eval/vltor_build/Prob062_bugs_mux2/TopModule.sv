module TopModule (
  input logic sel,
  input logic [8-1:0] a,
  input logic [8-1:0] b,
  output logic [8-1:0] out
);

  assign out = sel ? a : b;

endmodule

