module t_hamming_tx #(
  parameter int DATA_WIDTH = 4,
  parameter int PARITY_BIT = 3,
  parameter int ENCODED_DATA = PARITY_BIT + DATA_WIDTH + 1,
  parameter int ENCODED_DATA_BIT = 4
) (
  input logic [DATA_WIDTH-1:0] data_in,
  output logic [ENCODED_DATA-1:0] data_out
);

  logic [PARITY_BIT-1:0] parity_w;
  logic [ENCODED_DATA-1:0] enc;
  logic [ENCODED_DATA_BIT + 1-1:0] cnt;
  always_comb begin
    enc = 0;
    parity_w = 0;
    cnt = 0;
    for (int pos = 1; pos <= ENCODED_DATA - 1; pos++) begin
      if ((pos & pos - 1) != 0) begin
        if (cnt < DATA_WIDTH) begin
          enc[pos +: 1] = data_in[ENCODED_DATA_BIT'(cnt) +: 1];
          cnt = (ENCODED_DATA_BIT + 1)'(cnt + 1);
        end
      end
    end
    for (int j = 0; j <= PARITY_BIT - 1; j++) begin
      for (int i = 1; i <= ENCODED_DATA - 1; i++) begin
        if ((i & 1 << j) != 0) begin
          parity_w[j +: 1] = parity_w[j +: 1] ^ enc[i +: 1];
        end
      end
    end
    for (int j = 0; j <= PARITY_BIT - 1; j++) begin
      enc[1 << j +: 1] = parity_w[j +: 1];
    end
    data_out = enc;
  end

endmodule

