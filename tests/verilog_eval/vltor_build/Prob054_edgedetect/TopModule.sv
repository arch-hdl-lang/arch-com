// VerilogEval Prob054: 8-bit positive edge detection (registered output)
module TopModule (
  input logic clk,
  input logic [8-1:0] in,
  output logic [8-1:0] pedge
);

  logic [8-1:0] prev;
  always_ff @(posedge clk) begin
    prev <= in;
    pedge <= in & ~prev;
  end

endmodule

