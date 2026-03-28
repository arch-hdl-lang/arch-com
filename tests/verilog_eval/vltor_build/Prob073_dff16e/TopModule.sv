// VerilogEval Prob073: 16 DFFs with byte enable, active-low sync reset
module TopModule (
  input logic clk,
  input logic resetn,
  input logic [2-1:0] byteena,
  input logic [16-1:0] d,
  output logic [16-1:0] q
);

  always_ff @(posedge clk) begin
    if ((!resetn)) begin
      q <= 0;
    end else begin
      q[7:0] <= byteena[0] ? d[7:0] : q[7:0];
      q[15:8] <= byteena[1] ? d[15:8] : q[15:8];
    end
  end

endmodule

