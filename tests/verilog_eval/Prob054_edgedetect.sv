// VerilogEval Prob054: 8-bit positive edge detection (registered output)
// domain SysDomain

module TopModule (
  input logic clk,
  input logic [8-1:0] in_sig,
  output logic [8-1:0] pedge
);

  logic [8-1:0] prev;
  logic [8-1:0] pedge_r;
  always_ff @(posedge clk) begin
    prev <= in_sig;
    pedge_r <= (in_sig & (~prev));
  end
  assign pedge = pedge_r;

endmodule

