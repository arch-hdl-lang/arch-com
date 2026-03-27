module TopModule (
  input logic clk,
  input logic load,
  input logic [512-1:0] data,
  output logic [512-1:0] q
);

  logic [512-1:0] q_next;
  always_comb begin
    q_next[0] = q[1];
    for (int i = 1; i <= 510; i++) begin
      q_next[i] = q[i - 1] ^ q[i + 1];
    end
    q_next[511] = q[510];
  end
  always_ff @(posedge clk) begin
    q <= load ? data : q_next;
  end

endmodule

