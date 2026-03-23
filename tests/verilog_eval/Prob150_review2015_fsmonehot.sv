// VerilogEval Prob150: One-hot FSM combinational logic
// state[9:0]: S=0, S1=1, S11=2, S110=3, B0=4, B1=5, B2=6, B3=7, Count=8, Wait=9
module TopModule (
  input logic d,
  input logic done_counting,
  input logic ack,
  input logic [10-1:0] state_sig,
  output logic b3_next,
  output logic s_next,
  output logic s1_next,
  output logic count_next,
  output logic wait_next,
  output logic done,
  output logic counting,
  output logic shift_ena
);

  assign s_next = ((((state_sig[0] & (~d)) | (state_sig[1] & (~d))) | (state_sig[3] & (~d))) | (state_sig[9] & ack));
  assign s1_next = (state_sig[0] & d);
  assign b3_next = state_sig[6];
  assign count_next = (state_sig[7] | (state_sig[8] & (~done_counting)));
  assign wait_next = ((state_sig[8] & done_counting) | (state_sig[9] & (~ack)));
  assign shift_ena = (((state_sig[4] | state_sig[5]) | state_sig[6]) | state_sig[7]);
  assign counting = state_sig[8];
  assign done = state_sig[9];

endmodule

