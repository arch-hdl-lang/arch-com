// VerilogEval Prob086: 5-bit Galois LFSR, taps at 5 and 3, sync reset to 1
module TopModule (
  input logic clk,
  input logic reset,
  output logic [5-1:0] q
);

  logic [5-1:0] q_r;
  logic [5-1:0] q_next;
  assign q_next[4] = q_r[0];
  assign q_next[3] = q_r[4];
  assign q_next[2] = q_r[3] ^ q_r[0];
  assign q_next[1] = q_r[2];
  assign q_next[0] = q_r[1];
  assign q = q_r;
  always_ff @(posedge clk) begin
    if (reset) begin
      q_r <= 1;
    end else begin
      q_r <= q_next;
    end
  end

endmodule

