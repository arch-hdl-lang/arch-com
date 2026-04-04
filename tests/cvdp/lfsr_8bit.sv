module lfsr_8bit (
  input logic clock,
  input logic reset,
  input logic [8-1:0] lfsr_seed,
  output logic [8-1:0] lfsr_out
);

  logic [8-1:0] state;
  logic feedback;
  // Fibonacci LFSR: taps at bits 6,5,1,0 (0-indexed); right-shift
  assign feedback = state[6] ^ state[5] ^ state[1] ^ state[0];
  always_ff @(posedge clock or negedge reset) begin
    if ((!reset)) begin
      state <= 0;
    end else begin
      if (~reset) begin
        state <= lfsr_seed;
      end else begin
        state <= {feedback, state[7:1]};
      end
    end
  end
  assign lfsr_out = state;

endmodule

