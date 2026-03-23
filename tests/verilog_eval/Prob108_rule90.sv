// domain SysDomain

module TopModule (
  input logic clk,
  input logic load,
  input logic [512-1:0] data,
  output logic [512-1:0] q
);

  logic [512-1:0] q_r;
  logic [512-1:0] q_next;
  always_comb begin
    q_next[0] = q_r[1];
    for (int i = 1; i <= 510; i++) begin
      q_next[i] = (q_r[(i - 1)] ^ q_r[(i + 1)]);
    end
    q_next[511] = q_r[510];
    q = q_r;
  end
  always_ff @(posedge clk) begin
    if (load) begin
      q_r <= data;
    end else begin
      q_r <= q_next;
    end
  end

endmodule

