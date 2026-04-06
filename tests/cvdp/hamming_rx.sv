module hamming_rx #(
  parameter int DATA_WIDTH = 8,
  parameter int PARITY_BIT = 4,
  parameter int ENCODED_DATA = DATA_WIDTH + PARITY_BIT + 1,
  parameter int ENCODED_DATA_BIT = $clog2(ENCODED_DATA)
) (
  input logic [ENCODED_DATA-1:0] data_in,
  output logic [DATA_WIDTH-1:0] data_out
);

  logic [1-1:0] parity [ENCODED_DATA_BIT-1:0];
  logic [ENCODED_DATA_BIT-1:0] syndrome;
  logic [ENCODED_DATA-1:0] corrected;
  logic [$clog2(DATA_WIDTH)-1:0] data_idx;
  always_comb begin
    // Compute parity check bits over received codeword.
    for (int j = 0; j <= ENCODED_DATA_BIT - 1; j++) begin
      parity[j] = 0;
      for (int k = 0; k <= ENCODED_DATA - 1; k++) begin
        if ((k >> j & 1) == 1) begin
          parity[j] = parity[j] ^ data_in[k +: 1];
        end
      end
    end
    // Syndrome is parity vector interpreted as a bit index.
    syndrome = 0;
    for (int j = 0; j <= ENCODED_DATA_BIT - 1; j++) begin
      syndrome[j +: 1] = parity[j];
    end
    // Correct single-bit error at syndrome index.
    for (int i = 0; i <= ENCODED_DATA - 1; i++) begin
      corrected[i +: 1] = data_in[i +: 1];
      if (syndrome != 0 & syndrome == i) begin
        corrected[i +: 1] = ~data_in[i +: 1];
      end
    end
    // Extract data bits from non-parity positions.
    data_out = 0;
    data_idx = 0;
    for (int i = 0; i <= ENCODED_DATA - 1; i++) begin
      if (i != 0 & (i & i - 1) != 0) begin
        data_out[data_idx +: 1] = corrected[i +: 1];
        if (data_idx == DATA_WIDTH - 1) begin
          data_idx = 0;
        end else begin
          data_idx = $clog2(DATA_WIDTH)'(data_idx + 1);
        end
      end
    end
  end

endmodule

