module scrambler #(
  parameter int DATA_WIDTH = 128,
  localparam int NUM_CHUNKS = DATA_WIDTH / 16
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
  logic feedback_w;
  logic [16-1:0] lfsr_next;
  // Polynomial selection
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
  // Replicate 16-bit lfsr (registered value) to fill DATA_WIDTH bits
  logic [DATA_WIDTH-1:0] lfsr_mask;
  always_comb begin
    for (int i = 0; i <= NUM_CHUNKS - 1; i++) begin
      lfsr_mask[i * 16 +: 16] = lfsr;
    end
  end
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      lfsr <= 16384;
    end else begin
      lfsr <= lfsr_next;
    end
  end
  // Combinational output: XOR data_in with current lfsr mask
  assign data_out = data_in ^ lfsr_mask;

endmodule

