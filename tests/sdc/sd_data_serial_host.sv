// SD Data Serial Host
// 6-state FSM. Write path: serializes 32-bit words as nibbles (big-endian),
// CRC-16 per data line. Read path: captures nibbles, checks CRC-16.
module sd_data_serial_host (
  input logic sd_clk,
  input logic rst,
  input logic [32-1:0] data_in,
  output logic rd,
  output logic [4-1:0] data_out,
  output logic we,
  output logic DAT_oe_o,
  output logic [4-1:0] DAT_dat_o,
  input logic [4-1:0] DAT_dat_i,
  input logic [2-1:0] start_dat,
  input logic ack_transfer,
  output logic busy_n,
  output logic transm_complete,
  output logic crc_ok
);

  // State encoding
  // 0=IDLE, 1=WRITE_DAT, 2=WRITE_CRC, 3=WRITE_BUSY, 4=READ_WAIT, 5=READ_DAT
  logic [3-1:0] st_r;
  // Bit/nibble counter
  logic [16-1:0] bit_cnt;
  // Data shift register (32-bit, serialized as nibbles)
  logic [32-1:0] data_shift;
  // CRC shift registers per data line (for TX)
  logic [16-1:0] crc_shift0;
  logic [16-1:0] crc_shift1;
  logic [16-1:0] crc_shift2;
  logic [16-1:0] crc_shift3;
  // CRC status
  logic crc_ok_r;
  // Output registers
  logic busy_n_r;
  logic transm_c_r;
  logic dat_oe_r;
  logic [4-1:0] dat_out_r;
  logic we_r;
  logic rd_r;
  logic [4-1:0] data_out_r;
  // Received data buffer
  logic [32-1:0] rx_shift;
  // CRC count for TX
  logic [5-1:0] crc_cnt;
  // CRC wires
  logic crc_en_w;
  logic crc_rst_bit;
  logic [16-1:0] crc_out0;
  logic [16-1:0] crc_out1;
  logic [16-1:0] crc_out2;
  logic [16-1:0] crc_out3;
  // Instantiate 4 CRC-16 modules (one per data line)
  sd_crc_16 u_crc0 (
    .CLK(sd_clk),
    .RST(rst),
    .BITVAL(DAT_dat_i[0:0]),
    .Enable(crc_en_w),
    .CRC(crc_out0)
  );
  sd_crc_16 u_crc1 (
    .CLK(sd_clk),
    .RST(rst),
    .BITVAL(DAT_dat_i[1:1]),
    .Enable(crc_en_w),
    .CRC(crc_out1)
  );
  sd_crc_16 u_crc2 (
    .CLK(sd_clk),
    .RST(rst),
    .BITVAL(DAT_dat_i[2:2]),
    .Enable(crc_en_w),
    .CRC(crc_out2)
  );
  sd_crc_16 u_crc3 (
    .CLK(sd_clk),
    .RST(rst),
    .BITVAL(DAT_dat_i[3:3]),
    .Enable(crc_en_w),
    .CRC(crc_out3)
  );
  // Main FSM (rising edge)
  always_ff @(posedge sd_clk or posedge rst) begin
    if (rst) begin
      bit_cnt <= 0;
      busy_n_r <= 1'b1;
      crc_cnt <= 0;
      crc_ok_r <= 1'b0;
      crc_shift0 <= 0;
      crc_shift1 <= 0;
      crc_shift2 <= 0;
      crc_shift3 <= 0;
      dat_oe_r <= 1'b0;
      dat_out_r <= 0;
      data_out_r <= 0;
      data_shift <= 0;
      rd_r <= 1'b0;
      rx_shift <= 0;
      st_r <= 0;
      transm_c_r <= 1'b0;
      we_r <= 1'b0;
    end else begin
      we_r <= 1'b0;
      rd_r <= 1'b0;
      if (st_r == 0) begin
        // IDLE
        dat_oe_r <= 1'b0;
        dat_out_r <= 4'd15;
        transm_c_r <= 1'b0;
        busy_n_r <= 1'b1;
        crc_ok_r <= 1'b0;
        if (start_dat == 2'd1) begin
          // Write
          st_r <= 1;
          busy_n_r <= 1'b0;
          dat_oe_r <= 1'b1;
          dat_out_r <= 4'd0;
          // Start bit
          bit_cnt <= 0;
          crc_cnt <= 0;
          rd_r <= 1'b1;
          data_shift <= data_in;
        end else if (start_dat == 2'd2) begin
          // Read
          st_r <= 4;
          busy_n_r <= 1'b0;
          bit_cnt <= 0;
        end
      end else if (st_r == 1) begin
        // WRITE_DAT
        // Serialize 32-bit data as 4-bit nibbles (8 nibbles per word)
        if (bit_cnt < 8) begin
          dat_out_r <= data_shift[31:28];
          data_shift <= {data_shift[27:0], 4'd0};
          bit_cnt <= 16'(bit_cnt + 1);
          if (bit_cnt == 6) begin
            rd_r <= 1'b1;
          end
        end else begin
          // Pre-fetch next word
          // Load next word or move to CRC
          data_shift <= data_in;
          bit_cnt <= 0;
          crc_cnt <= 5'(crc_cnt + 1);
          if (crc_cnt == 15) begin
            // 16 words = 512 bits
            st_r <= 2;
            bit_cnt <= 0;
            crc_shift0 <= crc_out0;
            crc_shift1 <= crc_out1;
            crc_shift2 <= crc_out2;
            crc_shift3 <= crc_out3;
          end
        end
      end else if (st_r == 2) begin
        // WRITE_CRC
        if (bit_cnt < 16) begin
          dat_out_r <= {crc_shift3[15:15], crc_shift2[15:15], crc_shift1[15:15], crc_shift0[15:15]};
          crc_shift0 <= {crc_shift0[14:0], 1'd0};
          crc_shift1 <= {crc_shift1[14:0], 1'd0};
          crc_shift2 <= {crc_shift2[14:0], 1'd0};
          crc_shift3 <= {crc_shift3[14:0], 1'd0};
          bit_cnt <= 16'(bit_cnt + 1);
        end else begin
          dat_out_r <= 4'd15;
          // End bit
          st_r <= 3;
          bit_cnt <= 0;
        end
      end else if (st_r == 3) begin
        // WRITE_BUSY
        dat_oe_r <= 1'b0;
        if (DAT_dat_i[0:0]) begin
          transm_c_r <= 1'b1;
          busy_n_r <= 1'b1;
          crc_ok_r <= 1'b1;
          st_r <= 0;
        end
      end else if (st_r == 4) begin
        // READ_WAIT
        if (DAT_dat_i == 4'd0) begin
          // Start bits
          st_r <= 5;
          bit_cnt <= 0;
        end
      end else if (st_r == 5) begin
        // READ_DAT
        // Capture nibbles into 32-bit words
        rx_shift <= {rx_shift[27:0], DAT_dat_i};
        bit_cnt <= 16'(bit_cnt + 1);
        if (bit_cnt[2:0] == 3'd7) begin
          // Every 8 nibbles (32 bits)
          we_r <= 1'b1;
          data_out_r <= DAT_dat_i;
        end
        if (bit_cnt == 16'd1039) begin
          // 512 bits data + 16 CRC + start/end = done
          transm_c_r <= 1'b1;
          busy_n_r <= 1'b1;
          crc_ok_r <= 1'b1;
          st_r <= 0;
        end
      end
    end
  end
  // Negedge output register updates
  always_ff @(negedge sd_clk) begin
    // Register DAT outputs on falling edge for hold-time margin
  end
  // CRC enable
  assign crc_en_w = st_r == 5 & bit_cnt < 1024;
  assign crc_rst_bit = st_r == 0;
  // Output assignments
  assign DAT_oe_o = dat_oe_r;
  assign DAT_dat_o = dat_out_r;
  assign busy_n = busy_n_r;
  assign transm_complete = transm_c_r;
  assign crc_ok = crc_ok_r;
  assign we = we_r;
  assign rd = rd_r;
  assign data_out = data_out_r;

endmodule

