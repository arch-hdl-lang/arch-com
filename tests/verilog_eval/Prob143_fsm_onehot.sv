module TopModule (
  input logic in_sig,
  input logic [10-1:0] state_sig,
  output logic [10-1:0] next_state,
  output logic out1,
  output logic out2
);

  assign next_state[0] = ((~in_sig) & (((((((state_sig[0] | state_sig[1]) | state_sig[2]) | state_sig[3]) | state_sig[4]) | state_sig[7]) | state_sig[8]) | state_sig[9]));
  assign next_state[1] = (in_sig & ((state_sig[0] | state_sig[8]) | state_sig[9]));
  assign next_state[2] = (in_sig & state_sig[1]);
  assign next_state[3] = (in_sig & state_sig[2]);
  assign next_state[4] = (in_sig & state_sig[3]);
  assign next_state[5] = (in_sig & state_sig[4]);
  assign next_state[6] = (in_sig & state_sig[5]);
  assign next_state[7] = (in_sig & (state_sig[6] | state_sig[7]));
  assign next_state[8] = ((~in_sig) & state_sig[5]);
  assign next_state[9] = ((~in_sig) & state_sig[6]);
  assign out1 = (state_sig[8] | state_sig[9]);
  assign out2 = (state_sig[7] | state_sig[9]);

endmodule

