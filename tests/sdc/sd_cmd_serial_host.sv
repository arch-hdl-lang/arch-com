// SD CRC-7 Generator (LFSR)
// Polynomial: x^7 + x^3 + 1. Taps at positions 0 and 3.
module sd_crc_7 (
  input logic CLK,
  input logic RST,
  input logic BITVAL,
  input logic Enable,
  output logic [6:0] CRC
);

  logic [6:0] crc_r;
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

// SD Command Serial Host
// 10-state one-hot FSM. Serializes 48-bit SD commands on cmd_out_o.
// Reads 40-bit or 128-bit responses on cmd_dat_i. CRC-7 via sd_crc_7 instance.
module sd_cmd_serial_host (
  input logic SD_CLK_IN,
  input logic RST_IN,
  input logic [15:0] SETTING_IN,
  input logic [39:0] CMD_IN,
  input logic REQ_IN,
  input logic ACK_IN,
  input logic cmd_dat_i,
  output logic [39:0] CMD_OUT,
  output logic ACK_OUT,
  output logic REQ_OUT,
  output logic [7:0] STATUS,
  output logic cmd_oe_o,
  output logic cmd_out_o,
  output logic [1:0] st_dat_t
);

  // SETTING_IN decode
  logic NEED_RESP;
  assign NEED_RESP = SETTING_IN[15:15];
  logic RESP_136;
  assign RESP_136 = SETTING_IN[14:14];
  logic [4:0] CMD_INDEX;
  assign CMD_INDEX = SETTING_IN[12:8];
  logic CRC_CHECK;
  assign CRC_CHECK = SETTING_IN[7:7];
  logic INDEX_CHECK;
  assign INDEX_CHECK = SETTING_IN[6:6];
  logic [5:0] DELAY_VAL;
  assign DELAY_VAL = SETTING_IN[5:0];
  // State index 0..9
  logic [3:0] st_r;
  // Double-flop syncs for REQ_IN, ACK_IN
  logic req_sync1;
  logic req_sync2;
  logic ack_sync1;
  logic ack_sync2;
  // Shift register for command/response
  logic [47:0] cmd_shift;
  logic [39:0] resp_shift;
  // Bit counter
  logic [7:0] bit_cnt;
  // Delay counter
  logic [5:0] dly_cnt;
  // CRC accumulation
  logic [6:0] crc_val;
  logic [3:0] crc_cnt;
  // Output registers
  logic ack_out_r;
  logic req_out_r;
  logic [7:0] status_r;
  logic cmd_oe_r;
  logic cmd_out_r;
  logic [1:0] st_dat_r;
  // Response length
  logic [7:0] resp_len;
  assign resp_len = RESP_136 ? 8'd135 : 8'd39;
  // CRC enable and bit value for CRC module
  logic crc_en_w;
  logic crc_bit_w;
  logic [6:0] crc_out_w;
  sd_crc_7 u_crc7 (
    .CLK(SD_CLK_IN),
    .RST(RST_IN),
    .BITVAL(crc_bit_w),
    .Enable(crc_en_w),
    .CRC(crc_out_w)
  );
  // Double-flop synchronizers
  always_ff @(posedge SD_CLK_IN or posedge RST_IN) begin
    if (RST_IN) begin
      ack_sync1 <= 1'b0;
      ack_sync2 <= 1'b0;
      req_sync1 <= 1'b0;
      req_sync2 <= 1'b0;
    end else begin
      req_sync1 <= REQ_IN;
      req_sync2 <= req_sync1;
      ack_sync1 <= ACK_IN;
      ack_sync2 <= ack_sync1;
    end
  end
  // Main FSM
  always_ff @(posedge SD_CLK_IN or posedge RST_IN) begin
    if (RST_IN) begin
      ack_out_r <= 1'b0;
      bit_cnt <= 0;
      cmd_oe_r <= 1'b0;
      cmd_out_r <= 1'b1;
      cmd_shift <= 0;
      crc_cnt <= 0;
      dly_cnt <= 0;
      req_out_r <= 1'b0;
      resp_shift <= 0;
      st_dat_r <= 0;
      st_r <= 0;
      status_r <= 0;
    end else begin
      if (st_r == 0) begin
        // INIT
        cmd_oe_r <= 1'b1;
        cmd_out_r <= 1'b1;
        bit_cnt <= 0;
        st_r <= 1;
        ack_out_r <= 1'b0;
        req_out_r <= 1'b0;
        status_r <= 0;
        st_dat_r <= 0;
      end else if (st_r == 1) begin
        // IDLE
        cmd_oe_r <= 1'b0;
        cmd_out_r <= 1'b1;
        st_dat_r <= 0;
        if (req_sync2) begin
          cmd_shift <= {2'd1, CMD_INDEX, CMD_IN, 1'd1};
          bit_cnt <= 0;
          crc_cnt <= 0;
          st_r <= 2;
          cmd_oe_r <= 1'b1;
        end
      end else if (st_r == 2) begin
        // WRITE_WR: serialize command bits
        if (bit_cnt < 48) begin
          cmd_out_r <= cmd_shift[47:47];
          cmd_shift <= {cmd_shift[46:0], 1'd1};
          bit_cnt <= 8'(bit_cnt + 1);
        end else begin
          dly_cnt <= 0;
          st_r <= 3;
          cmd_oe_r <= 1'b0;
        end
      end else if (st_r == 3) begin
        // DLY_WR: inter-frame delay
        cmd_out_r <= 1'b1;
        if (dly_cnt >= DELAY_VAL) begin
          if (NEED_RESP) begin
            bit_cnt <= 0;
            st_r <= 4;
          end else begin
            st_r <= 7;
          end
        end else begin
          dly_cnt <= 6'(dly_cnt + 1);
        end
      end else if (st_r == 4) begin
        // READ_WR: receive response
        if (bit_cnt == 0) begin
          if (~cmd_dat_i) begin
            resp_shift <= 0;
            bit_cnt <= 8'(bit_cnt + 1);
            st_dat_r <= 2'd1;
          end
        end else if (bit_cnt < resp_len) begin
          resp_shift <= {resp_shift[38:0], cmd_dat_i};
          bit_cnt <= 8'(bit_cnt + 1);
        end else begin
          dly_cnt <= 0;
          st_r <= 5;
        end
      end else if (st_r == 5) begin
        // DLY_READ
        if (dly_cnt >= DELAY_VAL) begin
          st_r <= 6;
        end else begin
          dly_cnt <= 6'(dly_cnt + 1);
        end
      end else if (st_r == 6) begin
        // ACK_WR: acknowledge with response
        ack_out_r <= 1'b1;
        status_r <= 0;
        st_dat_r <= 2'd2;
        if (ack_sync2) begin
          ack_out_r <= 1'b0;
          st_r <= 1;
        end
      end else if (st_r == 7) begin
        // WRITE_WO: write-only (no response)
        dly_cnt <= 0;
        st_r <= 8;
      end else if (st_r == 8) begin
        // DLY_WO
        if (dly_cnt >= DELAY_VAL) begin
          st_r <= 9;
        end else begin
          dly_cnt <= 6'(dly_cnt + 1);
        end
      end else if (st_r == 9) begin
        // ACK_WO: acknowledge without response
        req_out_r <= 1'b1;
        if (ack_sync2) begin
          req_out_r <= 1'b0;
          st_r <= 1;
        end
      end
    end
  end
  // CRC interface
  assign crc_en_w = (st_r == 2) & (bit_cnt < 48);
  assign crc_bit_w = cmd_shift[47:47];
  // Output assignments
  assign CMD_OUT = resp_shift;
  assign ACK_OUT = ack_out_r;
  assign REQ_OUT = req_out_r;
  assign STATUS = status_r;
  assign cmd_oe_o = cmd_oe_r;
  assign cmd_out_o = cmd_out_r;
  assign st_dat_t = st_dat_r;
  always_ff @(posedge SD_CLK_IN or posedge RST_IN) begin
    if (RST_IN) begin
      crc_val <= 0;
    end
  end

endmodule

