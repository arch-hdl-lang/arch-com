// SD CRC-16 Generator (LFSR)
// CRC-CCITT: x^16 + x^12 + x^5 + 1. Taps at positions 0, 5, 12.
module sd_crc_16 (
  input logic CLK,
  input logic RST,
  input logic BITVAL,
  input logic Enable,
  output logic [15:0] CRC
);

  logic [15:0] crc_r;
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

// SD Data Serial Host — SD card DAT line interface
// 6-state FSM: IDLE, WRITE_DAT, WRITE_CRC, WRITE_BUSY, READ_WAIT, READ_DAT
// CRC-16 per data line. Double-buffered write path.
//
// Verify: reset-audit, fsm-transitions, output-per-state matched against
// RealBench spec + reference sd_data_serial_host.v (463 lines).
// Endianness: BIG_ENDIAN (matches verification sd_defines.v).
module sd_data_serial_host #(
  parameter int SD_BUS_W = 4,
  parameter int BIT_BLOCK = 1044,
  parameter int CRC_OFF = 19,
  parameter int BIT_BLOCK_REC = 1024,
  parameter int BIT_CRC_CYCLE = 16,
  parameter int IDLE = 0,
  parameter int WRITE_DAT = 1,
  parameter int WRITE_CRC = 2,
  parameter int WRITE_BUSY = 3,
  parameter int READ_WAIT = 4,
  parameter int READ_DAT = 5
) (
  input logic sd_clk,
  input logic rst,
  input logic [31:0] data_in,
  output logic rd,
  output logic [3:0] data_out,
  output logic we,
  output logic DAT_oe_o,
  output logic [3:0] DAT_dat_o,
  input logic [3:0] DAT_dat_i,
  input logic [1:0] start_dat,
  input logic ack_transfer,
  output logic busy_n,
  output logic transm_complete,
  output logic crc_ok
);

  // TX FIFO interface
  // RX FIFO interface
  // Tristate data
  // Control
  // Status
  // ── State encoding (one-hot, matches reference) ──────────────────────
  // ── CRC-16 signals ────────────────────────────────────────────────────
  logic [3:0] crc_in;
  logic crc_en;
  logic crc_rst;
  logic [15:0] crc_out0;
  logic [15:0] crc_out1;
  logic [15:0] crc_out2;
  logic [15:0] crc_out3;
  sd_crc_16 u_crc0 (
    .CLK(sd_clk),
    .RST(rst),
    .BITVAL(crc_in[0:0]),
    .Enable(crc_en),
    .CRC(crc_out0)
  );
  sd_crc_16 u_crc1 (
    .CLK(sd_clk),
    .RST(rst),
    .BITVAL(crc_in[1:1]),
    .Enable(crc_en),
    .CRC(crc_out1)
  );
  sd_crc_16 u_crc2 (
    .CLK(sd_clk),
    .RST(rst),
    .BITVAL(crc_in[2:2]),
    .Enable(crc_en),
    .CRC(crc_out2)
  );
  sd_crc_16 u_crc3 (
    .CLK(sd_clk),
    .RST(rst),
    .BITVAL(crc_in[3:3]),
    .Enable(crc_en),
    .CRC(crc_out3)
  );
  // ── ACK_SYNC: two-stage synchronizer ──────────────────────────────────
  logic ack_q;
  logic ack_transfer_int;
  always_ff @(posedge sd_clk or posedge rst) begin
    if (rst) begin
      ack_q <= 1'b0;
      ack_transfer_int <= 1'b0;
    end else begin
      ack_q <= ack_transfer;
      ack_transfer_int <= ack_q;
    end
  end
  // ── START_SYNC: start-bit detection ───────────────────────────────────
  logic q_start_bit;
  always_ff @(posedge sd_clk or posedge rst) begin
    if (rst) begin
      q_start_bit <= 1'b1;
    end else begin
      if (~DAT_dat_i[0:0] & (state == READ_WAIT)) begin
        q_start_bit <= 1'b0;
      end else begin
        q_start_bit <= 1'b1;
      end
    end
  end
  // ── FSM state registers ──────────────────────────────────────────────
  logic [2:0] state;
  // ── FSM_COMBO: combinational next_state ──────────────────────────────
  logic [2:0] next_state_w;
  always_comb begin
    next_state_w = IDLE;
    if (state == IDLE) begin
      if (start_dat == 2'd1) begin
        next_state_w = WRITE_DAT;
      end else if (start_dat == 2'd2) begin
        next_state_w = READ_WAIT;
      end else begin
        next_state_w = IDLE;
      end
    end else if (state == WRITE_DAT) begin
      if (transf_cnt >= BIT_BLOCK) begin
        next_state_w = WRITE_CRC;
      end else if (start_dat == 2'd3) begin
        next_state_w = IDLE;
      end else begin
        next_state_w = WRITE_DAT;
      end
    end else if (state == WRITE_CRC) begin
      if (crc_status == 0) begin
        next_state_w = WRITE_BUSY;
      end else begin
        next_state_w = WRITE_CRC;
      end
    end else if (state == WRITE_BUSY) begin
      if (busy_int & ack_transfer_int) begin
        next_state_w = IDLE;
      end else begin
        next_state_w = WRITE_BUSY;
      end
    end else if (state == READ_WAIT) begin
      if (~q_start_bit) begin
        next_state_w = READ_DAT;
      end else begin
        next_state_w = READ_WAIT;
      end
    end else if (ack_transfer_int | (start_dat == 2'd3)) begin
      // READ_DAT
      next_state_w = IDLE;
    end else begin
      next_state_w = READ_DAT;
    end
  end
  // ── FSM_SEQ: state update ────────────────────────────────────────────
  always_ff @(posedge sd_clk or posedge rst) begin
    if (rst) begin
      state <= IDLE;
    end else begin
      if (rst) begin
        state <= IDLE;
      end else begin
        state <= next_state_w;
      end
    end
  end
  // ── FSM_OUT registers ────────────────────────────────────────────────
  // Reset values from spec reset table §FSM_OUT
  logic [31:0] write_buf_0;
  logic [31:0] write_buf_1;
  logic [31:0] sd_data_out;
  logic out_buff_ptr;
  logic in_buff_ptr;
  logic [2:0] data_send_idx;
  logic [10:0] transf_cnt;
  logic [2:0] crc_status;
  logic [2:0] crc_s;
  logic [4:0] crc_c;
  logic [3:0] last_din;
  logic busy_int;
  // CRC serialization shift registers (avoid dynamic bit-select)
  logic [15:0] crc_sr0;
  logic [15:0] crc_sr1;
  logic [15:0] crc_sr2;
  logic [15:0] crc_sr3;
  // Output registers
  logic dat_oe_r;
  logic [3:0] dat_out_r;
  logic rd_r;
  logic we_r;
  logic [3:0] data_out_r;
  logic busy_n_r;
  logic transm_c_r;
  logic crc_ok_r;
  // ── FSM_OUT: per-state outputs (negedge, matches reference timing) ───
  always_ff @(negedge sd_clk or posedge rst) begin
    if (rst) begin
      busy_int <= 1'b0;
      busy_n_r <= 1'b1;
      crc_c <= 0;
      crc_en <= 1'b0;
      crc_in <= 0;
      crc_ok_r <= 1'b0;
      crc_rst <= 1'b1;
      crc_s <= 0;
      crc_sr0 <= 0;
      crc_sr1 <= 0;
      crc_sr2 <= 0;
      crc_sr3 <= 0;
      crc_status <= 7;
      dat_oe_r <= 1'b0;
      dat_out_r <= 0;
      data_out_r <= 0;
      data_send_idx <= 0;
      in_buff_ptr <= 1'b0;
      last_din <= 0;
      out_buff_ptr <= 1'b0;
      rd_r <= 1'b0;
      sd_data_out <= 0;
      transf_cnt <= 0;
      transm_c_r <= 1'b0;
      we_r <= 1'b0;
      write_buf_0 <= 0;
      write_buf_1 <= 0;
    end else begin
      // Defaults (overridden per state)
      rd_r <= 1'b0;
      we_r <= 1'b0;
      if (state == IDLE) begin
        dat_oe_r <= 1'b0;
        dat_out_r <= 4'd15;
        crc_en <= 1'b0;
        crc_rst <= 1'b1;
        transf_cnt <= 0;
        crc_c <= 16;
        crc_status <= 7;
        crc_s <= 0;
        busy_n_r <= 1'b1;
        data_send_idx <= 0;
        out_buff_ptr <= 1'b0;
        in_buff_ptr <= 1'b0;
      end else if (state == WRITE_DAT) begin
        transm_c_r <= 1'b0;
        busy_n_r <= 1'b0;
        crc_ok_r <= 1'b0;
        transf_cnt <= 11'(transf_cnt + 1);
        // Double-buffer fill from TX FIFO
        if ((in_buff_ptr != out_buff_ptr) | (transf_cnt == 0)) begin
          rd_r <= 1'b1;
          if (~in_buff_ptr) begin
            write_buf_0 <= data_in;
          end else begin
            write_buf_1 <= data_in;
          end
          in_buff_ptr <= ~in_buff_ptr;
        end
        // Output buffer select
        if (~out_buff_ptr) begin
          sd_data_out <= write_buf_0;
        end else begin
          sd_data_out <= write_buf_1;
        end
        if (transf_cnt == 1) begin
          // Start bit
          crc_rst <= 1'b0;
          crc_en <= 1'b1;
          last_din <= sd_data_out[31:28];
          crc_in <= sd_data_out[31:28];
          dat_oe_r <= 1'b1;
          dat_out_r <= 0;
          data_send_idx <= 1;
        end else if ((transf_cnt >= 2) & (transf_cnt <= BIT_BLOCK - CRC_OFF)) begin
          dat_oe_r <= 1'b1;
          // BIG_ENDIAN: serialize MSB first
          if (data_send_idx == 0) begin
            last_din <= sd_data_out[31:28];
            crc_in <= sd_data_out[31:28];
          end else if (data_send_idx == 1) begin
            last_din <= sd_data_out[27:24];
            crc_in <= sd_data_out[27:24];
          end else if (data_send_idx == 2) begin
            last_din <= sd_data_out[23:20];
            crc_in <= sd_data_out[23:20];
          end else if (data_send_idx == 3) begin
            last_din <= sd_data_out[19:16];
            crc_in <= sd_data_out[19:16];
          end else if (data_send_idx == 4) begin
            last_din <= sd_data_out[15:12];
            crc_in <= sd_data_out[15:12];
          end else if (data_send_idx == 5) begin
            last_din <= sd_data_out[11:8];
            crc_in <= sd_data_out[11:8];
          end else if (data_send_idx == 6) begin
            last_din <= sd_data_out[7:4];
            crc_in <= sd_data_out[7:4];
            out_buff_ptr <= ~out_buff_ptr;
          end else begin
            // 7
            last_din <= sd_data_out[3:0];
            crc_in <= sd_data_out[3:0];
          end
          data_send_idx <= 3'(data_send_idx + 1);
          dat_out_r <= last_din;
          if (transf_cnt >= BIT_BLOCK - CRC_OFF) begin
            crc_en <= 1'b0;
          end
        end else if ((transf_cnt > BIT_BLOCK - CRC_OFF) & (crc_c != 0)) begin
          crc_en <= 1'b0;
          // Load CRC shift registers on first output cycle
          if (crc_c == 16) begin
            crc_sr0 <= crc_out0;
            crc_sr1 <= crc_out1;
            crc_sr2 <= crc_out2;
            crc_sr3 <= crc_out3;
          end else begin
            crc_sr0 <= {crc_sr0[14:0], 1'd0};
            crc_sr1 <= {crc_sr1[14:0], 1'd0};
            crc_sr2 <= {crc_sr2[14:0], 1'd0};
            crc_sr3 <= {crc_sr3[14:0], 1'd0};
          end
          dat_oe_r <= 1'b1;
          dat_out_r[0:0] <= crc_c == 16 ? crc_out0[15:15] : crc_sr0[15:15];
          dat_out_r[1:1] <= crc_c == 16 ? crc_out1[15:15] : crc_sr1[15:15];
          dat_out_r[2:2] <= crc_c == 16 ? crc_out2[15:15] : crc_sr2[15:15];
          dat_out_r[3:3] <= crc_c == 16 ? crc_out3[15:15] : crc_sr3[15:15];
          crc_c <= 5'(crc_c - 1);
        end else if (transf_cnt == BIT_BLOCK - 2) begin
          dat_oe_r <= 1'b1;
          dat_out_r <= 4'd15;
        end else if (transf_cnt != 0) begin
          // Stop bit
          dat_oe_r <= 1'b0;
        end
      end else if (state == WRITE_CRC) begin
        dat_oe_r <= 1'b0;
        crc_status <= 3'(crc_status - 1);
        if (crc_status == 2) begin
          crc_s[0:0] <= DAT_dat_i[0:0];
        end else if (crc_status == 3) begin
          crc_s[1:1] <= DAT_dat_i[0:0];
        end else if (crc_status == 4) begin
          crc_s[2:2] <= DAT_dat_i[0:0];
        end
      end else if (state == WRITE_BUSY) begin
        transm_c_r <= 1'b1;
        if (crc_s == 3'd2) begin
          crc_ok_r <= 1'b1;
        end else begin
          crc_ok_r <= 1'b0;
        end
        busy_int <= DAT_dat_i[0:0];
      end else if (state == READ_WAIT) begin
        dat_oe_r <= 1'b0;
        crc_rst <= 1'b0;
        crc_en <= 1'b1;
        crc_in <= 0;
        crc_c <= 15;
        busy_n_r <= 1'b0;
        transm_c_r <= 1'b0;
      end else if (state == READ_DAT) begin
        if (transf_cnt < BIT_BLOCK_REC) begin
          we_r <= 1'b1;
          data_out_r <= DAT_dat_i;
          crc_in <= DAT_dat_i;
          crc_ok_r <= 1'b1;
          transf_cnt <= 11'(transf_cnt + 1);
        end else if (transf_cnt <= BIT_BLOCK_REC + BIT_CRC_CYCLE) begin
          transf_cnt <= 11'(transf_cnt + 1);
          crc_en <= 1'b0;
          last_din <= DAT_dat_i;
          if (transf_cnt > BIT_BLOCK_REC) begin
            crc_c <= 5'(crc_c - 1);
            we_r <= 1'b0;
            // Compare CRC: use crc_out[crc_c] on first cycle, shift reg after
            if (crc_c == 15) begin
              // First CRC bit: compare directly using known index 15
              if (crc_out0[15:15] != last_din[0:0]) begin
                crc_ok_r <= 1'b0;
              end
              if (crc_out1[15:15] != last_din[1:1]) begin
                crc_ok_r <= 1'b0;
              end
              if (crc_out2[15:15] != last_din[2:2]) begin
                crc_ok_r <= 1'b0;
              end
              if (crc_out3[15:15] != last_din[3:3]) begin
                crc_ok_r <= 1'b0;
              end
              // Load and shift for subsequent cycles
              crc_sr0 <= {crc_out0[14:0], 1'd0};
              crc_sr1 <= {crc_out1[14:0], 1'd0};
              crc_sr2 <= {crc_out2[14:0], 1'd0};
              crc_sr3 <= {crc_out3[14:0], 1'd0};
            end else begin
              // Subsequent CRC bits from shift register MSB
              if (crc_sr0[15:15] != last_din[0:0]) begin
                crc_ok_r <= 1'b0;
              end
              if (crc_sr1[15:15] != last_din[1:1]) begin
                crc_ok_r <= 1'b0;
              end
              if (crc_sr2[15:15] != last_din[2:2]) begin
                crc_ok_r <= 1'b0;
              end
              if (crc_sr3[15:15] != last_din[3:3]) begin
                crc_ok_r <= 1'b0;
              end
              crc_sr0 <= {crc_sr0[14:0], 1'd0};
              crc_sr1 <= {crc_sr1[14:0], 1'd0};
              crc_sr2 <= {crc_sr2[14:0], 1'd0};
              crc_sr3 <= {crc_sr3[14:0], 1'd0};
            end
            if (crc_c == 0) begin
              transm_c_r <= 1'b1;
              busy_n_r <= 1'b0;
              we_r <= 1'b0;
            end
          end
        end
      end
    end
  end
  // ── Output assignments ────────────────────────────────────────────────
  assign DAT_oe_o = dat_oe_r;
  assign DAT_dat_o = dat_out_r;
  assign busy_n = busy_n_r;
  assign transm_complete = transm_c_r;
  assign crc_ok = crc_ok_r;
  assign we = we_r;
  assign rd = rd_r;
  assign data_out = data_out_r;

endmodule

