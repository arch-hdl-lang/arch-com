module TopModule (
  input logic a,
  input logic b,
  output logic out_assign,
  output logic out_alwaysblock
);

  assign out_assign = (a & b);
  assign out_alwaysblock = (a & b);

endmodule

