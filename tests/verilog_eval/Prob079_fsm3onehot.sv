// VerilogEval Prob079: One-hot FSM combinational logic
module TopModule (
  input logic in_sig,
  input logic [4-1:0] state_sig,
  output logic [4-1:0] next_state,
  output logic out_sig
);

  logic ns0;
  logic ns1;
  logic ns2;
  logic ns3;
  assign ns0 = ((state_sig[0] & (~in_sig)) | (state_sig[2] & (~in_sig)));
  assign ns1 = (((state_sig[0] & in_sig) | (state_sig[1] & in_sig)) | (state_sig[3] & in_sig));
  assign ns2 = ((state_sig[1] & (~in_sig)) | (state_sig[3] & (~in_sig)));
  assign ns3 = (state_sig[2] & in_sig);
  assign next_state[0] = ns0;
  assign next_state[1] = ns1;
  assign next_state[2] = ns2;
  assign next_state[3] = ns3;
  assign out_sig = state_sig[3];

endmodule

