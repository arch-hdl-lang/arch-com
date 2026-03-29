module qam16_mapper_interpolated #(
  parameter int N = 4,
  parameter int IN_WIDTH = 4,
  parameter int OUT_WIDTH = 3
) (
  input logic [N * IN_WIDTH-1:0] bits,
  output logic [(N + N / 2) * OUT_WIDTH-1:0] I,
  output logic [(N + N / 2) * OUT_WIDTH-1:0] Q
);

  // Per-symbol mapped values
  logic [OUT_WIDTH-1:0] mi [0:N-1];
  logic [OUT_WIDTH-1:0] mq [0:N-1];
  // Output elements: N + N/2 total
  logic [OUT_WIDTH-1:0] out_i [0:N + N / 2-1];
  logic [OUT_WIDTH-1:0] out_q [0:N + N / 2-1];
  // Accumulator for packing
  logic [(N + N / 2) * OUT_WIDTH-1:0] acc_i [0:N + N / 2 + 1-1];
  logic [(N + N / 2) * OUT_WIDTH-1:0] acc_q [0:N + N / 2 + 1-1];
  // Map: 00->-3(101), 01->-1(111), 10->1(001), 11->3(011)
  // Formula: (2*x + 5) mod 8
  always_comb begin
    for (int i = 0; i <= N - 1; i++) begin
      mi[i] = OUT_WIDTH'(bits[i * IN_WIDTH + IN_WIDTH - 2 +: 2] * 2 + 5);
      mq[i] = OUT_WIDTH'(bits[i * IN_WIDTH +: 2] * 2 + 5);
    end
  end
  // Build output elements: for each pair, first/interp/second
  always_comb begin
    for (int j = 0; j <= N / 2 - 1; j++) begin
      out_i[3 * j] = mi[2 * j];
      out_q[3 * j] = mq[2 * j];
      out_i[3 * j + 1] = OUT_WIDTH'({mi[2 * j][OUT_WIDTH - 1], mi[2 * j]} + {mi[2 * j + 1][OUT_WIDTH - 1], mi[2 * j + 1]} >> 1);
      out_q[3 * j + 1] = OUT_WIDTH'({mq[2 * j][OUT_WIDTH - 1], mq[2 * j]} + {mq[2 * j + 1][OUT_WIDTH - 1], mq[2 * j + 1]} >> 1);
      out_i[3 * j + 2] = mi[2 * j + 1];
      out_q[3 * j + 2] = mq[2 * j + 1];
    end
  end
  // Pack via accumulator: acc[0]=0, acc[k+1] = acc[k] | (out[k] << k*OUT_WIDTH)
  always_comb begin
    acc_i[0] = 0;
    acc_q[0] = 0;
    for (int k = 0; k <= N + N / 2 - 1; k++) begin
      acc_i[k + 1] = acc_i[k] | ((N + N / 2) * OUT_WIDTH)'($unsigned(out_i[k])) << k * OUT_WIDTH;
      acc_q[k + 1] = acc_q[k] | ((N + N / 2) * OUT_WIDTH)'($unsigned(out_q[k])) << k * OUT_WIDTH;
    end
    I = acc_i[N + N / 2];
    Q = acc_q[N + N / 2];
  end

endmodule

