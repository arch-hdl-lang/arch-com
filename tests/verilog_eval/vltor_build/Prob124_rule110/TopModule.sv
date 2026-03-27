// domain SysDomain

module TopModule (
  input logic clk,
  input logic load,
  input logic [512-1:0] data,
  output logic [512-1:0] q
);

  logic [512-1:0] q_r;
  logic [512-1:0] q_next;
  logic left_i;
  logic center_i;
  logic right_i;
  always_comb begin
    left_i = q_r[1];
    center_i = q_r[0];
    right_i = 0;
    q_next[0] = ~(left_i & center_i & right_i | ~left_i & ~center_i & ~right_i | left_i & ~center_i & ~right_i);
    for (int i = 1; i <= 510; i++) begin
      left_i = q_r[i + 1];
      center_i = q_r[i];
      right_i = q_r[i - 1];
      q_next[i] = ~(left_i & center_i & right_i | ~left_i & ~center_i & ~right_i | left_i & ~center_i & ~right_i);
    end
    left_i = 0;
    center_i = q_r[511];
    right_i = q_r[510];
    q_next[511] = ~(left_i & center_i & right_i | ~left_i & ~center_i & ~right_i | left_i & ~center_i & ~right_i);
    q = q_r;
  end
  // Boundary: i=0 (right neighbor is 0)
  // Inner cells
  // Boundary: i=511 (left neighbor is 0)
  always_ff @(posedge clk) begin
    if (load) begin
      q_r <= data;
    end else begin
      q_r <= q_next;
    end
  end

endmodule

