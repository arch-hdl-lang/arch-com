module halfband_fir #(
  parameter int LGNTAPS = 7,
  parameter int IW = 16,
  parameter int TW = 12,
  parameter int OW = IW + TW + LGNTAPS,
  parameter int NTAPS = 107,
  parameter int OPT_HILBERT = 0,
  parameter int LGNMEM = LGNTAPS - 1,
  parameter int LGNCOEF = LGNMEM - 1,
  parameter int HALFTAPS = NTAPS / 2,
  parameter int QTRTAPS = HALFTAPS / 2 + 1,
  parameter int DMEMSZ = 1 << LGNMEM,
  parameter int CMEMSZ = 1 << LGNCOEF,
  parameter int HALFTAPS_M1 = HALFTAPS - 1,
  parameter int QTRTAPS_M1 = QTRTAPS - 1,
  parameter int QTRTAPS_M2 = QTRTAPS - 2
) (
  input logic i_clk,
  input logic i_reset,
  input logic i_tap_wr,
  input logic [TW-1:0] i_tap,
  input logic i_ce,
  input logic signed [IW-1:0] i_sample,
  output logic o_ce,
  output logic signed [OW-1:0] o_result
);

  // Pre-folded constants to avoid SV width mismatches in comparisons
  // Coefficient memory
  logic [CMEMSZ-1:0] [TW-1:0] coef_mem;
  logic [LGNCOEF-1:0] tap_wr_idx;
  // Circular sample memories (signed elements — now valid with fixed codegen)
  logic signed [DMEMSZ-1:0] [IW-1:0] dmem1;
  logic signed [DMEMSZ-1:0] [IW-1:0] dmem2;
  logic [LGNMEM-1:0] write_idx;
  // Pointer state
  logic [LGNMEM-1:0] left_idx;
  logic [LGNMEM-1:0] right_idx;
  logic [LGNCOEF-1:0] tap_idx;
  // Sample pipeline
  logic signed [IW-1:0] mid_sample;
  logic signed [IW-1:0] sample_left;
  logic signed [IW-1:0] sample_right;
  logic signed [IW + 1-1:0] sum_data;
  logic [TW-1:0] current_coef;
  // Control pipeline
  logic clk_en;
  logic data_en;
  logic sum_en;
  // messy_flag: 4-bit pipeline with feedback
  logic mf0;
  logic mf1;
  logic mf2;
  logic mf3;
  // Computation
  logic signed [IW + TW-1:0] mult_result;
  logic signed [OW-1:0] acc_result;
  logic signed [OW-1:0] mid_prod_r;
  logic signed [OW-1:0] o_result_r;
  logic o_ce_r;
  // last_tap when (QTRTAPS - tap_idx) < 2 ↔ tap_idx > QTRTAPS - 2
  logic last_tap_warn;
  assign last_tap_warn = tap_idx > LGNCOEF'(QTRTAPS_M1);
  logic last_data_warn;
  assign last_data_warn = tap_idx > LGNCOEF'(QTRTAPS_M2);
  logic mf_chain_bit;
  assign mf_chain_bit = clk_en | mf0 & ~last_tap_warn;
  assign o_result = o_result_r;
  assign o_ce = o_ce_r;
  always_ff @(posedge i_clk) begin
    if (i_reset) begin
      acc_result <= 0;
      clk_en <= 1'b0;
      current_coef <= 0;
      data_en <= 1'b0;
      left_idx <= 0;
      mf0 <= 1'b0;
      mf1 <= 1'b0;
      mf2 <= 1'b0;
      mf3 <= 1'b0;
      mid_prod_r <= 0;
      mid_sample <= 0;
      mult_result <= 0;
      o_ce_r <= 1'b0;
      o_result_r <= 0;
      right_idx <= 0;
      sample_left <= 0;
      sample_right <= 0;
      sum_data <= 0;
      sum_en <= 1'b0;
      tap_idx <= 0;
      tap_wr_idx <= 0;
      write_idx <= 0;
    end else begin
      // --- Coefficient write ---
      if (i_tap_wr) begin
        coef_mem[tap_wr_idx] <= i_tap;
        tap_wr_idx <= LGNCOEF'(tap_wr_idx + 1);
      end
      // --- Sample write ---
      if (i_ce) begin
        dmem1[write_idx] <= i_sample;
        dmem2[write_idx] <= mid_sample;
        write_idx <= LGNMEM'(write_idx + 1);
      end
      // --- Mid sample: captured from sample_left on each i_ce ---
      if (i_ce) begin
        mid_sample <= sample_left;
      end
      // --- Clock enable ---
      clk_en <= i_ce;
      // --- Index management ---
      // right_idx = write_idx - HALFTAPS + 1 = write_idx - (HALFTAPS - 1)
      if (i_ce) begin
        left_idx <= write_idx;
        right_idx <= LGNMEM'(write_idx - LGNMEM'(HALFTAPS_M1));
      end else if (clk_en | ~last_data_warn) begin
        left_idx <= LGNMEM'(left_idx - 2);
        right_idx <= LGNMEM'(right_idx + 2);
      end
      // --- Tap index ---
      if (clk_en) begin
        tap_idx <= 0;
      end else if (~last_tap_warn) begin
        tap_idx <= LGNCOEF'(tap_idx + 1);
      end
      // --- messy_flag ---
      if (i_ce) begin
        mf0 <= 1'b1;
      end else if (mf0 & ~last_tap_warn) begin
        mf0 <= 1'b1;
      end else if (~clk_en) begin
        mf0 <= 1'b0;
      end
      mf1 <= mf_chain_bit;
      mf2 <= mf1;
      mf3 <= mf2;
      // --- Sample reads (registered, 1 cycle latency) ---
      sample_left <= dmem1[left_idx];
      sample_right <= dmem2[right_idx];
      // --- Data enable ---
      data_en <= clk_en;
      // --- Coefficient read ---
      current_coef <= coef_mem[tap_idx];
      // --- Sum computation ---
      if (OPT_HILBERT == 1) begin
        sum_data <= sample_left - sample_right;
      end else begin
        sum_data <= (IW + 1)'({{(IW + 1-$bits(sample_left)){sample_left[$bits(sample_left)-1]}}, sample_left} + {{(IW + 1-$bits(sample_right)){sample_right[$bits(sample_right)-1]}}, sample_right});
      end
      // --- Sum enable ---
      sum_en <= data_en;
      // --- Multiply: sign-extend both operands to IW+TW ---
      mult_result <= (IW + TW)'((IW + TW)'($unsigned(current_coef)) * {{(IW + TW-$bits(sum_data)){sum_data[$bits(sum_data)-1]}}, sum_data});
      // --- Mid product: mid_sample * (2^(TW-1) - 1) ---
      if (clk_en) begin
        mid_prod_r <= (($bits({{(OW-$bits(mid_sample)){mid_sample[$bits(mid_sample)-1]}}, mid_sample} << TW - 1) > OW ? $bits({{(OW-$bits(mid_sample)){mid_sample[$bits(mid_sample)-1]}}, mid_sample} << TW - 1) : OW))'(({{(OW-$bits(mid_sample)){mid_sample[$bits(mid_sample)-1]}}, mid_sample} << TW - 1) - {{(OW-$bits(mid_sample)){mid_sample[$bits(mid_sample)-1]}}, mid_sample});
      end
      // --- Accumulate ---
      if (sum_en) begin
        acc_result <= mid_prod_r;
      end else if (mf3) begin
        acc_result <= OW'(acc_result + {{(OW-$bits(mult_result)){mult_result[$bits(mult_result)-1]}}, mult_result});
      end
      // --- Output capture ---
      if (sum_en) begin
        o_result_r <= acc_result;
      end
      o_ce_r <= sum_en;
    end
  end

endmodule

