module TopModule (
  input logic a,
  input logic b,
  input logic sel,
  output logic out
);

  assign out = sel == 1'd1 ? b : a;

endmodule

