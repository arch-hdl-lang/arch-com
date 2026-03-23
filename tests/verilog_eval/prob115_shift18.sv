// VerilogEval Prob115: 64-bit arithmetic shift register
// domain SysDomain

module TopModule (
  input logic clk,
  input logic load,
  input logic ena,
  input logic [2-1:0] amount,
  input logic [64-1:0] data,
  output logic [64-1:0] q
);

  logic [64-1:0] q_r = 0;
  logic [64-1:0] sra1;
  logic [64-1:0] sra8;
  assign sra1 = {q_r[63], 63'((q_r >> 1))};
  assign sra8 = {{8{q_r[63]}}, 56'((q_r >> 8))};
  assign q = q_r;
  always_ff @(posedge clk) begin
    if (load) begin
      q_r <= data;
    end else if (ena) begin
      if ((amount == 0)) begin
        q_r <= 64'((q_r << 1));
      end else if ((amount == 1)) begin
        q_r <= 64'((q_r << 8));
      end else if ((amount == 2)) begin
        q_r <= sra1;
      end else begin
        q_r <= sra8;
      end
    end
  end

endmodule

