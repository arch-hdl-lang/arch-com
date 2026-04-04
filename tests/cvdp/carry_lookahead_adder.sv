module carry_lookahead_adder #(
  parameter int WIDTH = 8
) (
  input logic clk,
  input logic reset,
  input logic [WIDTH-1:0] A,
  input logic [WIDTH-1:0] B,
  input logic Cin,
  output logic [WIDTH-1:0] S,
  output logic carry
);

  logic g [WIDTH-1:0];
  logic p [WIDTH-1:0];
  logic c [WIDTH + 1-1:0];
  logic [WIDTH-1:0] S_next;
  logic carry_next;
  logic [WIDTH-1:0] S_r;
  logic carry_r;
  always_comb begin
    for (int i = 0; i <= WIDTH - 1; i++) begin
      g[i] = A[i +: 1] != 0 & B[i +: 1] != 0;
      p[i] = A[i +: 1] != 0 ^ B[i +: 1] != 0;
    end
    c[0] = Cin;
    for (int i = 0; i <= WIDTH - 1; i++) begin
      c[i + 1] = g[i] | p[i] & c[i];
    end
    for (int i = 0; i <= WIDTH - 1; i++) begin
      S_next[i] = p[i] ^ c[i];
    end
    carry_next = c[WIDTH];
  end
  always_ff @(posedge clk) begin
    if (reset) begin
      S_r <= 0;
      carry_r <= 1'b0;
    end else begin
      S_r <= S_next;
      carry_r <= carry_next;
    end
  end
  assign S = S_r;
  assign carry = carry_r;

endmodule

