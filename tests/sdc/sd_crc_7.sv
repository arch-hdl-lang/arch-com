// SD CRC-7 Generator (LFSR)
// Polynomial: x^7 + x^3 + 1. Taps at positions 0 and 3.
module sd_crc_7 (
  input logic CLK,
  input logic RST,
  input logic BITVAL,
  input logic Enable,
  output logic [7-1:0] CRC
);

  logic [7-1:0] crc_r;
  logic inv;
  assign inv = BITVAL ^ crc_r[6:6];
  always_ff @(posedge CLK or posedge RST) begin
    if (RST) begin
      crc_r <= 0;
    end else begin
      if (Enable) begin
        crc_r[6:6] <= crc_r[5:5];
        crc_r[5:5] <= crc_r[4:4];
        crc_r[4:4] <= crc_r[3:3];
        crc_r[3:3] <= crc_r[2:2] ^ inv;
        crc_r[2:2] <= crc_r[1:1];
        crc_r[1:1] <= crc_r[0:0];
        crc_r[0:0] <= inv;
      end
    end
  end
  assign CRC = crc_r;

endmodule

