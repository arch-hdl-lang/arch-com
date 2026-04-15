module gcd_datapath #(
  parameter int WIDTH = 4
) (
  input logic clk,
  input logic rst,
  input logic [WIDTH-1:0] A,
  input logic [WIDTH-1:0] B,
  input logic [1:0] controlpath_state,
  output logic [WIDTH-1:0] OUT,
  output logic equal,
  output logic greater_than
);

  logic [WIDTH-1:0] a_ff = 0;
  logic [WIDTH-1:0] b_ff = 0;
  logic [WIDTH-1:0] out_r = 0;
  logic [WIDTH-1:0] cmp_a;
  logic [WIDTH-1:0] cmp_b;
  always_comb begin
    if (controlpath_state == 2'd0) begin
      cmp_a = A;
      cmp_b = B;
    end else begin
      cmp_a = a_ff;
      cmp_b = b_ff;
    end
    equal = cmp_a == cmp_b;
    greater_than = cmp_a > cmp_b;
    OUT = out_r;
  end
  always_ff @(posedge clk) begin
    if (rst) begin
      a_ff <= 0;
      b_ff <= 0;
      out_r <= 0;
    end else begin
      if (controlpath_state == 2'd0) begin
        a_ff <= A;
        b_ff <= B;
      end else if (controlpath_state == 2'd1) begin
        out_r <= a_ff;
      end else if (controlpath_state == 2'd2) begin
        if (greater_than) begin
          a_ff <= WIDTH'(a_ff - b_ff);
        end
      end else if (~equal & ~greater_than) begin
        b_ff <= WIDTH'(b_ff - a_ff);
      end
    end
  end

endmodule

