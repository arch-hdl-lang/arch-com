// VerilogEval Prob054: 8-bit positive edge detection (registered output)
// domain SysDomain

module TopModule (
  input logic clk,
  input logic [8-1:0] in,
  output logic [8-1:0] pedge
);

  logic [8-1:0] prev;
  logic [8-1:0] pedge_r;
  always_ff @(posedge clk) begin
    prev <= in;
    pedge_r <= in & ~prev;
  end
  assign pedge = pedge_r;

endmodule

