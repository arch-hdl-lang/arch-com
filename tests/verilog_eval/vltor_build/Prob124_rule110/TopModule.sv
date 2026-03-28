module TopModule (
  input logic clk,
  input logic load,
  input logic [512-1:0] data,
  output logic [512-1:0] q
);

  logic [512-1:0] q_next;
  logic left_i;
  logic center_i;
  logic right_i;
  always_comb begin
    left_i = q[1];
    center_i = q[0];
    right_i = 0;
    q_next[0] = ~(left_i & center_i & right_i | ~left_i & ~center_i & ~right_i | left_i & ~center_i & ~right_i);
    for (int i = 1; i <= 510; i++) begin
      left_i = q[i + 1];
      center_i = q[i];
      right_i = q[i - 1];
      q_next[i] = ~(left_i & center_i & right_i | ~left_i & ~center_i & ~right_i | left_i & ~center_i & ~right_i);
    end
    left_i = 0;
    center_i = q[511];
    right_i = q[510];
    q_next[511] = ~(left_i & center_i & right_i | ~left_i & ~center_i & ~right_i | left_i & ~center_i & ~right_i);
  end
  // Boundary: i=0 (right neighbor is 0)
  // Inner cells
  // Boundary: i=511 (left neighbor is 0)
  always_ff @(posedge clk) begin
    q <= load ? data : q_next;
  end

endmodule

