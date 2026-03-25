// VerilogEval Prob046: 8 DFFs, active high sync reset to 0x34, negedge clk
// domain SysDomain

module TopModule (
  input logic clk,
  input logic reset,
  input logic [8-1:0] d,
  output logic [8-1:0] q
);

  always_ff @(negedge clk) begin
    if (reset) begin
      q <= 'h34;
    end else begin
      q <= d;
    end
  end

endmodule

