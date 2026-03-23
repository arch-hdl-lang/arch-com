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
        for (int i = 0; i <= 7; i++) begin
          q_r[i] <= d[i];
        end
      end
      if (byteena[1]) begin
        for (int i = 8; i <= 15; i++) begin
          q_r[i] <= d[i];
        end
      end
    end
  end
  assign q = q_r;

endmodule

