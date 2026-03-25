// VerilogEval Prob033: 8-bit signed add with overflow detection
module TopModule (
  input logic [8-1:0] a,
  input logic [8-1:0] b,
  output logic [8-1:0] s,
  output logic overflow
);

  logic [9-1:0] sum9;
  assign sum9 = 9'(9'($unsigned(a)) + 9'($unsigned(b)));
  assign s = 8'(sum9);
  assign overflow = a[7] == b[7] & sum9[7] != a[7];

endmodule

