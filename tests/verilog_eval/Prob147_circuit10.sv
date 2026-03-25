// VerilogEval Prob147: Sequential circuit from waveform
// next_state = (a & b) | (state & (a ^ b))
// q = a ^ b ^ state
// domain SysDomain

module TopModule (
  input logic clk,
  input logic a,
  input logic b,
  output logic state,
  output logic q
);

  always_ff @(posedge clk) begin
    state <= a & b | state & (a ^ b);
  end
  assign q = a ^ b ^ state;

endmodule

