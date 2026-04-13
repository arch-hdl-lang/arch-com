module pseudoRandGenerator_ca (
  input logic clock,
  input logic reset,
  input logic [15:0] CA_seed,
  output logic [15:0] CA_out
);

  // state has no auto-reset; we handle reset manually in seq
  logic [15:0] state;
  // Rule 90: next[i] = state[i-1] ^ state[i+1]
  // Rule 150: next[i] = state[i-1] ^ state[i] ^ state[i+1]
  // rule_mask bit i=1 means Rule 150, bit i=0 means Rule 90
  // Using alternating pattern for good randomness
  logic [15:0] rule_mask;
  assign rule_mask = 'hAAAA;
  logic [15:0] next_state;
  logic [15:0] left_shifted;
  logic [15:0] right_shifted;
  logic [15:0] r90;
  logic [16:0] state_shifted_left;
  assign left_shifted = state >> 1 | 16'($unsigned(state[15:15])) << 15;
  assign state_shifted_left = 17'($unsigned(state)) << 1;
  assign right_shifted = state_shifted_left[15:0] | 16'($unsigned(state[0:0]));
  assign r90 = left_shifted ^ right_shifted;
  assign next_state = r90 ^ state & rule_mask;
  assign CA_out = state;
  // left_shifted[i] = state[i-1], with wrap: state[-1] = state[15]
  // right_shifted[i] = state[i+1], with wrap: state[16] = state[0]
  // Rule 90 base: left XOR right
  // Apply Rule 150 where rule_mask bit is 1: XOR in current state bit
  always_ff @(posedge clock) begin
    if (reset) begin
      state <= CA_seed;
    end else begin
      state <= next_state;
    end
  end

endmodule

