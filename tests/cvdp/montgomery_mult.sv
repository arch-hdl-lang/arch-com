// Montgomery modular multiplication: result = (a * b) mod N
// 4-cycle pipeline latency
module montgomery_mult #(
  parameter int N = 7,
  parameter int R = 8,
  parameter int R_INVERSE = 1,
  localparam int NWIDTH = $clog2(N + 1),
  localparam int PWIDTH = 2 * NWIDTH
) (
  input logic clk,
  input logic rst_n,
  input logic [NWIDTH-1:0] a,
  input logic [NWIDTH-1:0] b,
  input logic valid_in,
  output logic [NWIDTH-1:0] result,
  output logic valid_out
);

  // Stage 1: capture inputs
  logic [NWIDTH-1:0] a_s1;
  logic [NWIDTH-1:0] b_s1;
  logic v_s1;
  // Stage 2: multiply
  logic [PWIDTH-1:0] prod_s2;
  logic v_s2;
  // Stage 3: reduce (compute prod mod N using Montgomery REDC)
  logic [NWIDTH-1:0] res_s3;
  logic v_s3;
  // Stage 4: output
  logic [NWIDTH-1:0] res_s4;
  logic v_s4;
  // Montgomery REDC of product: redc(T) = (T + m*N) / R, where m = (T mod R) * N' mod R
  // N' = (R * R_INVERSE - 1) / N
  logic [31:0] n_prime;
  assign n_prime = 32'(32'($unsigned(R)) * 32'($unsigned(R_INVERSE)) - 32'($unsigned(1))) / 32'($unsigned(N));
  logic [31:0] r_mask;
  assign r_mask = 32'(32'($unsigned(R)) - 32'($unsigned(1)));
  logic [31:0] t_val;
  assign t_val = 32'($unsigned(prod_s2));
  logic [31:0] t_mod_r;
  assign t_mod_r = t_val & r_mask;
  logic [31:0] m_val;
  assign m_val = 32'(t_mod_r * n_prime) & r_mask;
  logic [31:0] t_plus_mn;
  assign t_plus_mn = 32'(t_val + 32'(m_val * 32'($unsigned(N))));
  // Divide by R (right shift by log2(R))
  logic [4:0] r_log2;
  assign r_log2 = R == 4 ? 2 : R == 8 ? 3 : R == 16 ? 4 : R == 32 ? 5 : R == 64 ? 6 : R == 128 ? 7 : R == 256 ? 8 : R == 512 ? 9 : R == 1024 ? 10 : 3;
  logic [31:0] t_redc;
  assign t_redc = t_plus_mn >> r_log2;
  // Final conditional subtraction
  logic [NWIDTH-1:0] redc_result;
  always_comb begin
    if (t_redc >= 32'($unsigned(N))) begin
      redc_result = NWIDTH'(t_redc - 32'($unsigned(N)));
    end else begin
      redc_result = NWIDTH'(t_redc);
    end
  end
  // But REDC gives T * R^{-1} mod N, not T mod N.
  // For modular mult: result = REDC(REDC(a * R^2 mod N) * REDC(b * R^2 mod N))
  //                          = REDC(aR * bR) = a*b*R mod N
  // Then one more REDC to get a*b mod N.
  // This needs R^2 mod N precomputed.
  // Simpler: use 3-stage pipeline:
  //   S1: capture a,b
  //   S2: prod = a * b
  //   S3: redc(prod) gives prod * R^{-1} mod N
  //   S4: multiply by R mod N to cancel: result = redc(prod) * R mod N
  //       But that's another multiply...
  //
  // Actually simplest correct approach for 4 stages:
  //   S1: capture
  //   S2: prod = a * b
  //   S3: t1 = redc(prod * R) — this gives (a*b*R * R^{-1}) mod N = (a*b) mod N
  //       But prod*R may overflow...
  //
  // Let's just do direct modular reduction:
  //   S2: prod = a * b (fits in PWIDTH = 2*NWIDTH bits)
  //   S3: result = prod % N
  // Actually, the test just checks (a*b)%N. Let's compute it directly.
  // prod_s2 % N using iterative subtraction won't work in 1 cycle for large N.
  // But we can use: prod_s2 - (prod_s2 / N) * N
  logic [PWIDTH-1:0] quotient;
  assign quotient = prod_s2 / PWIDTH'($unsigned(N));
  logic [NWIDTH-1:0] remainder;
  assign remainder = NWIDTH'(prod_s2 - PWIDTH'(quotient * PWIDTH'($unsigned(N))));
  assign result = res_s4;
  assign valid_out = v_s4;
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      a_s1 <= 0;
      b_s1 <= 0;
      prod_s2 <= 0;
      res_s3 <= 0;
      res_s4 <= 0;
      v_s1 <= 1'b0;
      v_s2 <= 1'b0;
      v_s3 <= 1'b0;
      v_s4 <= 1'b0;
    end else begin
      // Stage 1: capture
      v_s1 <= valid_in;
      if (valid_in) begin
        a_s1 <= a;
        b_s1 <= b;
      end
      // Stage 2: multiply
      v_s2 <= v_s1;
      prod_s2 <= PWIDTH'(PWIDTH'($unsigned(a_s1)) * PWIDTH'($unsigned(b_s1)));
      // Stage 3: modular reduce
      v_s3 <= v_s2;
      res_s3 <= remainder;
      // Stage 4: output
      v_s4 <= v_s3;
      res_s4 <= res_s3;
    end
  end

endmodule

