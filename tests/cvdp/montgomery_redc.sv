// Montgomery reduction: result = T * R^{-1} mod N
// Combinational implementation
module montgomery_redc #(
  parameter int N = 7,
  parameter int R = 8,
  parameter int R_INVERSE = 1,
  localparam int NWIDTH = $clog2(N + 1),
  localparam int TWIDTH = $clog2(N * R)
) (
  input logic [TWIDTH-1:0] T,
  output logic [NWIDTH-1:0] result
);

  // N_PRIME = (R * R_INVERSE - 1) / N
  logic [32-1:0] n_prime_wide;
  assign n_prime_wide = 32'(32'($unsigned(R)) * 32'($unsigned(R_INVERSE)) - 32'($unsigned(1))) / 32'($unsigned(N));
  // T_mod_R = T & (R-1) since R is a power of 2
  logic [32-1:0] r_mask;
  assign r_mask = 32'(32'($unsigned(R)) - 32'($unsigned(1)));
  logic [32-1:0] t_mod_r;
  assign t_mod_r = 32'($unsigned(T)) & r_mask;
  // m = (T_mod_R * N_PRIME) mod R
  logic [32-1:0] t_x_np;
  assign t_x_np = 32'(t_mod_r * n_prime_wide);
  logic [32-1:0] m_wide;
  assign m_wide = t_x_np & r_mask;
  // t_redc = (T + m * N) / R
  logic [32-1:0] m_x_n;
  assign m_x_n = 32'(m_wide * 32'($unsigned(N)));
  logic [32-1:0] t_plus_mn;
  assign t_plus_mn = 32'(32'($unsigned(T)) + m_x_n);
  // Divide by R (right shift by log2(R))
  logic [5-1:0] r_log2;
  assign r_log2 = R == 4 ? 2 : R == 8 ? 3 : R == 16 ? 4 : R == 32 ? 5 : R == 64 ? 6 : R == 128 ? 7 : R == 256 ? 8 : R == 512 ? 9 : R == 1024 ? 10 : 3;
  logic [32-1:0] t_redc_wide;
  assign t_redc_wide = t_plus_mn >> r_log2;
  // Final reduction: if t_redc >= N, subtract N
  always_comb begin
    if (t_redc_wide >= 32'($unsigned(N))) begin
      result = NWIDTH'(t_redc_wide - 32'($unsigned(N)));
    end else begin
      result = NWIDTH'(t_redc_wide);
    end
  end

endmodule

