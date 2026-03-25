// domain SysDomain

module TopModule (
  input logic a,
  input logic b,
  input logic cin,
  output logic sum,
  output logic cout
);

  assign sum = a ^ b ^ cin;
  assign cout = a & b | a & cin | b & cin;

endmodule

