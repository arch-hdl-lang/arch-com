module TopModule (
  input logic clk,
  input logic reset,
  output logic [32-1:0] q
);

  logic [32-1:0] q_next;
  always_comb begin
    q_next[31] = q[0];
    for (int i = 1; i <= 31; i++) begin
      q_next[i] = q[i + 1];
    end
    q_next[21] = q[22] ^ q[0];
    q_next[1] = q[2] ^ q[0];
    q_next[0] = q[1] ^ q[0];
  end
  always_ff @(posedge clk) begin
    if (reset) begin
      q <= 1;
    end else begin
      q <= q_next;
    end
  end

endmodule

