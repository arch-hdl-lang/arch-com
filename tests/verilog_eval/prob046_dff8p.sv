// VerilogEval Prob046: 8 DFFs, active high sync reset to 0x34, negedge clk
// domain SysDomain

module TopModule (
  input logic clk,
  input logic rst,
  input logic [8-1:0] d,
  output logic [8-1:0] q
);

  logic [8-1:0] q_r;
  always_ff @(negedge clk) begin
    if (rst) begin
      q_r <= 'h34;
    end else begin
      q_r <= d;
    end
  end
  assign q = q_r;

endmodule

