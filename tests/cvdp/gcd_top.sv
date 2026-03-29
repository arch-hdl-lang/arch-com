module gcd_top #(
  parameter int WIDTH = 4
) (
  input logic clk,
  input logic rst,
  input logic [WIDTH-1:0] A,
  input logic [WIDTH-1:0] B,
  input logic go,
  output logic [WIDTH-1:0] OUT,
  output logic done
);

  logic [2-1:0] cp_state;
  logic eq;
  logic gt;
  gcd_controlpath #(.WIDTH(WIDTH)) u_ctrl (
    .clk(clk),
    .rst(rst),
    .go(go),
    .equal(eq),
    .greater_than(gt),
    .controlpath_state(cp_state),
    .done(done)
  );
  gcd_datapath #(.WIDTH(WIDTH)) u_dp (
    .clk(clk),
    .rst(rst),
    .A(A),
    .B(B),
    .controlpath_state(cp_state),
    .OUT(OUT),
    .equal(eq),
    .greater_than(gt)
  );

endmodule

module gcd_datapath #(
  parameter int WIDTH = 4
) (
  input logic clk,
  input logic rst,
  input logic [WIDTH-1:0] A,
  input logic [WIDTH-1:0] B,
  input logic [2-1:0] controlpath_state,
  output logic [WIDTH-1:0] OUT,
  output logic equal,
  output logic greater_than
);

  logic [WIDTH-1:0] a_ff = 0;
  logic [WIDTH-1:0] b_ff = 0;
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
    OUT = a_ff;
  end
  always_ff @(posedge clk) begin
    if (rst) begin
      a_ff <= 0;
      b_ff <= 0;
    end else begin
      if (controlpath_state == 2'd0) begin
        a_ff <= A;
        b_ff <= B;
      end else if (controlpath_state == 2'd2) begin
        a_ff <= WIDTH'(a_ff - b_ff);
      end else if (controlpath_state == 2'd3) begin
        b_ff <= WIDTH'(b_ff - a_ff);
      end
    end
  end

endmodule

module gcd_controlpath #(
  parameter int WIDTH = 4
) (
  input logic clk,
  input logic rst,
  input logic go,
  input logic equal,
  input logic greater_than,
  output logic [2-1:0] controlpath_state,
  output logic done
);

  logic [2-1:0] state_r = 0;
  logic [2-1:0] next_state;
  always_comb begin
    if (state_r == 2'd0) begin
      if (go) begin
        if (equal) begin
          next_state = 2'd1;
        end else if (greater_than) begin
          next_state = 2'd2;
        end else begin
          next_state = 2'd3;
        end
      end else begin
        next_state = 2'd0;
      end
    end else if (state_r == 2'd1) begin
      next_state = 2'd0;
    end else if (equal) begin
      next_state = 2'd1;
    end else if (greater_than) begin
      next_state = 2'd2;
    end else begin
      next_state = 2'd3;
    end
    controlpath_state = state_r;
    done = state_r == 2'd1;
  end
  always_ff @(posedge clk) begin
    if (rst) begin
      state_r <= 0;
    end else begin
      state_r <= next_state;
    end
  end

endmodule

