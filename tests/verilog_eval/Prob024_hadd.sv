// domain SysDomain

module TopModule (
  input logic a,
  input logic b,
  output logic sum_sig,
  output logic cout
);

  assign sum_sig = (a ^ b);
  assign cout = (a & b);

endmodule

