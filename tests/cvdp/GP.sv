module GP (
  input logic i_A,
  input logic i_B,
  input logic i_Cin,
  output logic o_generate,
  output logic o_propagate,
  output logic o_Cout
);

  assign o_generate = i_A & i_B;
  assign o_propagate = i_A | i_B;
  assign o_Cout = i_A & i_B | (i_A | i_B) & i_Cin;

endmodule

