module hamming_tx #(
  parameter int DATA_WIDTH = 8,
  parameter int PARITY_BIT = 4,
  parameter int ENCODED_DATA = DATA_WIDTH + PARITY_BIT + 1,
  parameter int ENCODED_DATA_BIT = $clog2(ENCODED_DATA)
) (
  input logic [DATA_WIDTH-1:0] data_in,
  output logic [ENCODED_DATA-1:0] data_out
);

  logic [1-1:0] parity [ENCODED_DATA_BIT-1:0];
  logic [$clog2(DATA_WIDTH)-1:0] data_idx;
  logic [$clog2(ENCODED_DATA_BIT + 1)-1:0] parity_idx;
  always_comb begin
    data_out = 0;
    data_idx = 0;
    // Place data bits in non-parity positions; parity positions are powers of two.
    for (int i = 0; i <= ENCODED_DATA - 1; i++) begin
      if (i == 0) begin
        data_out[i +: 1] = 0;
      end else if ((i & i - 1) == 0) begin
        data_out[i +: 1] = 0;
      end else begin
        data_out[i +: 1] = data_in[data_idx +: 1];
        if (data_idx == DATA_WIDTH - 1) begin
          data_idx = 0;
        end else begin
          data_idx = $clog2(DATA_WIDTH)'(data_idx + 1);
        end
      end
    end
    // Calculate parity bits.
    for (int j = 0; j <= ENCODED_DATA_BIT - 1; j++) begin
      parity[j] = 0;
      for (int k = 0; k <= ENCODED_DATA - 1; k++) begin
        if ((k >> j & 1) == 1) begin
          parity[j] = parity[j] ^ data_out[k +: 1];
        end
      end
    end
    // Write parity bits into power-of-two positions (1,2,4,8,...).
    parity_idx = 0;
    for (int l = 0; l <= ENCODED_DATA - 1; l++) begin
      if (l != 0 & (l & l - 1) == 0) begin
        data_out[l +: 1] = parity[parity_idx];
        parity_idx = ($clog2(ENCODED_DATA_BIT + 1))'(parity_idx + 1);
      end
    end
  end

endmodule

