// Montgomery modular multiplication: result = (a * b) mod N
// Uses Montgomery reduction with R = power-of-2
module montgomery_mult #(
  parameter int N = 7,
  parameter int R = 8,
  parameter int R_INVERSE = 1,
  parameter int NWIDTH = 3,
  parameter int TWIDTH = 6
) (
  input logic clk,
  input logic rst_n,
  input logic [NWIDTH-1:0] a,
  input logic [NWIDTH-1:0] b,
  input logic valid_in,
  output logic [NWIDTH-1:0] result,
  output logic valid_out
);

  // R_MOD_N = R mod N (precomputed as R - N when R > N and a power of 2 less than 2*N)
  logic [NWIDTH-1:0] r_mod_n;
  assign r_mod_n = NWIDTH'(16'($unsigned(R)) - 16'($unsigned(N)));
  // Pipeline registers
  logic [NWIDTH-1:0] a_q;
  logic [NWIDTH-1:0] b_q;
  logic [NWIDTH-1:0] a_redc_q;
  logic [NWIDTH-1:0] b_redc_q;
  logic [NWIDTH-1:0] result_q;
  logic vin_q;
  logic vin_q1;
  logic vin_q2;
  logic vout_q;
  // TWIDTH = NWIDTH + r_log2; use generous 2*NWIDTH+4 for the multiplied input to redc
  // Compute ar = a_q * R_MOD_N and br = b_q * R_MOD_N (result fits in TWIDTH=2*NWIDTH bits)
  logic [TWIDTH-1:0] ar;
  assign ar = TWIDTH'(TWIDTH'($unsigned(a_q)) * TWIDTH'($unsigned(r_mod_n)));
  logic [TWIDTH-1:0] br;
  assign br = TWIDTH'(TWIDTH'($unsigned(b_q)) * TWIDTH'($unsigned(r_mod_n)));
  // Montgomery redc for a and b
  logic [NWIDTH-1:0] a_redc;
  logic [NWIDTH-1:0] b_redc;
  montgomery_redc #(.N(N), .R(R), .R_INVERSE(R_INVERSE), .NWIDTH(NWIDTH), .TWIDTH(TWIDTH)) redc_a (
    .T(ar),
    .result(a_redc)
  );
  montgomery_redc #(.N(N), .R(R), .R_INVERSE(R_INVERSE), .NWIDTH(NWIDTH), .TWIDTH(TWIDTH)) redc_b (
    .T(br),
    .result(b_redc)
  );
  // Product of a_redc_q and b_redc_q (fits in TWIDTH=2*NWIDTH bits)
  logic [TWIDTH-1:0] ab_t;
  assign ab_t = TWIDTH'(TWIDTH'($unsigned(a_redc_q)) * TWIDTH'($unsigned(b_redc_q)));
  // Final redc of product
  logic [NWIDTH-1:0] result_d;
  montgomery_redc #(.N(N), .R(R), .R_INVERSE(R_INVERSE), .NWIDTH(NWIDTH), .TWIDTH(TWIDTH)) redc_ab (
    .T(ab_t),
    .result(result_d)
  );
  assign result = result_q;
  assign valid_out = vout_q;
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      a_q <= 0;
      a_redc_q <= 0;
      b_q <= 0;
      b_redc_q <= 0;
      result_q <= 0;
      vin_q <= 1'b0;
      vin_q1 <= 1'b0;
      vin_q2 <= 1'b0;
      vout_q <= 1'b0;
    end else begin
      vin_q <= valid_in;
      vin_q1 <= vin_q;
      vin_q2 <= vin_q1;
      vout_q <= vin_q2;
      if (valid_in) begin
        a_q <= a;
        b_q <= b;
      end
      a_redc_q <= a_redc;
      b_redc_q <= b_redc;
      result_q <= result_d;
    end
  end

endmodule

