// VerilogEval Prob061: Shift register stage with load
// domain SysDomain

module TopModule (
  input logic clk,
  input logic w,
  input logic r_sig,
  input logic e,
  input logic l,
  output logic q_sig
);

  logic q_r;
  always_ff @(posedge clk) begin
    if (l) begin
      q_r <= r_sig;
    end else if (e) begin
      q_r <= w;
    end
  end
  assign q_sig = q_r;

endmodule

