// VerilogEval Prob060: 4-stage shift register, active-low sync reset
// domain SysDomain

module TopModule (
  input logic clk,
  input logic resetn,
  input logic in,
  output logic out
);

  logic [4-1:0] sr = 0;
  always_ff @(posedge clk) begin
    if (~resetn) begin
      sr <= 0;
    end else begin
      sr <= 4'(sr << 1) | 4'($unsigned(in));
    end
  end
  assign out = sr[3:3];

endmodule

