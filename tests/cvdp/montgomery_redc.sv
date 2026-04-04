// Montgomery reduction: result = T * R^{-1} mod N
// Combinational implementation
// N, R, R_INVERSE are parameters; NWIDTH and TWIDTH are derived widths
// Algorithm: REDC using the formula (T + m*N) >> log2(R)
// where m = (T mod R) * N' mod R, N' = -N^{-1} mod R
module montgomery_redc #(
  parameter int N = 7,
  parameter int R = 8,
  parameter int R_INVERSE = 1,
  parameter int NWIDTH = 3,
  parameter int TWIDTH = 6
) (
  input logic [TWIDTH-1:0] T,
  output logic [NWIDTH-1:0] result
);

  // Derived constants (computed combinationally from params)
  // N_PRIME = (R * R_INVERSE - 1) / N
  // Use 16-bit intermediates to handle a wide range of N,R values
  logic [16-1:0] n_prime_wide;
  assign n_prime_wide = 16'(16'($unsigned(R)) * 16'($unsigned(R_INVERSE)) - 16'($unsigned(1))) / 16'($unsigned(N));
  // T_mod_R = T & (R-1) since R is a power of 2
  // We use up to 16 bits for R-1
  logic [16-1:0] r_mask;
  assign r_mask = 16'(16'($unsigned(R)) - 16'($unsigned(1)));
  logic [16-1:0] t_mod_r;
  assign t_mod_r = 16'($unsigned(T)) & r_mask;
  // m = (T_mod_R * N_PRIME) mod R
  logic [32-1:0] t_x_np;
  assign t_x_np = 32'(32'($unsigned(t_mod_r)) * 32'($unsigned(n_prime_wide)));
  logic [16-1:0] m_wide;
  assign m_wide = 16'(t_x_np) & r_mask;
  // t_redc = (T + m * N) / R
  logic [32-1:0] m_x_n;
  assign m_x_n = 32'(32'($unsigned(m_wide)) * 32'($unsigned(N)));
  logic [32-1:0] t_plus_mn;
  assign t_plus_mn = 32'(32'($unsigned(T)) + m_x_n);
  // Divide by R (right shift by log2(R))
  // Use a fixed 5-bit shift amount based on R
  logic [5-1:0] r_log2;
  assign r_log2 = R == 4 ? 2 : R == 8 ? 3 : R == 16 ? 4 : R == 32 ? 5 : R == 64 ? 6 : R == 128 ? 7 : R == 256 ? 8 : R == 512 ? 9 : 10;
  logic [32-1:0] t_redc_wide;
  assign t_redc_wide = t_plus_mn >> r_log2;
  // Final reduction: if t_redc >= N, subtract N
  logic [16-1:0] t_redc_n;
  assign t_redc_n = 16'(t_redc_wide);
  always_comb begin
    if (t_redc_n >= 16'($unsigned(N))) begin
      result = NWIDTH'(t_redc_n - 16'($unsigned(N)));
    end else begin
      result = NWIDTH'(t_redc_n);
    end
  end

endmodule

