module GP (
  input logic i_A,
  input logic i_B,
  input logic i_Cin,
  output logic o_generate,
  output logic o_propagate,
  output logic o_Cout
);

  logic gen;
  logic prop;
  assign gen = i_A & i_B;
  assign prop = i_A | i_B;
  assign o_generate = gen;
  assign o_propagate = prop;
  assign o_Cout = gen | prop & i_Cin;

endmodule

