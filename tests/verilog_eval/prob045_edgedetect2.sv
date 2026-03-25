// VerilogEval Prob045: 8-bit any-edge detect
// domain SysDomain

module TopModule (
  input logic clk,
  input logic [8-1:0] in,
  output logic [8-1:0] anyedge = 0
);

  logic [8-1:0] d_last = 0;
  always_ff @(posedge clk) begin
    d_last <= in;
    anyedge <= in ^ d_last;
  end

endmodule

