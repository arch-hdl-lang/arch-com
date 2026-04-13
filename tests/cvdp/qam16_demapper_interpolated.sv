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
  output logic [0:0] error_flag
);

  // All sample slots (mapped + interp interleaved)
  logic signed [TOTAL_SAMPLES-1:0] [IN_WIDTH-1:0] si_val;
  logic signed [TOTAL_SAMPLES-1:0] [IN_WIDTH-1:0] sq_val;
  // Mapped values per output symbol
  logic signed [N-1:0] [IN_WIDTH-1:0] mi;
  logic signed [N-1:0] [IN_WIDTH-1:0] mq;
  // Per-group values extracted to individual wires (avoids $bits on variable-indexed Vec)
  logic signed [NUM_GROUPS-1:0] [IN_WIDTH-1:0] g_interp_i;
  logic signed [NUM_GROUPS-1:0] [IN_WIDTH-1:0] g_map0_i;
  logic signed [NUM_GROUPS-1:0] [IN_WIDTH-1:0] g_map1_i;
  logic signed [NUM_GROUPS-1:0] [IN_WIDTH-1:0] g_interp_q;
  logic signed [NUM_GROUPS-1:0] [IN_WIDTH-1:0] g_map0_q;
  logic signed [NUM_GROUPS-1:0] [IN_WIDTH-1:0] g_map1_q;
  // Wider intermediates for error detection
  // two_interp = interp + interp: SInt<IN_WIDTH+1>
  // sum_maps = m0 + m1: SInt<IN_WIDTH+1> (SInt<IN_WIDTH> + SInt<IN_WIDTH>)
  logic signed [NUM_GROUPS-1:0] [IN_WIDTH + 1-1:0] two_interp_i;
  logic signed [NUM_GROUPS-1:0] [IN_WIDTH + 1-1:0] sum_maps_i;
  logic signed [NUM_GROUPS-1:0] [IN_WIDTH + 1-1:0] two_interp_q;
  logic signed [NUM_GROUPS-1:0] [IN_WIDTH + 1-1:0] sum_maps_q;
  // Error per group and accumulator
  logic [NUM_GROUPS-1:0] [0:0] grp_err;
  logic [NUM_GROUPS + 1-1:0] [0:0] err_acc;
  logic [N-1:0] [1:0] i_bits;
  logic [N-1:0] [1:0] q_bits;
  logic [N + 1-1:0] [TOTAL_OUT_WIDTH-1:0] bits_acc;
  // Extract samples (pack_signal puts I_values[0] at MSB: index k at (TOTAL_SAMPLES-1-k)*IN_WIDTH)
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
  // Extract per-group values into individual wires
  always_comb begin
    for (int gi = 0; gi <= NUM_GROUPS - 1; gi++) begin
      g_interp_i[gi] = si_val[3 * gi + 1];
      g_map0_i[gi] = si_val[3 * gi];
      g_map1_i[gi] = si_val[3 * gi + 2];
      g_interp_q[gi] = sq_val[3 * gi + 1];
      g_map0_q[gi] = sq_val[3 * gi];
      g_map1_q[gi] = sq_val[3 * gi + 2];
    end
  end
  // Compute wider intermediates for error detection (no .sext on Vec elements)
  // two_interp = interp + interp -> SInt<IN_WIDTH+1> (auto-widen: SInt<N>+SInt<N>=SInt<N+1>)
  // sum_maps = m0 + m1 -> SInt<IN_WIDTH+1> (same rule)
  always_comb begin
    for (int gi = 0; gi <= NUM_GROUPS - 1; gi++) begin
      two_interp_i[gi] = g_interp_i[gi] + g_interp_i[gi];
      sum_maps_i[gi] = g_map0_i[gi] + g_map1_i[gi];
      two_interp_q[gi] = g_interp_q[gi] + g_interp_q[gi];
      sum_maps_q[gi] = g_map0_q[gi] + g_map1_q[gi];
    end
  end
  // Compute per-group error: |2*interp - sum| > 2*threshold
  // two_interp: SInt<IN_WIDTH+1>, sum_maps: SInt<IN_WIDTH+2>
  // Direct comparison without sext (avoids $bits on variable-indexed Vec)
  always_comb begin
    for (int gi = 0; gi <= NUM_GROUPS - 1; gi++) begin
      if (two_interp_i[gi] - sum_maps_i[gi] > 2 * ERROR_THRESHOLD) begin
        grp_err[gi] = 1;
      end else if (two_interp_i[gi] - sum_maps_i[gi] < -(2 * ERROR_THRESHOLD)) begin
        grp_err[gi] = 1;
      end else if (two_interp_q[gi] - sum_maps_q[gi] > 2 * ERROR_THRESHOLD) begin
        grp_err[gi] = 1;
      end else if (two_interp_q[gi] - sum_maps_q[gi] < -(2 * ERROR_THRESHOLD)) begin
        grp_err[gi] = 1;
      end else begin
        grp_err[gi] = 0;
      end
    end
  end
  // Accumulate error flag
  always_comb begin
    err_acc[0] = 0;
    for (int gi = 0; gi <= NUM_GROUPS - 1; gi++) begin
      err_acc[gi + 1] = err_acc[gi] | grp_err[gi];
    end
  end
  // Bit mapping: -3->00, -1->01, 1->10, 3->11
  // Output packing: symbol 0 at MSB
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

