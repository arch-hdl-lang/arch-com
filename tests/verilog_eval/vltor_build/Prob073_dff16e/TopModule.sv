// VerilogEval Prob073: 16 DFFs with byte enable, active-low sync reset
// domain SysDomain

module TopModule (
  input logic clk,
  input logic resetn,
  input logic [2-1:0] byteena,
  input logic [16-1:0] d,
  output logic [16-1:0] q
);

  logic [16-1:0] q_r;
  always_ff @(posedge clk) begin
    if ((!resetn)) begin
      q_r <= 0;
    end else begin
      if (byteena[0]) begin
        q_r[7:0] <= d[7:0];
      end
      if (byteena[1]) begin
        q_r[15:8] <= d[15:8];
      end
    end
  end
  assign q = q_r;

endmodule

