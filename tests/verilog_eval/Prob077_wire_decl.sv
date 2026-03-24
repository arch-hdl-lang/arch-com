module TopModule (
  input logic a,
  input logic b,
  input logic c,
  input logic d,
  output logic out,
  output logic out_n
);

  logic ab;
  logic cd;
  assign ab = (a & b);
  assign cd = (c & d);
  assign out = (ab | cd);
  assign out_n = (~(ab | cd));

endmodule

