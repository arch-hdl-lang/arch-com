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

  // Scalar wires avoid packed-2D-array issues with iverilog loop indexing
  logic [TOTAL_OUT_WIDTH-1:0] bits_out;
  logic [0:0] err_out;
  // Per-symbol encode temporaries (scalar, not Vec)
  logic [1:0] i_enc;
  logic [1:0] q_enc;
  // Bit encoding and output packing in one comb block
  // pack_signal: values[0] at MSB → sample k at shift=(TOTAL_SAMPLES-1-k)*IN_WIDTH
  // Even symbol si uses sample 3*(si/2), odd uses 3*(si/2)+2
  always_comb begin
    bits_out = 0;
    i_enc = 0;
    q_enc = 0;
    for (int si = 0; si <= N - 1; si++) begin
      if (si % 2 == 0) begin
        if ($signed(IN_WIDTH'(I >> (TOTAL_SAMPLES - 1 - 3 * (si / 2)) * IN_WIDTH)) < -2) begin
          i_enc = 0;
        end else if ($signed(IN_WIDTH'(I >> (TOTAL_SAMPLES - 1 - 3 * (si / 2)) * IN_WIDTH)) < 0) begin
          i_enc = 1;
        end else if ($signed(IN_WIDTH'(I >> (TOTAL_SAMPLES - 1 - 3 * (si / 2)) * IN_WIDTH)) < 2) begin
          i_enc = 2;
        end else begin
          i_enc = 3;
        end
        if ($signed(IN_WIDTH'(Q >> (TOTAL_SAMPLES - 1 - 3 * (si / 2)) * IN_WIDTH)) < -2) begin
          q_enc = 0;
        end else if ($signed(IN_WIDTH'(Q >> (TOTAL_SAMPLES - 1 - 3 * (si / 2)) * IN_WIDTH)) < 0) begin
          q_enc = 1;
        end else if ($signed(IN_WIDTH'(Q >> (TOTAL_SAMPLES - 1 - 3 * (si / 2)) * IN_WIDTH)) < 2) begin
          q_enc = 2;
        end else begin
          q_enc = 3;
        end
      end else begin
        if ($signed(IN_WIDTH'(I >> (TOTAL_SAMPLES - 3 - 3 * (si / 2)) * IN_WIDTH)) < -2) begin
          i_enc = 0;
        end else if ($signed(IN_WIDTH'(I >> (TOTAL_SAMPLES - 3 - 3 * (si / 2)) * IN_WIDTH)) < 0) begin
          i_enc = 1;
        end else if ($signed(IN_WIDTH'(I >> (TOTAL_SAMPLES - 3 - 3 * (si / 2)) * IN_WIDTH)) < 2) begin
          i_enc = 2;
        end else begin
          i_enc = 3;
        end
        if ($signed(IN_WIDTH'(Q >> (TOTAL_SAMPLES - 3 - 3 * (si / 2)) * IN_WIDTH)) < -2) begin
          q_enc = 0;
        end else if ($signed(IN_WIDTH'(Q >> (TOTAL_SAMPLES - 3 - 3 * (si / 2)) * IN_WIDTH)) < 0) begin
          q_enc = 1;
        end else if ($signed(IN_WIDTH'(Q >> (TOTAL_SAMPLES - 3 - 3 * (si / 2)) * IN_WIDTH)) < 2) begin
          q_enc = 2;
        end else begin
          q_enc = 3;
        end
      end
      bits_out = bits_out | TOTAL_OUT_WIDTH'($unsigned({i_enc, q_enc})) << (N - 1 - si) * OUT_WIDTH;
    end
  end
  // Error detection: group gi, samples at 3*gi (map0), 3*gi+1 (interp), 3*gi+2 (map1)
  // Auto-widening: SInt<IN_WIDTH>+SInt<IN_WIDTH> = SInt<IN_WIDTH+1>
  // (two_interp) - (sum_maps) = SInt<IN_WIDTH+2>
  // Compare with 2*ERROR_THRESHOLD (no sext needed - comparison auto-promotes)
  always_comb begin
    err_out = 0;
    for (int gi = 0; gi <= NUM_GROUPS - 1; gi++) begin
      if ($signed(IN_WIDTH'(I >> (TOTAL_SAMPLES - 2 - 3 * gi) * IN_WIDTH)) + $signed(IN_WIDTH'(I >> (TOTAL_SAMPLES - 2 - 3 * gi) * IN_WIDTH)) - ($signed(IN_WIDTH'(I >> (TOTAL_SAMPLES - 1 - 3 * gi) * IN_WIDTH)) + $signed(IN_WIDTH'(I >> (TOTAL_SAMPLES - 3 - 3 * gi) * IN_WIDTH))) > 2 * ERROR_THRESHOLD) begin
        err_out = 1;
      end else if ($signed(IN_WIDTH'(I >> (TOTAL_SAMPLES - 2 - 3 * gi) * IN_WIDTH)) + $signed(IN_WIDTH'(I >> (TOTAL_SAMPLES - 2 - 3 * gi) * IN_WIDTH)) - ($signed(IN_WIDTH'(I >> (TOTAL_SAMPLES - 1 - 3 * gi) * IN_WIDTH)) + $signed(IN_WIDTH'(I >> (TOTAL_SAMPLES - 3 - 3 * gi) * IN_WIDTH))) < -(2 * ERROR_THRESHOLD)) begin
        err_out = 1;
      end else if ($signed(IN_WIDTH'(Q >> (TOTAL_SAMPLES - 2 - 3 * gi) * IN_WIDTH)) + $signed(IN_WIDTH'(Q >> (TOTAL_SAMPLES - 2 - 3 * gi) * IN_WIDTH)) - ($signed(IN_WIDTH'(Q >> (TOTAL_SAMPLES - 1 - 3 * gi) * IN_WIDTH)) + $signed(IN_WIDTH'(Q >> (TOTAL_SAMPLES - 3 - 3 * gi) * IN_WIDTH))) > 2 * ERROR_THRESHOLD) begin
        err_out = 1;
      end else if ($signed(IN_WIDTH'(Q >> (TOTAL_SAMPLES - 2 - 3 * gi) * IN_WIDTH)) + $signed(IN_WIDTH'(Q >> (TOTAL_SAMPLES - 2 - 3 * gi) * IN_WIDTH)) - ($signed(IN_WIDTH'(Q >> (TOTAL_SAMPLES - 1 - 3 * gi) * IN_WIDTH)) + $signed(IN_WIDTH'(Q >> (TOTAL_SAMPLES - 3 - 3 * gi) * IN_WIDTH))) < -(2 * ERROR_THRESHOLD)) begin
        err_out = 1;
      end
    end
  end
  assign bits = bits_out;
  assign error_flag = err_out;

endmodule

