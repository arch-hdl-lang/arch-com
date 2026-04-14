module qam16_mapper_interpolated #(
  parameter int N = 4,
  parameter int IN_WIDTH = 4,
  parameter int OUT_WIDTH = 3,
  parameter int OUTW = (N + N / 2) * OUT_WIDTH,
  parameter int NPAIRS = N / 2
) (
  input logic [N * IN_WIDTH-1:0] bits,
  output logic [OUTW-1:0] I,
  output logic [OUTW-1:0] Q
);

  // Per-symbol mapped values (unsigned 3-bit encoding of signed {-3,-1,1,3})
  logic [N-1:0] [OUT_WIDTH-1:0] mi;
  logic [N-1:0] [OUT_WIDTH-1:0] mq;
  // Interpolated values per pair
  logic [NPAIRS-1:0] [OUT_WIDTH-1:0] itp_i;
  logic [NPAIRS-1:0] [OUT_WIDTH-1:0] itp_q;
  // Output slots: N + N/2 = 3*NPAIRS total
  logic [N + N / 2-1:0] [OUT_WIDTH-1:0] out_i;
  logic [N + N / 2-1:0] [OUT_WIDTH-1:0] out_q;
  // Accumulator for packing output
  logic [N + N / 2 + 1-1:0] [OUTW-1:0] acc_i;
  logic [N + N / 2 + 1-1:0] [OUTW-1:0] acc_q;
  // Map each symbol: MSBs[3:2] -> I, LSBs[1:0] -> Q
  // Formula: (2*x + 5) mod 8: 00->5(101), 01->7(111), 10->1(001), 11->3(011)
  always_comb begin
    for (int i = 0; i <= N - 1; i++) begin
      mi[i] = OUT_WIDTH'(bits[i * IN_WIDTH + IN_WIDTH - 2 +: 2] * 2 + 5);
      mq[i] = OUT_WIDTH'(bits[i * IN_WIDTH +: 2] * 2 + 5);
    end
  end
  // Interpolate: avg(2*k1-3, 2*k2-3) = k1+k2-3 = (k1+k2+5) mod 8 in 3-bit unsigned
  // Directly use the 2-bit input codes to avoid sext (no signed arithmetic needed)
  always_comb begin
    for (int j = 0; j <= NPAIRS - 1; j++) begin
      itp_i[j] = OUT_WIDTH'(bits[2 * j * IN_WIDTH + IN_WIDTH - 2 +: 2] + bits[(2 * j + 1) * IN_WIDTH + IN_WIDTH - 2 +: 2] + 5);
      itp_q[j] = OUT_WIDTH'(bits[2 * j * IN_WIDTH +: 2] + bits[(2 * j + 1) * IN_WIDTH +: 2] + 5);
    end
  end
  // Build output slots: for each pair j, three slots at 3*j, 3*j+1, 3*j+2
  always_comb begin
    for (int j = 0; j <= NPAIRS - 1; j++) begin
      out_i[3 * j] = mi[2 * j];
      out_i[3 * j + 1] = itp_i[j];
      out_i[3 * j + 2] = mi[2 * j + 1];
      out_q[3 * j] = mq[2 * j];
      out_q[3 * j + 1] = itp_q[j];
      out_q[3 * j + 2] = mq[2 * j + 1];
    end
  end
  // Pack output accumulator
  always_comb begin
    acc_i[0] = 0;
    acc_q[0] = 0;
    for (int k = 0; k <= N + N / 2 - 1; k++) begin
      acc_i[k + 1] = acc_i[k] | OUTW'($unsigned(out_i[k])) << k * OUT_WIDTH;
      acc_q[k + 1] = acc_q[k] | OUTW'($unsigned(out_q[k])) << k * OUT_WIDTH;
    end
    I = acc_i[N + N / 2];
    Q = acc_q[N + N / 2];
  end

endmodule

