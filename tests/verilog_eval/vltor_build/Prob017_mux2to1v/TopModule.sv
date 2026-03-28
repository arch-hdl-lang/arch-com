module TopModule (
  input logic [100-1:0] a,
  input logic [100-1:0] b,
  input logic sel,
  output logic [100-1:0] out
);

  assign out = sel ? b : a;

endmodule

