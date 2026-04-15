module onebit_ecc #(
  parameter int DATA_WIDTH = 16,
  parameter int CODE_WIDTH = DATA_WIDTH + $clog2(DATA_WIDTH + 1)
) (
  input logic [DATA_WIDTH-1:0] data_in,
  input logic [CODE_WIDTH-1:0] received,
  output logic [DATA_WIDTH-1:0] data_out,
  output logic [CODE_WIDTH-1:0] encoded,
  output logic error_detected,
  output logic error_corrected
);

  logic [$clog2(CODE_WIDTH + 1)-1:0] [0:0] parity_enc;
  logic [$clog2(DATA_WIDTH + 1)-1:0] data_idx_enc;
  logic [$clog2($clog2(CODE_WIDTH + 1) + 1)-1:0] parity_idx_enc;
  logic [$clog2(CODE_WIDTH + 1)-1:0] [0:0] parity_dec;
  logic [$clog2(CODE_WIDTH + 1)-1:0] syndrome_val;
  logic [CODE_WIDTH-1:0] corrected;
  logic [$clog2(DATA_WIDTH + 1)-1:0] data_idx_dec;
  always_comb begin
    // ---- Encoder: place data bits, compute parity ----
    // Use 1-indexed positions: bit i corresponds to position (i+1).
    // Parity positions are where (i+1) is a power of 2, i.e. ((i+1) & i) == 0.
    encoded = 0;
    data_idx_enc = 0;
    // Place data bits in non-parity positions
    for (int i = 0; i <= CODE_WIDTH - 1; i++) begin
      if ((i + 1 & i) == 0) begin
        encoded[i +: 1] = 0;
      end else begin
        encoded[i +: 1] = data_in[data_idx_enc +: 1];
        if (data_idx_enc == DATA_WIDTH - 1) begin
          data_idx_enc = 0;
        end else begin
          data_idx_enc = ($clog2(DATA_WIDTH + 1))'(data_idx_enc + 1);
        end
      end
    end
    // Parity position (1-indexed power of 2): leave as 0 for now
    // Calculate parity bits using 1-indexed positions
    for (int j = 0; j <= $clog2(CODE_WIDTH + 1) - 1; j++) begin
      parity_enc[j] = 0;
      for (int k = 0; k <= CODE_WIDTH - 1; k++) begin
        if ((k + 1 >> j & 1) == 1) begin
          parity_enc[j] = parity_enc[j] ^ encoded[k +: 1];
        end
      end
    end
    // Write parity bits into parity positions
    parity_idx_enc = 0;
    for (int l = 0; l <= CODE_WIDTH - 1; l++) begin
      if ((l + 1 & l) == 0) begin
        encoded[l +: 1] = parity_enc[parity_idx_enc];
        parity_idx_enc = ($clog2($clog2(CODE_WIDTH + 1) + 1))'(parity_idx_enc + 1);
      end
    end
    // ---- Decoder: compute syndrome, correct, extract ----
    for (int j = 0; j <= $clog2(CODE_WIDTH + 1) - 1; j++) begin
      parity_dec[j] = 0;
      for (int k = 0; k <= CODE_WIDTH - 1; k++) begin
        if ((k + 1 >> j & 1) == 1) begin
          parity_dec[j] = parity_dec[j] ^ received[k +: 1];
        end
      end
    end
    syndrome_val = 0;
    for (int j = 0; j <= $clog2(CODE_WIDTH + 1) - 1; j++) begin
      syndrome_val[j +: 1] = parity_dec[j];
    end
    // Correct single-bit error: syndrome is 1-indexed position, convert to 0-indexed
    for (int i = 0; i <= CODE_WIDTH - 1; i++) begin
      corrected[i +: 1] = received[i +: 1];
      if ((syndrome_val != 0) & (syndrome_val == i + 1)) begin
        corrected[i +: 1] = ~received[i +: 1];
      end
    end
    error_detected = syndrome_val != 0;
    error_corrected = syndrome_val != 0;
    // Extract data bits from corrected codeword (non-parity positions)
    data_out = 0;
    data_idx_dec = 0;
    for (int i = 0; i <= CODE_WIDTH - 1; i++) begin
      if ((i + 1 & i) != 0) begin
        data_out[data_idx_dec +: 1] = corrected[i +: 1];
        if (data_idx_dec == DATA_WIDTH - 1) begin
          data_idx_dec = 0;
        end else begin
          data_idx_dec = ($clog2(DATA_WIDTH + 1))'(data_idx_dec + 1);
        end
      end
    end
  end

endmodule

