// SD CRC-16 Generator (LFSR)
// CRC-CCITT: x^16 + x^12 + x^5 + 1. Taps at positions 0, 5, 12.
module sd_crc_16 (
  input logic CLK,
  input logic RST,
  input logic BITVAL,
  input logic Enable,
  output logic [16-1:0] CRC
);

  logic [16-1:0] crc_r;
  logic inv;
  assign inv = BITVAL ^ crc_r[15:15];
  always_ff @(posedge CLK or posedge RST) begin
    if (RST) begin
      crc_r <= 0;
    end else begin
      if (Enable) begin
        crc_r[15:15] <= crc_r[14:14];
        crc_r[14:14] <= crc_r[13:13];
        crc_r[13:13] <= crc_r[12:12];
        crc_r[12:12] <= crc_r[11:11] ^ inv;
        crc_r[11:11] <= crc_r[10:10];
        crc_r[10:10] <= crc_r[9:9];
        crc_r[9:9] <= crc_r[8:8];
        crc_r[8:8] <= crc_r[7:7];
        crc_r[7:7] <= crc_r[6:6];
        crc_r[6:6] <= crc_r[5:5];
        crc_r[5:5] <= crc_r[4:4] ^ inv;
        crc_r[4:4] <= crc_r[3:3];
        crc_r[3:3] <= crc_r[2:2];
        crc_r[2:2] <= crc_r[1:1];
        crc_r[1:1] <= crc_r[0:0];
        crc_r[0:0] <= inv;
      end
    end
  end
  assign CRC = crc_r;

endmodule

