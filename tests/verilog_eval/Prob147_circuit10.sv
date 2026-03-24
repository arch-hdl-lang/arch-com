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

  logic st;
  always_ff @(posedge clk) begin
    st <= ((a & b) | (st & (a ^ b)));
  end
  assign q = ((a ^ b) ^ st);
  assign state = st;

endmodule

