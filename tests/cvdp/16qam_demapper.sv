module qam16_demapper_interpolated #(
  parameter int N = 4,
  parameter int OUT_WIDTH = 4,
  parameter int IN_WIDTH = 3,
  parameter int ERROR_THRESHOLD = 1,
  parameter int TOTAL_SAMPLES = N + N / 2,
  parameter int TOTAL_I_WIDTH = TOTAL_SAMPLES * IN_WIDTH,
  parameter int TOTAL_OUT_WIDTH = N * OUT_WIDTH,
  parameter int NUM_GROUPS = N / 2
) (
  input logic [TOTAL_I_WIDTH-1:0] I,
  input logic [TOTAL_I_WIDTH-1:0] Q,
  output logic [TOTAL_OUT_WIDTH-1:0] bits,
  output logic [1-1:0] error_flag
);

  // Extract all samples
  logic signed [IN_WIDTH-1:0] si_val [TOTAL_SAMPLES-1:0];
  logic signed [IN_WIDTH-1:0] sq_val [TOTAL_SAMPLES-1:0];
  // Mapped values per output symbol
  logic signed [IN_WIDTH-1:0] mi [N-1:0];
  logic signed [IN_WIDTH-1:0] mq [N-1:0];
  // Error detection: use 2x scale to avoid division
  // 2*interp - (m0 + m1) vs 2*threshold
  logic signed [IN_WIDTH + 2-1:0] twice_interp_i [NUM_GROUPS-1:0];
  logic signed [IN_WIDTH + 1-1:0] sum_mapped_i [NUM_GROUPS-1:0];
  logic signed [IN_WIDTH + 2-1:0] diff2_i [NUM_GROUPS-1:0];
  logic signed [IN_WIDTH + 2-1:0] twice_interp_q [NUM_GROUPS-1:0];
  logic signed [IN_WIDTH + 1-1:0] sum_mapped_q [NUM_GROUPS-1:0];
  logic signed [IN_WIDTH + 2-1:0] diff2_q [NUM_GROUPS-1:0];
  logic [1-1:0] err_acc [NUM_GROUPS + 1-1:0];
  logic [2-1:0] i_bits [N-1:0];
  logic [2-1:0] q_bits [N-1:0];
  logic [TOTAL_OUT_WIDTH-1:0] bits_acc [N + 1-1:0];
  // Extract samples using shift+trunc (MSB-first packing)
  always_comb begin
    for (int k = 0; k <= TOTAL_SAMPLES - 1; k++) begin
      si_val[k] = $signed(IN_WIDTH'(I >> (TOTAL_SAMPLES - 1 - k) * IN_WIDTH));
      sq_val[k] = $signed(IN_WIDTH'(Q >> (TOTAL_SAMPLES - 1 - k) * IN_WIDTH));
    end
  end
  // Extract mapped values
  always_comb begin
    for (int si = 0; si <= N - 1; si++) begin
      if (si % 2 == 0) begin
        mi[si] = si_val[3 * (si / 2)];
        mq[si] = sq_val[3 * (si / 2)];
      end else begin
        mi[si] = si_val[3 * (si / 2) + 2];
        mq[si] = sq_val[3 * (si / 2) + 2];
      end
    end
  end
  // Error detection using 2x scale
  always_comb begin
    err_acc[0] = 0;
    for (int gi = 0; gi <= NUM_GROUPS - 1; gi++) begin
      twice_interp_i[gi] = {{(IN_WIDTH + 2-$bits(si_val[3 * gi + 1])){si_val[3 * gi + 1][$bits(si_val[3 * gi + 1])-1]}}, si_val[3 * gi + 1]} + {{(IN_WIDTH + 2-$bits(si_val[3 * gi + 1])){si_val[3 * gi + 1][$bits(si_val[3 * gi + 1])-1]}}, si_val[3 * gi + 1]};
      twice_interp_q[gi] = {{(IN_WIDTH + 2-$bits(sq_val[3 * gi + 1])){sq_val[3 * gi + 1][$bits(sq_val[3 * gi + 1])-1]}}, sq_val[3 * gi + 1]} + {{(IN_WIDTH + 2-$bits(sq_val[3 * gi + 1])){sq_val[3 * gi + 1][$bits(sq_val[3 * gi + 1])-1]}}, sq_val[3 * gi + 1]};
      sum_mapped_i[gi] = {{(IN_WIDTH + 1-$bits(si_val[3 * gi])){si_val[3 * gi][$bits(si_val[3 * gi])-1]}}, si_val[3 * gi]} + {{(IN_WIDTH + 1-$bits(si_val[3 * gi + 2])){si_val[3 * gi + 2][$bits(si_val[3 * gi + 2])-1]}}, si_val[3 * gi + 2]};
      sum_mapped_q[gi] = {{(IN_WIDTH + 1-$bits(sq_val[3 * gi])){sq_val[3 * gi][$bits(sq_val[3 * gi])-1]}}, sq_val[3 * gi]} + {{(IN_WIDTH + 1-$bits(sq_val[3 * gi + 2])){sq_val[3 * gi + 2][$bits(sq_val[3 * gi + 2])-1]}}, sq_val[3 * gi + 2]};
      diff2_i[gi] = twice_interp_i[gi] - {{(IN_WIDTH + 2-$bits(sum_mapped_i[gi])){sum_mapped_i[gi][$bits(sum_mapped_i[gi])-1]}}, sum_mapped_i[gi]};
      diff2_q[gi] = twice_interp_q[gi] - {{(IN_WIDTH + 2-$bits(sum_mapped_q[gi])){sum_mapped_q[gi][$bits(sum_mapped_q[gi])-1]}}, sum_mapped_q[gi]};
      if (diff2_i[gi] > 2 * ERROR_THRESHOLD) begin
        err_acc[gi + 1] = 1;
      end else if (diff2_i[gi] < -(2 * ERROR_THRESHOLD)) begin
        err_acc[gi + 1] = 1;
      end else if (diff2_q[gi] > 2 * ERROR_THRESHOLD) begin
        err_acc[gi + 1] = 1;
      end else if (diff2_q[gi] < -(2 * ERROR_THRESHOLD)) begin
        err_acc[gi + 1] = 1;
      end else begin
        err_acc[gi + 1] = err_acc[gi];
      end
    end
    // 2 * interp (shift left 1 of sign-extended value)
    // sum of two mapped values
    // diff2 = 2*interp - sum
    // |diff2| > 2*threshold
  end
  // Bit mapping: -3->00, -1->01, 1->10, 3->11
  always_comb begin
    bits_acc[0] = 0;
    for (int si = 0; si <= N - 1; si++) begin
      if (mi[si] < -2) begin
        i_bits[si] = 0;
      end else if (mi[si] < 0) begin
        i_bits[si] = 1;
      end else if (mi[si] < 2) begin
        i_bits[si] = 2;
      end else begin
        i_bits[si] = 3;
      end
      if (mq[si] < -2) begin
        q_bits[si] = 0;
      end else if (mq[si] < 0) begin
        q_bits[si] = 1;
      end else if (mq[si] < 2) begin
        q_bits[si] = 2;
      end else begin
        q_bits[si] = 3;
      end
      bits_acc[si + 1] = bits_acc[si] | TOTAL_OUT_WIDTH'($unsigned({i_bits[si], q_bits[si]})) << (N - 1 - si) * OUT_WIDTH;
    end
  end
  assign bits = bits_acc[N];
  assign error_flag = err_acc[NUM_GROUPS];

endmodule

