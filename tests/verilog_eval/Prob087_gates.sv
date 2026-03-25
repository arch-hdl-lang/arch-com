module TopModule (
  input logic a,
  input logic b,
  output logic out_and,
  output logic out_or,
  output logic out_xor,
  output logic out_nand,
  output logic out_nor,
  output logic out_xnor,
  output logic out_anotb
);

  assign out_and = a & b;
  assign out_or = a | b;
  assign out_xor = a ^ b;
  assign out_nand = ~(a & b);
  assign out_nor = ~(a | b);
  assign out_xnor = ~(a ^ b);
  assign out_anotb = a & ~b;

endmodule

