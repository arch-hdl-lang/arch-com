// VerilogEval Prob060: 4-stage shift register, active-low sync reset
module TopModule (
  input logic clk,
  input logic resetn,
  input logic in,
  output logic out
);

  logic [4-1:0] sr = 0;
  always_ff @(posedge clk) begin
    sr <= ~resetn ? 0 : 4'(sr << 1) | 4'($unsigned(in));
  end
  assign out = sr[3:3];

endmodule

