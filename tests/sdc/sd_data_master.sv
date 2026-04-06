// SD Data Master (Data Transfer Orchestrator)
// 9-state one-hot FSM. Orchestrates data transfers: reads BDs, constructs
// commands, monitors transfers, handles errors with retry (up to 3 retries).
module sd_data_master (
  input logic clk,
  input logic rst,
  input logic [16-1:0] dat_in_tx,
  input logic [5-1:0] free_tx_bd,
  input logic ack_i_s_tx,
  output logic re_s_tx,
  output logic a_cmp_tx,
  input logic [16-1:0] dat_in_rx,
  input logic [5-1:0] free_rx_bd,
  input logic ack_i_s_rx,
  output logic re_s_rx,
  output logic a_cmp_rx,
  input logic cmd_busy,
  output logic we_req,
  input logic we_ack,
  output logic d_write,
  output logic d_read,
  output logic [32-1:0] cmd_arg,
  output logic [16-1:0] cmd_set,
  input logic cmd_tsf_err,
  input logic [5-1:0] card_status,
  output logic start_tx_fifo,
  output logic start_rx_fifo,
  output logic [32-1:0] sys_adr,
  input logic tx_empt,
  input logic tx_full,
  input logic rx_full,
  input logic busy_n,
  input logic transm_complete,
  input logic crc_ok,
  output logic ack_transfer,
  output logic [8-1:0] Dat_Int_Status,
  input logic Dat_Int_Status_rst,
  output logic CIDAT,
  input logic [2-1:0] transfer_type
);

  // TX BD interface
  // RX BD interface
  // Command interface
  // FIFO controls
  // System address
  // TX/RX FIFO status
  // Data serial host status
  // Interrupt status
  // Card detect / transfer type
  // State encoding:
  // 0=IDLE, 1=GET_TX_BD, 2=GET_RX_BD, 3=SEND_CMD, 4=RECIVE_CMD,
  // 5=DATA_TRANSFER, 6=STOP, 7=STOP_SEND, 8=STOP_RECIVE_CMD
  logic [4-1:0] st_r;
  // BD read data registers (two 16-bit reads per BD)
  logic [16-1:0] bd_adr_lo;
  logic [16-1:0] bd_adr_hi;
  logic [16-1:0] bd_blk;
  // Sub-state counter for BD reading
  logic [3-1:0] sub_cnt;
  // Retry counter
  logic [2-1:0] retry_cnt;
  // Output registers
  logic re_s_tx_r;
  logic re_s_rx_r;
  logic a_cmp_tx_r;
  logic a_cmp_rx_r;
  logic we_req_r;
  logic d_write_r;
  logic d_read_r;
  logic [32-1:0] cmd_arg_r;
  logic [16-1:0] cmd_set_r;
  logic start_tx_fifo_r;
  logic start_rx_fifo_r;
  logic [32-1:0] sys_adr_r;
  logic ack_transfer_r;
  logic [8-1:0] dat_int_r;
  logic cidat_r;
  // Is TX transfer?
  logic is_tx;
  assign is_tx = transfer_type == 2'd1;
  // Main FSM
  always_ff @(posedge clk or posedge rst) begin
    if (rst) begin
      a_cmp_rx_r <= 1'b0;
      a_cmp_tx_r <= 1'b0;
      ack_transfer_r <= 1'b0;
      bd_adr_hi <= 0;
      bd_adr_lo <= 0;
      cidat_r <= 1'b0;
      cmd_arg_r <= 0;
      d_read_r <= 1'b0;
      d_write_r <= 1'b0;
      dat_int_r <= 0;
      re_s_rx_r <= 1'b0;
      re_s_tx_r <= 1'b0;
      retry_cnt <= 0;
      st_r <= 0;
      start_rx_fifo_r <= 1'b0;
      start_tx_fifo_r <= 1'b0;
      sub_cnt <= 0;
      sys_adr_r <= 0;
      we_req_r <= 1'b0;
    end else begin
      re_s_tx_r <= 1'b0;
      re_s_rx_r <= 1'b0;
      we_req_r <= 1'b0;
      ack_transfer_r <= 1'b0;
      if (st_r == 0) begin
        // IDLE
        d_write_r <= 1'b0;
        d_read_r <= 1'b0;
        start_tx_fifo_r <= 1'b0;
        start_rx_fifo_r <= 1'b0;
        a_cmp_tx_r <= 1'b0;
        a_cmp_rx_r <= 1'b0;
        cidat_r <= 1'b0;
        retry_cnt <= 0;
        sub_cnt <= 0;
        if (is_tx & free_tx_bd > 0) begin
          st_r <= 1;
          // GET_TX_BD
          re_s_tx_r <= 1'b1;
          sub_cnt <= 0;
        end else if (~is_tx & free_rx_bd > 0) begin
          st_r <= 2;
          // GET_RX_BD
          re_s_rx_r <= 1'b1;
          sub_cnt <= 0;
        end
      end else if (st_r == 1) begin
        // GET_TX_BD
        if (sub_cnt == 0) begin
          // First read: system address low
          bd_adr_lo <= dat_in_tx;
          re_s_tx_r <= 1'b1;
          sub_cnt <= 1;
        end else if (sub_cnt == 1) begin
          // Second read: system address high
          bd_adr_hi <= dat_in_tx;
          sub_cnt <= 2;
        end else begin
          sys_adr_r <= {bd_adr_hi, bd_adr_lo};
          cmd_arg_r <= {bd_adr_hi, bd_adr_lo};
          st_r <= 3;
          // SEND_CMD
          we_req_r <= 1'b1;
          d_write_r <= 1'b1;
        end
      end else if (st_r == 2) begin
        // GET_RX_BD
        if (sub_cnt == 0) begin
          bd_adr_lo <= dat_in_rx;
          re_s_rx_r <= 1'b1;
          sub_cnt <= 1;
        end else if (sub_cnt == 1) begin
          bd_adr_hi <= dat_in_rx;
          sub_cnt <= 2;
        end else begin
          sys_adr_r <= {bd_adr_hi, bd_adr_lo};
          cmd_arg_r <= {bd_adr_hi, bd_adr_lo};
          st_r <= 3;
          // SEND_CMD
          we_req_r <= 1'b1;
          d_read_r <= 1'b1;
        end
      end else if (st_r == 3) begin
        // SEND_CMD
        cidat_r <= 1'b1;
        if (we_ack) begin
          st_r <= 4;
        end
      end else if (st_r == 4) begin
        // RECIVE_CMD
        // RECIVE_CMD
        if (~cmd_busy) begin
          if (cmd_tsf_err) begin
            dat_int_r[0:0] <= 1'd1;
            // Command error
            st_r <= 0;
            // IDLE
            cidat_r <= 1'b0;
          end else begin
            st_r <= 5;
            // DATA_TRANSFER
            if (d_write_r) begin
              start_tx_fifo_r <= 1'b1;
            end else begin
              start_rx_fifo_r <= 1'b1;
            end
          end
        end
      end else if (st_r == 5) begin
        // DATA_TRANSFER
        if (transm_complete) begin
          ack_transfer_r <= 1'b1;
          if (crc_ok) begin
            dat_int_r[2:2] <= 1'd1;
            // Transfer complete
            if (d_write_r) begin
              a_cmp_tx_r <= 1'b1;
            end else begin
              a_cmp_rx_r <= 1'b1;
            end
            st_r <= 0;
            // IDLE
            cidat_r <= 1'b0;
          end else begin
            // CRC error
            dat_int_r[5:5] <= 1'd1;
            if (retry_cnt < 3) begin
              retry_cnt <= 2'(retry_cnt + 1);
              st_r <= 3;
              // Retry: SEND_CMD
              we_req_r <= 1'b1;
            end else begin
              st_r <= 0;
              // IDLE
              cidat_r <= 1'b0;
            end
          end
        end
      end else if (st_r == 6) begin
        // STOP
        we_req_r <= 1'b1;
        st_r <= 7;
      end else if (st_r == 7) begin
        // STOP_SEND
        if (we_ack) begin
          st_r <= 8;
        end
      end else if (st_r == 8) begin
        // STOP_RECIVE_CMD
        if (~cmd_busy) begin
          st_r <= 0;
          cidat_r <= 1'b0;
        end
      end
      // Clear interrupt status on reset signal
      if (Dat_Int_Status_rst) begin
        dat_int_r <= 0;
      end
    end
  end
  // Output assignments
  assign re_s_tx = re_s_tx_r;
  assign re_s_rx = re_s_rx_r;
  assign a_cmp_tx = a_cmp_tx_r;
  assign a_cmp_rx = a_cmp_rx_r;
  assign we_req = we_req_r;
  assign d_write = d_write_r;
  assign d_read = d_read_r;
  assign cmd_arg = cmd_arg_r;
  assign cmd_set = cmd_set_r;
  assign start_tx_fifo = start_tx_fifo_r;
  assign start_rx_fifo = start_rx_fifo_r;
  assign sys_adr = sys_adr_r;
  assign ack_transfer = ack_transfer_r;
  assign Dat_Int_Status = dat_int_r;
  assign CIDAT = cidat_r;

endmodule

