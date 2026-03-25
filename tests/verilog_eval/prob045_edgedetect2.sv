// VerilogEval Prob045: 8-bit any-edge detect
// domain SysDomain

module TopModule (
  input logic clk,
  input logic [8-1:0] in,
  output logic [8-1:0] anyedge
);

  logic [8-1:0] d_last = 0;
  logic [8-1:0] anyedge_r = 0;
  always_ff @(posedge clk) begin
    d_last <= in;
    anyedge_r <= in ^ d_last;
  end
  assign anyedge = anyedge_r;

endmodule

