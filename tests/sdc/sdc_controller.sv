// SDC Controller (Top-level integration)
// Instantiates all sub-modules with port interfaces matching the OpenCores
// SDC reference Verilog. WB master mux, status register pipeline, IRQ generation.
module sdc_controller (
  input logic wb_clk_i,
  input logic wb_rst_i,
  input logic [32-1:0] wb_dat_i,
  output logic [32-1:0] wb_dat_o,
  input logic [8-1:0] wb_adr_i,
  input logic [4-1:0] wb_sel_i,
  input logic wb_we_i,
  input logic wb_cyc_i,
  input logic wb_stb_i,
  output logic wb_ack_o,
  output logic [32-1:0] m_wb_adr_o,
  output logic [4-1:0] m_wb_sel_o,
  output logic m_wb_we_o,
  output logic [32-1:0] m_wb_dat_o,
  input logic [32-1:0] m_wb_dat_i,
  output logic m_wb_cyc_o,
  output logic m_wb_stb_o,
  input logic m_wb_ack_i,
  output logic [3-1:0] m_wb_cti_o,
  output logic [2-1:0] m_wb_bte_o,
  input logic sd_cmd_dat_i,
  output logic sd_cmd_out_o,
  output logic sd_cmd_oe_o,
  input logic card_detect,
  input logic [4-1:0] sd_dat_dat_i,
  output logic [4-1:0] sd_dat_out_o,
  output logic sd_dat_oe_o,
  output logic sd_clk_o_pad,
  output logic int_a,
  output logic int_b,
  output logic int_c
);

  // Wishbone slave
  // Wishbone master
  // SD card interface
  // Interrupts
  // ---- Internal wires ----
  // Clock divider
  logic [8-1:0] clock_divider_w;
  logic sd_clk_w;
  // Controller WB outputs
  logic new_cmd_w;
  logic d_write_w;
  logic d_read_w;
  logic we_ack_w;
  logic int_ack_w;
  logic cmd_int_busy_w;
  logic int_busy_w;
  logic we_m_tx_bd_w;
  logic we_m_rx_bd_w;
  logic Bd_isr_reset_w;
  logic normal_isr_reset_w;
  logic error_isr_reset_w;
  logic [16-1:0] dat_in_m_tx_bd_w;
  logic [16-1:0] dat_in_m_rx_bd_w;
  logic [32-1:0] argument_reg_w;
  logic [16-1:0] cmd_setting_reg_w;
  logic [8-1:0] software_reset_reg_w;
  logic [16-1:0] time_out_reg_w;
  logic [16-1:0] normal_int_signal_enable_reg_w;
  logic [16-1:0] error_int_signal_enable_reg_w;
  logic [8-1:0] Bd_isr_enable_reg_w;
  // Command master outputs
  logic [16-1:0] status_reg_cm;
  logic [32-1:0] cmd_resp_1_cm;
  logic [5-1:0] err_int_cm;
  logic [16-1:0] normal_int_cm;
  logic [16-1:0] settings_w;
  logic go_idle_w;
  logic [40-1:0] cmd_out_master;
  logic req_out_master;
  logic ack_out_master;
  // Command serial host outputs
  logic [40-1:0] cmd_in_host;
  logic ack_in_host;
  logic req_in_host;
  logic [8-1:0] serial_status_w;
  logic cs_cmd_oe_w;
  logic cs_cmd_out_w;
  logic [2-1:0] st_dat_t_w;
  // Data master outputs
  logic dm_re_s_tx_w;
  logic dm_a_cmp_tx_w;
  logic dm_re_s_rx_w;
  logic dm_a_cmp_rx_w;
  logic dm_we_req_w;
  logic dm_d_write_w;
  logic dm_d_read_w;
  logic [32-1:0] dm_cmd_arg_w;
  logic [16-1:0] dm_cmd_set_w;
  logic dm_start_tx_fifo_w;
  logic dm_start_rx_fifo_w;
  logic [32-1:0] dm_sys_adr_w;
  logic dm_ack_transfer_w;
  logic [8-1:0] dm_dat_int_w;
  logic dm_cidat_w;
  // Data serial host outputs
  logic ds_rd_w;
  logic [4-1:0] ds_data_out_w;
  logic ds_we_w;
  logic ds_dat_oe_w;
  logic [4-1:0] ds_dat_out_w;
  logic ds_busy_n_w;
  logic ds_transm_w;
  logic ds_crc_ok_w;
  // TX BD
  logic [16-1:0] tx_bd_dat_out_w;
  logic [5-1:0] tx_bd_free_w;
  logic tx_bd_ack_w;
  // RX BD
  logic [16-1:0] rx_bd_dat_out_w;
  logic [5-1:0] rx_bd_free_w;
  logic rx_bd_ack_w;
  // TX filler (contains internal TX FIFO)
  logic [32-1:0] txf_q_w;
  logic txf_full_w;
  logic txf_empty_w;
  logic [32-1:0] txf_adr_w;
  logic txf_we_w;
  logic txf_cyc_w;
  logic txf_stb_w;
  logic [3-1:0] txf_cti_w;
  logic [2-1:0] txf_bte_w;
  // RX filler (contains internal RX FIFO)
  logic rxf_full_w;
  logic rxf_empty_w;
  logic [32-1:0] rxf_adr_w;
  logic rxf_we_w;
  logic [32-1:0] rxf_dato_w;
  logic rxf_cyc_w;
  logic rxf_stb_w;
  logic [3-1:0] rxf_cti_w;
  logic [2-1:0] rxf_bte_w;
  // Status registers (pipelined)
  logic [16-1:0] status_reg_r;
  logic [32-1:0] cmd_resp_1_r;
  logic [16-1:0] normal_int_status_reg_r;
  logic [16-1:0] error_int_status_reg_r;
  logic [16-1:0] Bd_Status_reg_r;
  logic [8-1:0] Bd_isr_reg_r;
  // write_req_s: data master wants CMD bus access
  logic write_req_s_w;
  logic [16-1:0] cmd_set_s_w;
  logic [32-1:0] cmd_arg_s_w;
  logic cmd_busy_w;
  // --- Clock divider ---
  sd_clock_divider u_clk_div (
    .CLK(wb_clk_i),
    .RST(wb_rst_i),
    .DIVIDER(clock_divider_w),
    .SD_CLK(sd_clk_w)
  );
  // --- Controller WB ---
  sd_controller_wb u_wb (
    .wb_clk_i(wb_clk_i),
    .wb_rst_i(wb_rst_i),
    .wb_dat_i(wb_dat_i),
    .wb_dat_o(wb_dat_o),
    .wb_adr_i(wb_adr_i),
    .wb_sel_i(wb_sel_i),
    .wb_we_i(wb_we_i),
    .wb_cyc_i(wb_cyc_i),
    .wb_stb_i(wb_stb_i),
    .wb_ack_o(wb_ack_o),
    .we_m_tx_bd(we_m_tx_bd_w),
    .new_cmd(new_cmd_w),
    .we_ack(we_ack_w),
    .int_ack(int_ack_w),
    .cmd_int_busy(cmd_int_busy_w),
    .we_m_rx_bd(we_m_rx_bd_w),
    .int_busy(int_busy_w),
    .write_req_s(write_req_s_w),
    .cmd_set_s(cmd_set_s_w),
    .cmd_arg_s(cmd_arg_s_w),
    .argument_reg(argument_reg_w),
    .cmd_setting_reg(cmd_setting_reg_w),
    .software_reset_reg(software_reset_reg_w),
    .time_out_reg(time_out_reg_w),
    .normal_int_signal_enable_reg(normal_int_signal_enable_reg_w),
    .error_int_signal_enable_reg(error_int_signal_enable_reg_w),
    .clock_divider(clock_divider_w),
    .Bd_isr_enable_reg(Bd_isr_enable_reg_w),
    .status_reg(status_reg_r),
    .cmd_resp_1(cmd_resp_1_r),
    .normal_int_status_reg(normal_int_status_reg_r),
    .error_int_status_reg(error_int_status_reg_r),
    .Bd_Status_reg(Bd_Status_reg_r),
    .Bd_isr_reg(Bd_isr_reg_r),
    .Bd_isr_reset(Bd_isr_reset_w),
    .normal_isr_reset(normal_isr_reset_w),
    .error_isr_reset(error_isr_reset_w),
    .dat_in_m_rx_bd(dat_in_m_rx_bd_w),
    .dat_in_m_tx_bd(dat_in_m_tx_bd_w)
  );
  // --- Command Master ---
  // Reset = wb_rst_i | software_reset_reg_w[0] in reference.
  // We connect the base reset; software reset would need OR'd externally.
  sd_cmd_master u_cmd_master (
    .CLK_PAD_IO(wb_clk_i),
    .RST_PAD_I(wb_rst_i),
    .New_CMD(new_cmd_w),
    .data_write(dm_d_write_w),
    .data_read(dm_d_read_w),
    .ARG_REG(argument_reg_w),
    .CMD_SET_REG(cmd_setting_reg_w[13:0]),
    .TIMEOUT_REG(time_out_reg_w),
    .STATUS_REG(status_reg_cm),
    .RESP_1_REG(cmd_resp_1_cm),
    .ERR_INT_REG(err_int_cm),
    .NORMAL_INT_REG(normal_int_cm),
    .ERR_INT_RST(error_isr_reset_w),
    .NORMAL_INT_RST(normal_isr_reset_w),
    .settings(settings_w),
    .go_idle_o(go_idle_w),
    .cmd_out(cmd_out_master),
    .req_out(req_out_master),
    .ack_out(ack_out_master),
    .req_in(req_in_host),
    .ack_in(ack_in_host),
    .cmd_in(cmd_in_host),
    .serial_status(serial_status_w),
    .card_detect(card_detect)
  );
  // --- Command Serial Host ---
  sd_cmd_serial_host u_cmd_serial (
    .SD_CLK_IN(wb_clk_i),
    .RST_IN(wb_rst_i),
    .SETTING_IN(settings_w),
    .CMD_IN(cmd_out_master),
    .REQ_IN(req_out_master),
    .ACK_IN(ack_out_master),
    .cmd_dat_i(sd_cmd_dat_i),
    .CMD_OUT(cmd_in_host),
    .ACK_OUT(ack_in_host),
    .REQ_OUT(req_in_host),
    .STATUS(serial_status_w),
    .cmd_oe_o(cs_cmd_oe_w),
    .cmd_out_o(cs_cmd_out_w),
    .st_dat_t(st_dat_t_w)
  );
  // --- Data Master ---
  sd_data_master u_data_master (
    .clk(wb_clk_i),
    .rst(wb_rst_i),
    .dat_in_tx(tx_bd_dat_out_w),
    .free_tx_bd(tx_bd_free_w),
    .ack_i_s_tx(tx_bd_ack_w),
    .re_s_tx(dm_re_s_tx_w),
    .a_cmp_tx(dm_a_cmp_tx_w),
    .dat_in_rx(rx_bd_dat_out_w),
    .free_rx_bd(rx_bd_free_w),
    .ack_i_s_rx(rx_bd_ack_w),
    .re_s_rx(dm_re_s_rx_w),
    .a_cmp_rx(dm_a_cmp_rx_w),
    .cmd_busy(cmd_busy_w),
    .we_req(write_req_s_w),
    .we_ack(we_ack_w),
    .d_write(dm_d_write_w),
    .d_read(dm_d_read_w),
    .cmd_arg(cmd_arg_s_w),
    .cmd_set(cmd_set_s_w),
    .cmd_tsf_err(normal_int_status_reg_r[15]),
    .card_status(cmd_resp_1_r[12:8]),
    .start_tx_fifo(dm_start_tx_fifo_w),
    .start_rx_fifo(dm_start_rx_fifo_w),
    .sys_adr(dm_sys_adr_w),
    .tx_empt(txf_empty_w),
    .tx_full(txf_full_w),
    .rx_full(rxf_full_w),
    .busy_n(ds_busy_n_w),
    .transm_complete(ds_transm_w),
    .crc_ok(ds_crc_ok_w),
    .ack_transfer(dm_ack_transfer_w),
    .Dat_Int_Status(dm_dat_int_w),
    .Dat_Int_Status_rst(Bd_isr_reset_w),
    .CIDAT(dm_cidat_w),
    .transfer_type(cmd_setting_reg_w[15:14])
  );
  // --- Data Serial Host ---
  sd_data_serial_host u_data_serial (
    .sd_clk(wb_clk_i),
    .rst(wb_rst_i),
    .data_in(txf_q_w),
    .rd(ds_rd_w),
    .data_out(ds_data_out_w),
    .we(ds_we_w),
    .DAT_oe_o(ds_dat_oe_w),
    .DAT_dat_o(ds_dat_out_w),
    .DAT_dat_i(sd_dat_dat_i),
    .start_dat(st_dat_t_w),
    .ack_transfer(dm_ack_transfer_w),
    .busy_n(ds_busy_n_w),
    .transm_complete(ds_transm_w),
    .crc_ok(ds_crc_ok_w)
  );
  // --- TX BD ---
  sd_bd u_tx_bd (
    .clk(wb_clk_i),
    .rst(wb_rst_i),
    .we_m(we_m_tx_bd_w),
    .dat_in_m(dat_in_m_tx_bd_w),
    .free_bd(tx_bd_free_w),
    .re_s(dm_re_s_tx_w),
    .ack_o_s(tx_bd_ack_w),
    .a_cmp(dm_a_cmp_tx_w),
    .dat_out_s(tx_bd_dat_out_w)
  );
  // --- RX BD ---
  sd_bd u_rx_bd (
    .clk(wb_clk_i),
    .rst(wb_rst_i),
    .we_m(we_m_rx_bd_w),
    .dat_in_m(dat_in_m_rx_bd_w),
    .free_bd(rx_bd_free_w),
    .re_s(dm_re_s_rx_w),
    .ack_o_s(rx_bd_ack_w),
    .a_cmp(dm_a_cmp_rx_w),
    .dat_out_s(rx_bd_dat_out_w)
  );
  // --- TX filler (internally contains sd_tx_fifo) ---
  sd_fifo_tx_filler u_tx_filler (
    .clk(wb_clk_i),
    .rst(wb_rst_i),
    .m_wb_adr_o(txf_adr_w),
    .m_wb_we_o(txf_we_w),
    .m_wb_dat_i(m_wb_dat_i),
    .m_wb_cyc_o(txf_cyc_w),
    .m_wb_stb_o(txf_stb_w),
    .m_wb_ack_i(m_wb_ack_i),
    .m_wb_cti_o(txf_cti_w),
    .m_wb_bte_o(txf_bte_w),
    .en(dm_start_tx_fifo_w),
    .adr(dm_sys_adr_w),
    .sd_clk(wb_clk_i),
    .dat_o(txf_q_w),
    .rd(ds_rd_w),
    .empty(txf_empty_w),
    .fe(txf_full_w)
  );
  // --- RX filler (internally contains sd_rx_fifo) ---
  sd_fifo_rx_filler u_rx_filler (
    .clk(wb_clk_i),
    .rst(wb_rst_i),
    .m_wb_adr_o(rxf_adr_w),
    .m_wb_we_o(rxf_we_w),
    .m_wb_dat_o(rxf_dato_w),
    .m_wb_cyc_o(rxf_cyc_w),
    .m_wb_stb_o(rxf_stb_w),
    .m_wb_ack_i(m_wb_ack_i),
    .m_wb_cti_o(rxf_cti_w),
    .m_wb_bte_o(rxf_bte_w),
    .en(dm_start_rx_fifo_w),
    .adr(dm_sys_adr_w),
    .sd_clk(wb_clk_i),
    .dat_i(ds_data_out_w),
    .wr(ds_we_w),
    .full(rxf_full_w),
    .empty(rxf_empty_w)
  );
  // --- WB master mux: TX filler vs RX filler ---
  always_comb begin
    if (dm_start_tx_fifo_w) begin
      m_wb_adr_o = txf_adr_w;
      m_wb_we_o = txf_we_w;
      m_wb_cyc_o = txf_cyc_w;
      m_wb_stb_o = txf_stb_w;
      m_wb_cti_o = txf_cti_w;
      m_wb_bte_o = txf_bte_w;
      m_wb_dat_o = 0;
    end else if (dm_start_rx_fifo_w) begin
      m_wb_adr_o = rxf_adr_w;
      m_wb_we_o = rxf_we_w;
      m_wb_cyc_o = rxf_cyc_w;
      m_wb_stb_o = rxf_stb_w;
      m_wb_cti_o = rxf_cti_w;
      m_wb_bte_o = rxf_bte_w;
      m_wb_dat_o = rxf_dato_w;
    end else begin
      m_wb_adr_o = 0;
      m_wb_we_o = 1'b0;
      m_wb_cyc_o = 1'b0;
      m_wb_stb_o = 1'b0;
      m_wb_cti_o = 0;
      m_wb_bte_o = 0;
      m_wb_dat_o = 0;
    end
    m_wb_sel_o = 4'd15;
  end
  // --- SD card outputs ---
  assign sd_cmd_out_o = cs_cmd_out_w;
  assign sd_cmd_oe_o = cs_cmd_oe_w;
  assign sd_dat_out_o = ds_dat_out_w;
  assign sd_dat_oe_o = ds_dat_oe_w;
  assign sd_clk_o_pad = sd_clk_w;
  // --- cmd_busy: int_busy | status_reg[0] ---
  assign cmd_busy_w = int_busy_w | status_reg_r[0];
  // --- Status register pipeline ---
  // Reference: always @(posedge wb_clk_i) (no reset)
  logic status_reg_busy_w;
  assign status_reg_busy_w = cmd_int_busy_w | status_reg_cm[0];
  always_ff @(posedge wb_clk_i or posedge wb_rst_i) begin
    if (wb_rst_i) begin
      Bd_Status_reg_r <= 0;
      Bd_isr_reg_r <= 0;
      cmd_resp_1_r <= 0;
      error_int_status_reg_r <= 0;
      normal_int_status_reg_r <= 0;
      status_reg_r <= 0;
    end else begin
      Bd_Status_reg_r[15:8] <= 8'($unsigned(rx_bd_free_w));
      Bd_Status_reg_r[7:0] <= 8'($unsigned(tx_bd_free_w));
      cmd_resp_1_r <= cmd_resp_1_cm;
      normal_int_status_reg_r <= normal_int_cm;
      error_int_status_reg_r <= 16'($unsigned(err_int_cm));
      status_reg_r[0] <= status_reg_busy_w;
      status_reg_r[15:1] <= status_reg_cm[15:1];
      status_reg_r[1] <= dm_cidat_w;
      Bd_isr_reg_r <= dm_dat_int_w;
    end
  end
  // --- Interrupts ---
  assign int_a = (normal_int_status_reg_r & normal_int_signal_enable_reg_w) != 0;
  assign int_b = (error_int_status_reg_r & error_int_signal_enable_reg_w) != 0;
  assign int_c = (Bd_isr_reg_r & Bd_isr_enable_reg_w) != 0;

endmodule

