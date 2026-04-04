module scrambler #(
  parameter int DATA_WIDTH = 128
) (
  input logic clk,
  input logic rst_n,
  input logic [DATA_WIDTH-1:0] data_in,
  input logic [4-1:0] mode,
  output logic [DATA_WIDTH-1:0] data_out,
  output logic feedback
);

  // 16-bit LFSR initialized to 0x4000 (bit 14 set)
  logic [16-1:0] lfsr;
  logic [DATA_WIDTH-1:0] data_out_r;
  logic feedback_w;
  logic [16-1:0] lfsr_next;
  // Polynomial selection: right-shift LFSR (feedback inserted at MSB=bit15)
  always_comb begin
    if (mode == 0) begin
      feedback_w = lfsr[15] ^ lfsr[14];
    end else if (mode == 1) begin
      feedback_w = lfsr[15] ^ lfsr[13];
    end else if (mode == 2) begin
      feedback_w = lfsr[15] ^ lfsr[7] ^ lfsr[0];
    end else if (mode == 3) begin
      feedback_w = lfsr[15] ^ lfsr[7];
    end else if (mode == 4) begin
      feedback_w = lfsr[15] ^ lfsr[12] ^ lfsr[1];
    end else if (mode == 5) begin
      feedback_w = lfsr[15] ^ lfsr[11];
    end else if (mode == 6) begin
      feedback_w = lfsr[15] ^ lfsr[2] ^ lfsr[0];
    end else if (mode == 7) begin
      feedback_w = lfsr[15] ^ lfsr[10] ^ lfsr[3];
    end else begin
      feedback_w = lfsr[15] ^ lfsr[0];
    end
    lfsr_next = {lfsr[14:0], feedback_w};
  end
  assign feedback = feedback_w;
  // Replicate 16-bit lfsr_next to fill DATA_WIDTH bits (cyclic: bit i → lfsr_next[i%16])
  logic [DATA_WIDTH-1:0] lfsr_mask;
  always_comb begin
    for (int i = 0; i <= DATA_WIDTH - 1; i++) begin
      lfsr_mask[i +: 1] = lfsr_next[i & 15 +: 1];
    end
  end
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      data_out_r <= 0;
      lfsr <= 16384;
    end else begin
      lfsr <= lfsr_next;
      data_out_r <= data_in ^ lfsr_mask;
    end
  end
  assign data_out = data_out_r;

endmodule

