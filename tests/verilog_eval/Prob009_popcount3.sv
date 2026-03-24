// domain SysDomain

module TopModule (
  input logic [3-1:0] in,
  output logic [2-1:0] out
);

  assign out = 2'(((2'($unsigned(in[0])) + 2'($unsigned(in[1]))) + 2'($unsigned(in[2]))));

endmodule

