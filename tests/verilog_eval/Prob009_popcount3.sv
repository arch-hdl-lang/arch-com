// domain SysDomain

module TopModule (
  input logic [3-1:0] in_sig,
  output logic [2-1:0] out_sig
);

  assign out_sig = 2'(((2'($unsigned(in_sig[0])) + 2'($unsigned(in_sig[1]))) + 2'($unsigned(in_sig[2]))));

endmodule

