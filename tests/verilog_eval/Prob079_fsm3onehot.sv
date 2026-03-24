// VerilogEval Prob079: One-hot FSM combinational logic
module TopModule (
  input logic in,
  input logic [4-1:0] state,
  output logic [4-1:0] next_state,
  output logic out
);

  logic ns0;
  logic ns1;
  logic ns2;
  logic ns3;
  assign ns0 = ((state[0] & (~in)) | (state[2] & (~in)));
  assign ns1 = (((state[0] & in) | (state[1] & in)) | (state[3] & in));
  assign ns2 = ((state[1] & (~in)) | (state[3] & (~in)));
  assign ns3 = (state[2] & in);
  assign next_state = {ns3, ns2, ns1, ns0};
  assign out = state[3];

endmodule

