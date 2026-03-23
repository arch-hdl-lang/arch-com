// VerilogEval Prob016: 4-bit adder with 5-bit sum
module TopModule (
  input logic [4-1:0] x,
  input logic [4-1:0] y,
  output logic [5-1:0] sum
);

  assign sum = 5'((5'($unsigned(x)) + 5'($unsigned(y))));

endmodule

