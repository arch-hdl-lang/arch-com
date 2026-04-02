// 4-stage pipelined 32-bit adder using the pipeline construct.
// Each stage adds one 8-bit byte with carry from the previous stage.
// The pipeline auto-generates inter-stage registers and valid propagation.
module pipelined_adder_32bit (
  input logic clk,
  input logic reset,
  input logic [32-1:0] A,
  input logic [32-1:0] B,
  input logic start,
  output logic [32-1:0] S,
  output logic Co,
  output logic done
);

  // ── Stage valid registers ──
  logic add0_valid_r;
  logic add1_valid_r;
  logic add2_valid_r;
  logic add3_valid_r;
  
  // ── Stage data registers ──
  logic [8-1:0] add0_s0 = 0;
  logic add0_c0 = 1'b0;
  logic [8-1:0] add0_a1 = 0;
  logic [8-1:0] add0_b1 = 0;
  logic [8-1:0] add0_a2 = 0;
  logic [8-1:0] add0_b2 = 0;
  logic [8-1:0] add0_a3 = 0;
  logic [8-1:0] add0_b3 = 0;
  logic [9-1:0] add0_sum0;
  logic [8-1:0] add1_s1 = 0;
  logic add1_c1 = 1'b0;
  logic [8-1:0] add1_s0_d = 0;
  logic [8-1:0] add1_a2_d = 0;
  logic [8-1:0] add1_b2_d = 0;
  logic [8-1:0] add1_a3_d = 0;
  logic [8-1:0] add1_b3_d = 0;
  logic [9-1:0] add1_sum1;
  logic [8-1:0] add2_s2 = 0;
  logic add2_c2 = 1'b0;
  logic [8-1:0] add2_s0_dd = 0;
  logic [8-1:0] add2_s1_d = 0;
  logic [8-1:0] add2_a3_dd = 0;
  logic [8-1:0] add2_b3_dd = 0;
  logic [9-1:0] add2_sum2;
  logic [8-1:0] add3_s3 = 0;
  logic add3_c3 = 1'b0;
  logic [8-1:0] add3_s0_ddd = 0;
  logic [8-1:0] add3_s1_dd = 0;
  logic [8-1:0] add3_s2_d = 0;
  logic [9-1:0] add3_sum3;
  
  // ── Stage register updates ──
  always_ff @(posedge clk) begin
    if (reset) begin
      add0_valid_r <= 1'b0;
      add0_s0 <= 0;
      add0_c0 <= 1'b0;
      add0_a1 <= 0;
      add0_b1 <= 0;
      add0_a2 <= 0;
      add0_b2 <= 0;
      add0_a3 <= 0;
      add0_b3 <= 0;
      add1_valid_r <= 1'b0;
      add1_s1 <= 0;
      add1_c1 <= 1'b0;
      add1_s0_d <= 0;
      add1_a2_d <= 0;
      add1_b2_d <= 0;
      add1_a3_d <= 0;
      add1_b3_d <= 0;
      add2_valid_r <= 1'b0;
      add2_s2 <= 0;
      add2_c2 <= 1'b0;
      add2_s0_dd <= 0;
      add2_s1_d <= 0;
      add2_a3_dd <= 0;
      add2_b3_dd <= 0;
      add3_valid_r <= 1'b0;
      add3_s3 <= 0;
      add3_c3 <= 1'b0;
      add3_s0_ddd <= 0;
      add3_s1_dd <= 0;
      add3_s2_d <= 0;
    end else begin
      add0_valid_r <= 1'b1;
      add0_valid_r <= start;
      add0_s0 <= 8'(add0_sum0);
      add0_c0 <= add0_sum0[8];
      add0_a1 <= A[15:8];
      add0_b1 <= B[15:8];
      add0_a2 <= A[23:16];
      add0_b2 <= B[23:16];
      add0_a3 <= A[31:24];
      add0_b3 <= B[31:24];
      add1_valid_r <= add0_valid_r;
      add1_s1 <= 8'(add1_sum1);
      add1_c1 <= add1_sum1[8];
      add1_s0_d <= add0_s0;
      add1_a2_d <= add0_a2;
      add1_b2_d <= add0_b2;
      add1_a3_d <= add0_a3;
      add1_b3_d <= add0_b3;
      add2_valid_r <= add1_valid_r;
      add2_s2 <= 8'(add2_sum2);
      add2_c2 <= add2_sum2[8];
      add2_s0_dd <= add1_s0_d;
      add2_s1_d <= add1_s1;
      add2_a3_dd <= add1_a3_d;
      add2_b3_dd <= add1_b3_d;
      add3_valid_r <= add2_valid_r;
      add3_s3 <= 8'(add3_sum3);
      add3_c3 <= add3_sum3[8];
      add3_s0_ddd <= add2_s0_dd;
      add3_s1_dd <= add2_s1_d;
      add3_s2_d <= add2_s2;
    end
  end
  
  // ── Combinational outputs ──
  assign add0_sum0 = (9'(A[7:0]) + 9'(B[7:0]));
  assign add1_sum1 = ((9'(add0_a1) + 9'(add0_b1)) + 9'(add0_c0));
  assign add2_sum2 = ((9'(add1_a2_d) + 9'(add1_b2_d)) + 9'(add1_c1));
  assign add3_sum3 = ((9'(add2_a3_dd) + 9'(add2_b3_dd)) + 9'(add2_c2));
  assign S = {add3_s3, add3_s2_d, add3_s1_dd, add3_s0_ddd};
  assign Co = add3_c3;
  assign done = add3_valid_r;

endmodule

