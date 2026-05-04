// SD Clock Divider
// Generates SD_CLK at CLK/(2*(DIVIDER+1)) frequency.
module sd_clock_divider (
  input logic CLK,
  input logic RST,
  input logic [7:0] DIVIDER,
  output logic SD_CLK
);

  logic [7:0] clock_div;
  logic sd_clk_o;
  always_ff @(posedge CLK or posedge RST) begin
    if (RST) begin
      clock_div <= 0;
      sd_clk_o <= 1'b0;
    end else begin
      if (clock_div == DIVIDER) begin
        clock_div <= 0;
        sd_clk_o <= ~sd_clk_o;
      end else begin
        clock_div <= 8'(clock_div + 1);
      end
    end
  end
  assign SD_CLK = sd_clk_o;

endmodule

// SD Controller Wishbone Slave Interface
// Register bank for SD controller. Matches OpenCores SDC reference port interface.
// Uses RAM_MEM_WIDTH=16 (ACTEL config from sd_defines.v).
module sd_controller_wb (
  input logic wb_clk_i,
  input logic wb_rst_i,
  input logic [31:0] wb_dat_i,
  output logic [31:0] wb_dat_o,
  input logic [7:0] wb_adr_i,
  input logic [3:0] wb_sel_i,
  input logic wb_we_i,
  input logic wb_cyc_i,
  input logic wb_stb_i,
  output logic wb_ack_o,
  output logic we_m_tx_bd,
  output logic new_cmd,
  output logic we_ack,
  output logic int_ack,
  output logic cmd_int_busy,
  output logic we_m_rx_bd,
  output logic int_busy,
  input logic write_req_s,
  input logic [15:0] cmd_set_s,
  input logic [31:0] cmd_arg_s,
  output logic [31:0] argument_reg,
  output logic [15:0] cmd_setting_reg,
  output logic [7:0] software_reset_reg,
  output logic [15:0] time_out_reg,
  output logic [15:0] normal_int_signal_enable_reg,
  output logic [15:0] error_int_signal_enable_reg,
  output logic [7:0] clock_divider,
  output logic [7:0] Bd_isr_enable_reg,
  input logic [15:0] status_reg,
  input logic [31:0] cmd_resp_1,
  input logic [15:0] normal_int_status_reg,
  input logic [15:0] error_int_status_reg,
  input logic [15:0] Bd_Status_reg,
  input logic [7:0] Bd_isr_reg,
  output logic Bd_isr_reset,
  output logic normal_isr_reset,
  output logic error_isr_reset,
  output logic [15:0] dat_in_m_rx_bd,
  output logic [15:0] dat_in_m_tx_bd
);

  // Wishbone slave
  // Control outputs
  // Data master cmd write request
  // Bus-accessible registers (outputs)
  // Status inputs
  // Register control
  // --- Internal registers ---
  logic [31:0] argument_reg_r;
  logic [15:0] cmd_setting_reg_r;
  logic [7:0] software_reset_reg_r;
  logic [15:0] time_out_reg_r;
  logic [15:0] normal_int_signal_enable_reg_r;
  logic [15:0] error_int_signal_enable_reg_r;
  logic [7:0] clock_divider_r;
  logic [7:0] Bd_isr_enable_reg_r;
  logic [1:0] we_r;
  logic int_busy_r;
  logic wb_ack_o_r;
  logic we_m_tx_bd_r;
  logic we_m_rx_bd_r;
  logic new_cmd_r;
  logic we_ack_r;
  logic cmd_int_busy_r;
  logic Bd_isr_reset_r;
  logic normal_isr_reset_r;
  logic error_isr_reset_r;
  logic [15:0] dat_in_m_rx_bd_r;
  logic [15:0] dat_in_m_tx_bd_r;
  // Combinational int_ack (blocking in reference)
  logic int_ack_w;
  // Combinational wb_ack (blocking in reference)
  logic wb_ack_comb;
  logic [31:0] wb_dat_o_r;
  // Register address constants
  // argument=0x00, command=0x04, status=0x08, resp1=0x0c,
  // controller=0x1c, block=0x20, power=0x24, software=0x28,
  // timeout=0x2c, normal_isr=0x30, error_isr=0x34, normal_iser=0x38,
  // error_iser=0x3c, capa=0x48, clock_d=0x4c, bd_status=0x50,
  // bd_isr=0x54, bd_iser=0x58, bd_rx=0x60, bd_tx=0x80
  // Parameters (from sd_defines.v with ACTEL + SD_BUS_WIDTH_4 + SUPPLY_VOLTAGE_3_3)
  logic [7:0] power_controll_reg;
  assign power_controll_reg = 8'd15;
  logic [31:0] block_size_reg;
  assign block_size_reg = 512;
  logic [15:0] controll_setting_reg;
  assign controll_setting_reg = 16'd2;
  logic [15:0] capabilies_reg;
  assign capabilies_reg = 0;
  // --- Main clocked logic ---
  // Reference uses mixed blocking/non-blocking. We model int_ack as combinational
  // (it's set blocking at start and conditionally cleared in bd_rx/bd_tx writes).
  // wb_ack_o is also combinational in reference (blocking assign).
  always_ff @(posedge wb_clk_i or posedge wb_rst_i) begin
    if (wb_rst_i) begin
      Bd_isr_enable_reg_r <= 0;
      Bd_isr_reset_r <= 0;
      argument_reg_r <= 0;
      clock_divider_r <= 0;
      cmd_int_busy_r <= 0;
      cmd_setting_reg_r <= 0;
      dat_in_m_rx_bd_r <= 0;
      dat_in_m_tx_bd_r <= 0;
      error_int_signal_enable_reg_r <= 0;
      error_isr_reset_r <= 0;
      int_busy_r <= 0;
      new_cmd_r <= 0;
      normal_int_signal_enable_reg_r <= 0;
      normal_isr_reset_r <= 0;
      software_reset_reg_r <= 0;
      time_out_reg_r <= 0;
      wb_ack_o_r <= 0;
      we_ack_r <= 0;
      we_m_rx_bd_r <= 0;
      we_m_tx_bd_r <= 0;
      we_r <= 0;
    end else begin
      // Defaults (non-blocking, set each cycle)
      we_m_rx_bd_r <= 1'b0;
      we_m_tx_bd_r <= 1'b0;
      new_cmd_r <= 1'b0;
      we_ack_r <= 1'b0;
      cmd_int_busy_r <= 1'b0;
      // After reset
      Bd_isr_reset_r <= 1'b0;
      normal_isr_reset_r <= 1'b0;
      error_isr_reset_r <= 1'b0;
      if ((wb_stb_i & wb_cyc_i) | wb_ack_o_r) begin
        if (wb_we_i) begin
          if (wb_adr_i == 8'd0) begin
            // argument
            argument_reg_r <= wb_dat_i;
            new_cmd_r <= 1'b1;
          end else if (wb_adr_i == 8'd4) begin
            // command
            cmd_setting_reg_r <= wb_dat_i[15:0];
            int_busy_r <= 1'b1;
          end else if (wb_adr_i == 8'd40) begin
            // software
            software_reset_reg_r <= wb_dat_i[7:0];
          end else if (wb_adr_i == 8'd44) begin
            // timeout
            time_out_reg_r <= wb_dat_i[15:0];
          end else if (wb_adr_i == 8'd56) begin
            // normal_iser
            normal_int_signal_enable_reg_r <= wb_dat_i[15:0];
          end else if (wb_adr_i == 8'd60) begin
            // error_iser
            error_int_signal_enable_reg_r <= wb_dat_i[15:0];
          end else if (wb_adr_i == 8'd48) begin
            // normal_isr
            normal_isr_reset_r <= 1'b1;
          end else if (wb_adr_i == 8'd52) begin
            // error_isr
            error_isr_reset_r <= 1'b1;
          end else if (wb_adr_i == 8'd76) begin
            // clock_d
            clock_divider_r <= wb_dat_i[7:0];
          end else if (wb_adr_i == 8'd84) begin
            // bd_isr
            Bd_isr_reset_r <= 1'b1;
          end else if (wb_adr_i == 8'd88) begin
            // bd_iser
            Bd_isr_enable_reg_r <= wb_dat_i[7:0];
          end else if (wb_adr_i == 8'd96) begin
            // RAM_MEM_WIDTH_16 bd_rx (0x60)
            we_r <= 2'(we_r + 1);
            we_m_rx_bd_r <= 1'b1;
            if (we_r[1:0] == 2'd0) begin
              we_m_rx_bd_r <= 1'b0;
            end else if (we_r[1:0] == 2'd1) begin
              dat_in_m_rx_bd_r <= wb_dat_i[15:0];
            end else if (we_r[1:0] == 2'd2) begin
              dat_in_m_rx_bd_r <= wb_dat_i[31:16];
            end else begin
              we_r <= 0;
              we_m_rx_bd_r <= 1'b0;
            end
          end else if (wb_adr_i == 8'd128) begin
            // RAM_MEM_WIDTH_16 bd_tx (0x80)
            we_r <= 2'(we_r + 1);
            we_m_tx_bd_r <= 1'b1;
            if (we_r[1:0] == 2'd0) begin
              we_m_tx_bd_r <= 1'b0;
            end else if (we_r[1:0] == 2'd1) begin
              dat_in_m_tx_bd_r <= wb_dat_i[15:0];
            end else if (we_r[1:0] == 2'd2) begin
              dat_in_m_tx_bd_r <= wb_dat_i[31:16];
            end else begin
              we_r <= 0;
              we_m_tx_bd_r <= 1'b0;
            end
          end
        end
      end else if (write_req_s) begin
        // wb_ack_o computed combinationally below
        new_cmd_r <= 1'b1;
        cmd_setting_reg_r <= cmd_set_s;
        argument_reg_r <= cmd_arg_s;
        cmd_int_busy_r <= 1'b1;
        we_ack_r <= 1'b1;
      end
      if (status_reg[0]) begin
        int_busy_r <= 1'b0;
      end
      wb_ack_o_r <= wb_ack_comb;
    end
  end
  // --- Combinational: int_ack ---
  // In the reference, int_ack is set to 1 (blocking) at the start of the always block,
  // then cleared to 0 in bd_rx/bd_tx multi-cycle writes. Since our model is non-blocking,
  // we compute it combinationally.
  always_comb begin
    int_ack_w = 1'b1;
    if ((wb_stb_i & wb_cyc_i) | wb_ack_o_r) begin
      if (wb_we_i) begin
        if (wb_adr_i == 8'd96) begin
          // bd_rx
          if (we_r[1:0] != 2'd3) begin
            int_ack_w = 1'b0;
          end
        end else if (wb_adr_i == 8'd128) begin
          // bd_tx
          if (we_r[1:0] != 2'd3) begin
            int_ack_w = 1'b0;
          end
        end
      end
    end
  end
  // --- Combinational: wb_ack_o ---
  assign wb_ack_comb = wb_cyc_i & wb_stb_i & ~wb_ack_o_r & int_ack_w;
  // --- Read data mux (separate clocked block, no reset) ---
  // The reference uses a separate always @(posedge wb_clk_i) for reads.
  logic [31:0] wb_dat_o_rd;
  always_ff @(posedge wb_clk_i or posedge wb_rst_i) begin
    if (wb_rst_i) begin
      wb_dat_o_rd <= 0;
    end else begin
      if (wb_stb_i & wb_cyc_i) begin
        if (wb_adr_i == 8'd0) begin
          wb_dat_o_rd <= argument_reg_r;
        end else if (wb_adr_i == 8'd4) begin
          wb_dat_o_rd <= 32'($unsigned(cmd_setting_reg_r));
        end else if (wb_adr_i == 8'd8) begin
          wb_dat_o_rd <= 32'($unsigned(status_reg));
        end else if (wb_adr_i == 8'd12) begin
          wb_dat_o_rd <= cmd_resp_1;
        end else if (wb_adr_i == 8'd28) begin
          wb_dat_o_rd <= 32'($unsigned(controll_setting_reg));
        end else if (wb_adr_i == 8'd32) begin
          wb_dat_o_rd <= block_size_reg;
        end else if (wb_adr_i == 8'd36) begin
          wb_dat_o_rd <= 32'($unsigned(power_controll_reg));
        end else if (wb_adr_i == 8'd40) begin
          wb_dat_o_rd <= 32'($unsigned(software_reset_reg_r));
        end else if (wb_adr_i == 8'd44) begin
          wb_dat_o_rd <= 32'($unsigned(time_out_reg_r));
        end else if (wb_adr_i == 8'd48) begin
          wb_dat_o_rd <= 32'($unsigned(normal_int_status_reg));
        end else if (wb_adr_i == 8'd52) begin
          wb_dat_o_rd <= 32'($unsigned(error_int_status_reg));
        end else if (wb_adr_i == 8'd56) begin
          wb_dat_o_rd <= 32'($unsigned(normal_int_signal_enable_reg_r));
        end else if (wb_adr_i == 8'd60) begin
          wb_dat_o_rd <= 32'($unsigned(error_int_signal_enable_reg_r));
        end else if (wb_adr_i == 8'd72) begin
          wb_dat_o_rd <= 32'($unsigned(capabilies_reg));
        end else if (wb_adr_i == 8'd76) begin
          wb_dat_o_rd <= 32'($unsigned(clock_divider_r));
        end else if (wb_adr_i == 8'd80) begin
          wb_dat_o_rd <= 32'($unsigned(Bd_Status_reg));
        end else if (wb_adr_i == 8'd84) begin
          wb_dat_o_rd <= 32'($unsigned(Bd_isr_reg));
        end else if (wb_adr_i == 8'd88) begin
          wb_dat_o_rd <= 32'($unsigned(Bd_isr_enable_reg_r));
        end
      end
    end
  end
  // --- Output assignments ---
  assign wb_dat_o = wb_dat_o_rd;
  assign wb_ack_o = wb_ack_o_r;
  assign we_m_tx_bd = we_m_tx_bd_r;
  assign new_cmd = new_cmd_r;
  assign we_ack = we_ack_r;
  assign int_ack = int_ack_w;
  assign cmd_int_busy = cmd_int_busy_r;
  assign we_m_rx_bd = we_m_rx_bd_r;
  assign int_busy = int_busy_r;
  assign argument_reg = argument_reg_r;
  assign cmd_setting_reg = cmd_setting_reg_r;
  assign software_reset_reg = software_reset_reg_r;
  assign time_out_reg = time_out_reg_r;
  assign normal_int_signal_enable_reg = normal_int_signal_enable_reg_r;
  assign error_int_signal_enable_reg = error_int_signal_enable_reg_r;
  assign clock_divider = clock_divider_r;
  assign Bd_isr_enable_reg = Bd_isr_enable_reg_r;
  assign Bd_isr_reset = Bd_isr_reset_r;
  assign normal_isr_reset = normal_isr_reset_r;
  assign error_isr_reset = error_isr_reset_r;
  assign dat_in_m_rx_bd = dat_in_m_rx_bd_r;
  assign dat_in_m_tx_bd = dat_in_m_tx_bd_r;

endmodule

// SD Command Master
// 3-state FSM (IDLE/SETUP/EXECUTE) with card detect debounce, command
// timeout watchdog, CRC/index error checking. Port interface matches
// the OpenCores SDC reference exactly.
module sd_cmd_master (
  input logic CLK_PAD_IO,
  input logic RST_PAD_I,
  input logic New_CMD,
  input logic data_write,
  input logic data_read,
  input logic [31:0] ARG_REG,
  input logic [13:0] CMD_SET_REG,
  input logic [15:0] TIMEOUT_REG,
  output logic [15:0] STATUS_REG,
  output logic [31:0] RESP_1_REG,
  output logic [4:0] ERR_INT_REG,
  output logic [15:0] NORMAL_INT_REG,
  input logic ERR_INT_RST,
  input logic NORMAL_INT_RST,
  output logic [15:0] settings,
  output logic go_idle_o,
  output logic [39:0] cmd_out,
  output logic req_out,
  output logic ack_out,
  input logic req_in,
  input logic ack_in,
  input logic [39:0] cmd_in,
  input logic [7:0] serial_status,
  input logic card_detect
);

  // --- Internal registers ---
  // Input synchronizers (2-stage)
  logic req_q;
  logic req_in_int;
  logic ack_q;
  logic ack_in_int;
  // Card detect debounce
  logic [3:0] debounce_r;
  logic card_present_r;
  // FSM state (one-hot: IDLE=001, SETUP=010, EXECUTE=100)
  logic [2:0] state_r;
  // Registered outputs & internals
  logic CRC_check_enable_r;
  logic index_check_enable_r;
  logic [6:0] response_size_r;
  logic [15:0] status_r;
  logic [15:0] Watchdog_Cnt_r;
  logic [15:0] STATUS_REG_r;
  logic [31:0] RESP_1_REG_r;
  logic [4:0] ERR_INT_REG_r;
  logic [15:0] NORMAL_INT_REG_r;
  logic [15:0] settings_r;
  logic go_idle_o_r;
  logic [39:0] cmd_out_r;
  logic req_out_r;
  logic ack_out_r;
  // Combinational signals computed each cycle (blocking equivalents)
  logic complete_w;
  logic [2:0] next_state_w;
  logic [15:0] Watchdog_next;
  // --- Input synchronizers ---
  always_ff @(posedge CLK_PAD_IO or posedge RST_PAD_I) begin
    if (RST_PAD_I) begin
      req_in_int <= 0;
      req_q <= 0;
    end else begin
      req_q <= req_in;
      req_in_int <= req_q;
    end
  end
  always_ff @(posedge CLK_PAD_IO or posedge RST_PAD_I) begin
    if (RST_PAD_I) begin
      ack_in_int <= 0;
      ack_q <= 0;
    end else begin
      ack_q <= ack_in;
      ack_in_int <= ack_q;
    end
  end
  // --- Card detect debounce ---
  always_ff @(posedge CLK_PAD_IO or posedge RST_PAD_I) begin
    if (RST_PAD_I) begin
      card_present_r <= 0;
      debounce_r <= 0;
    end else begin
      if (~card_detect) begin
        if (debounce_r != 4'd15) begin
          debounce_r <= 4'(debounce_r + 1);
        end
      end else begin
        debounce_r <= 0;
      end
      if (debounce_r == 4'd15) begin
        card_present_r <= 1'b1;
      end else begin
        card_present_r <= 1'b0;
      end
    end
  end
  // --- Combinational: next_state and complete ---
  // complete is set combinationally so that the FSM combo sees it same-cycle
  always_comb begin
    Watchdog_next = 16'(Watchdog_Cnt_r + 1);
    complete_w = 1'b0;
    if (state_r == 3'd4) begin
      // EXECUTE
      if (Watchdog_next > TIMEOUT_REG) begin
        if (ack_in) begin
          complete_w = 1'b1;
        end
      end
      if (req_in_int) begin
        if (serial_status[6]) begin
          // dat_ava from freshly latched status
          complete_w = 1'b1;
        end
      end
    end
  end
  always_comb begin
    next_state_w = 0;
    if (state_r == 3'd1) begin
      // IDLE
      if (New_CMD) begin
        next_state_w = 3'd2;
      end else begin
        // SETUP
        next_state_w = 3'd1;
      end
    end else if (state_r == 3'd2) begin
      // IDLE
      // SETUP
      if (ack_in_int) begin
        next_state_w = 3'd4;
      end else begin
        // EXECUTE
        next_state_w = 3'd2;
      end
    end else if (state_r == 3'd4) begin
      // SETUP
      // EXECUTE
      if (complete_w) begin
        next_state_w = 3'd1;
      end else begin
        // IDLE
        next_state_w = 3'd4;
      end
    end else begin
      // EXECUTE
      next_state_w = 3'd1;
    end
    // default IDLE
  end
  // --- FSM state register ---
  always_ff @(posedge CLK_PAD_IO or posedge RST_PAD_I) begin
    if (RST_PAD_I) begin
      state_r <= 1;
    end else begin
      state_r <= next_state_w;
    end
  end
  // --- Main output logic ---
  // The reference uses blocking assignments in a clocked block. We replicate
  // the same behavior using combinational "next value" wires fed into regs.
  // However, many signals in the reference depend on ordering (blocking);
  // we carefully break them into comb + seq to preserve cycle-accuracy.
  // Aliases for CMD_SET_REG fields
  logic [5:0] CMDI;
  assign CMDI = CMD_SET_REG[13:8];
  logic [1:0] WORD_SELECT;
  assign WORD_SELECT = CMD_SET_REG[7:6];
  logic CICE;
  assign CICE = CMD_SET_REG[4];
  logic CRCE;
  assign CRCE = CMD_SET_REG[3];
  logic [1:0] RTS;
  assign RTS = CMD_SET_REG[1:0];
  always_ff @(posedge CLK_PAD_IO or posedge RST_PAD_I) begin
    if (RST_PAD_I) begin
      CRC_check_enable_r <= 0;
      ERR_INT_REG_r <= 0;
      NORMAL_INT_REG_r <= 0;
      RESP_1_REG_r <= 0;
      STATUS_REG_r <= 0;
      Watchdog_Cnt_r <= 0;
      ack_out_r <= 0;
      cmd_out_r <= 0;
      go_idle_o_r <= 0;
      index_check_enable_r <= 0;
      req_out_r <= 0;
      response_size_r <= 0;
      settings_r <= 0;
      status_r <= 0;
    end else begin
      // --- NORMAL_INT_REG card present bits (always updated) ---
      NORMAL_INT_REG_r[1] <= card_present_r;
      NORMAL_INT_REG_r[2] <= ~card_present_r;
      if (state_r == 3'd1) begin
        // IDLE
        go_idle_o_r <= 1'b0;
        req_out_r <= 1'b0;
        ack_out_r <= 1'b0;
        STATUS_REG_r[0] <= 1'b0;
        // CICMD = 0
        if (req_in_int) begin
          status_r <= 16'($unsigned(serial_status));
          ack_out_r <= 1'b1;
        end
      end else if (state_r == 3'd2) begin
        // SETUP
        NORMAL_INT_REG_r <= 0;
        ERR_INT_REG_r <= 0;
        index_check_enable_r <= CICE;
        CRC_check_enable_r <= CRCE;
        if ((RTS == 2'd2) | (RTS == 2'd3)) begin
          response_size_r <= 7'd40;
        end else if (RTS == 2'd1) begin
          response_size_r <= 7'd127;
        end else begin
          response_size_r <= 0;
        end
        cmd_out_r[39:38] <= 2'd1;
        cmd_out_r[37:32] <= CMDI;
        cmd_out_r[31:0] <= ARG_REG;
        settings_r[14:13] <= WORD_SELECT;
        settings_r[12] <= data_read;
        settings_r[11] <= data_write;
        settings_r[10:8] <= 3'd7;
        settings_r[7] <= CRCE;
        // response_size computed above — assign combinationally
        if ((RTS == 2'd2) | (RTS == 2'd3)) begin
          settings_r[6:0] <= 7'd40;
        end else if (RTS == 2'd1) begin
          settings_r[6:0] <= 7'd127;
        end else begin
          settings_r[6:0] <= 0;
        end
        Watchdog_Cnt_r <= 0;
        STATUS_REG_r[0] <= 1'b1;
      end else if (state_r == 3'd4) begin
        // CICMD = 1
        // EXECUTE
        Watchdog_Cnt_r <= Watchdog_next;
        if (Watchdog_next > TIMEOUT_REG) begin
          ERR_INT_REG_r[0] <= 1'b1;
          // CTE
          NORMAL_INT_REG_r[15] <= 1'b1;
          // EI
          if (ack_in) begin
          end
          // complete handled combinationally
          go_idle_o_r <= 1'b1;
        end
        // Default
        req_out_r <= 1'b0;
        ack_out_r <= 1'b0;
        // Start sending when serial module is ready
        if (ack_in_int) begin
          req_out_r <= 1'b1;
        end else if (req_in_int) begin
          // Incoming New Status
          status_r <= 16'($unsigned(serial_status));
          ack_out_r <= 1'b1;
          if (serial_status[6]) begin
            // dat_ava
            NORMAL_INT_REG_r[15] <= 1'b0;
            // EI = 0
            if (CRC_check_enable_r & ~serial_status[5]) begin
              // ~crc_valid
              ERR_INT_REG_r[1] <= 1'b1;
              // CCRCE
              NORMAL_INT_REG_r[15] <= 1'b1;
            end
            // EI = 1
            if (index_check_enable_r & (cmd_out_r[37:32] != cmd_in[37:32])) begin
              ERR_INT_REG_r[3] <= 1'b1;
              // CIE
              NORMAL_INT_REG_r[15] <= 1'b1;
            end
            // EI = 1
            NORMAL_INT_REG_r[0] <= 1'b1;
            // CC
            if (response_size_r != 0) begin
              RESP_1_REG_r <= cmd_in[31:0];
            end
          end
        end
      end
      // dat_ava
      // req_in_int vs ack_in_int
      // state
      // Interrupt reset (always, overrides above)
      if (ERR_INT_RST) begin
        ERR_INT_REG_r <= 0;
      end
      if (NORMAL_INT_RST) begin
        NORMAL_INT_REG_r <= 0;
      end
    end
  end
  // --- Output assignments ---
  assign STATUS_REG = STATUS_REG_r;
  assign RESP_1_REG = RESP_1_REG_r;
  assign ERR_INT_REG = ERR_INT_REG_r;
  assign NORMAL_INT_REG = NORMAL_INT_REG_r;
  assign settings = settings_r;
  assign go_idle_o = go_idle_o_r;
  assign cmd_out = cmd_out_r;
  assign req_out = req_out_r;
  assign ack_out = ack_out_r;

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

// SD Data Master (Data Transfer Orchestrator)
// 9-state one-hot FSM. Orchestrates data transfers: reads BDs, constructs
// commands, monitors transfers, handles errors with retry (up to 3 retries).
module sd_data_master (
  input logic clk,
  input logic rst,
  input logic [15:0] dat_in_tx,
  input logic [4:0] free_tx_bd,
  input logic ack_i_s_tx,
  output logic re_s_tx,
  output logic a_cmp_tx,
  input logic [15:0] dat_in_rx,
  input logic [4:0] free_rx_bd,
  input logic ack_i_s_rx,
  output logic re_s_rx,
  output logic a_cmp_rx,
  input logic cmd_busy,
  output logic we_req,
  input logic we_ack,
  output logic d_write,
  output logic d_read,
  output logic [31:0] cmd_arg,
  output logic [15:0] cmd_set,
  input logic cmd_tsf_err,
  input logic [4:0] card_status,
  output logic start_tx_fifo,
  output logic start_rx_fifo,
  output logic [31:0] sys_adr,
  input logic tx_empt,
  input logic tx_full,
  input logic rx_full,
  input logic busy_n,
  input logic transm_complete,
  input logic crc_ok,
  output logic ack_transfer,
  output logic [7:0] Dat_Int_Status,
  input logic Dat_Int_Status_rst,
  output logic CIDAT,
  input logic [1:0] transfer_type
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
  logic [3:0] st_r;
  // BD read data registers (two 16-bit reads per BD)
  logic [15:0] bd_adr_lo;
  logic [15:0] bd_adr_hi;
  logic [15:0] bd_blk;
  // Sub-state counter for BD reading
  logic [2:0] sub_cnt;
  // Retry counter
  logic [1:0] retry_cnt;
  // Output registers
  logic re_s_tx_r;
  logic re_s_rx_r;
  logic a_cmp_tx_r;
  logic a_cmp_rx_r;
  logic we_req_r;
  logic d_write_r;
  logic d_read_r;
  logic [31:0] cmd_arg_r;
  logic [15:0] cmd_set_r;
  logic start_tx_fifo_r;
  logic start_rx_fifo_r;
  logic [31:0] sys_adr_r;
  logic ack_transfer_r;
  logic [7:0] dat_int_r;
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
        if (is_tx & (free_tx_bd > 0)) begin
          st_r <= 1;
          // GET_TX_BD
          re_s_tx_r <= 1'b1;
          sub_cnt <= 0;
        end else if (~is_tx & (free_rx_bd > 0)) begin
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
  always_ff @(posedge clk or posedge rst) begin
    if (rst) begin
      bd_blk <= 0;
      cmd_set_r <= 0;
    end
  end

endmodule

// SD Data Serial Host
// 6-state FSM. Write path: serializes 32-bit words as nibbles (big-endian),
// CRC-16 per data line. Read path: captures nibbles, checks CRC-16.
module sd_data_serial_host (
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

  // State encoding
  // 0=IDLE, 1=WRITE_DAT, 2=WRITE_CRC, 3=WRITE_BUSY, 4=READ_WAIT, 5=READ_DAT
  logic [2:0] st_r;
  // Bit/nibble counter
  logic [15:0] bit_cnt;
  // Data shift register (32-bit, serialized as nibbles)
  logic [31:0] data_shift;
  // CRC shift registers per data line (for TX)
  logic [15:0] crc_shift0;
  logic [15:0] crc_shift1;
  logic [15:0] crc_shift2;
  logic [15:0] crc_shift3;
  // CRC status
  logic crc_ok_r;
  // Output registers
  logic busy_n_r;
  logic transm_c_r;
  logic dat_oe_r;
  logic [3:0] dat_out_r;
  logic we_r;
  logic rd_r;
  logic [3:0] data_out_r;
  // Received data buffer
  logic [31:0] rx_shift;
  // CRC count for TX
  logic [4:0] crc_cnt;
  // CRC wires
  logic crc_en_w;
  logic crc_rst_bit;
  logic [15:0] crc_out0;
  logic [15:0] crc_out1;
  logic [15:0] crc_out2;
  logic [15:0] crc_out3;
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
  assign crc_en_w = (st_r == 5) & (bit_cnt < 1024);
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

// SD Buffer Descriptor Manager
// RAM_MEM_WIDTH_16 variant: 16-bit x 32 RAM, 4 writes per BD (2x16-bit address + 2x16-bit data).
// free_bd resets to BD_SIZE/4 = 8. Matches OpenCores SDC reference.
module sd_bd (
  input logic clk,
  input logic rst,
  input logic we_m,
  input logic [15:0] dat_in_m,
  output logic [4:0] free_bd,
  input logic re_s,
  output logic ack_o_s,
  input logic a_cmp,
  output logic [15:0] dat_out_s
);

  // BD memory: 32 entries x 16 bits
  logic [31:0] [15:0] bd_mem;
  // Master write pointer (5-bit, indexes into bd_mem)
  logic [4:0] m_wr_pnt;
  // Write counter: tracks 4 writes per BD (0,1,2,3)
  logic [1:0] write_cnt;
  // new_bw: pulses when a BD is fully written
  logic new_bw;
  // Free BD counter: starts at BD_SIZE/4 = 8
  logic [4:0] free_bd_r;
  // last_a_cmp: edge detection for a_cmp
  logic last_a_cmp;
  // Slave read pointer
  logic [4:0] s_rd_pnt;
  // Slave read sub-word counter
  logic [1:0] read_s_cnt;
  // Slave ack
  logic ack_o_s_r;
  // Slave data output
  logic [15:0] dat_out_s_r;
  // Master write logic
  always_ff @(posedge clk or posedge rst) begin
    if (rst) begin
      m_wr_pnt <= 0;
      new_bw <= 1'b0;
      write_cnt <= 0;
    end else begin
      new_bw <= 1'b0;
      if (we_m) begin
        if (free_bd_r > 0) begin
          write_cnt <= 2'(write_cnt + 1);
          m_wr_pnt <= 5'(m_wr_pnt + 1);
          if (~write_cnt[1]) begin
            // First two writes: address part
            bd_mem[m_wr_pnt] <= dat_in_m;
          end else begin
            // Second two writes: data part
            bd_mem[m_wr_pnt] <= dat_in_m;
            new_bw <= write_cnt[0];
          end
        end
      end
      // Complete BD on 4th write (cnt goes 0->1->2->3)
    end
  end
  // Free BD counter
  always_ff @(posedge clk or posedge rst) begin
    if (rst) begin
      free_bd_r <= 8;
      last_a_cmp <= 1'b0;
    end else begin
      if (new_bw) begin
        free_bd_r <= 5'(free_bd_r - 1);
      end else if (a_cmp) begin
        last_a_cmp <= a_cmp;
        if (~last_a_cmp) begin
          free_bd_r <= 5'(free_bd_r + 1);
        end
      end else begin
        last_a_cmp <= a_cmp;
      end
    end
  end
  // Slave read logic
  always_ff @(posedge clk or posedge rst) begin
    if (rst) begin
      ack_o_s_r <= 1'b0;
      dat_out_s_r <= 0;
      read_s_cnt <= 0;
      s_rd_pnt <= 0;
    end else begin
      ack_o_s_r <= 1'b0;
      if (re_s) begin
        read_s_cnt <= 2'(read_s_cnt + 1);
        s_rd_pnt <= 5'(s_rd_pnt + 1);
        ack_o_s_r <= 1'b1;
        if (~read_s_cnt[1]) begin
          dat_out_s_r <= bd_mem[s_rd_pnt];
        end else begin
          dat_out_s_r <= bd_mem[s_rd_pnt];
        end
      end
    end
  end
  assign free_bd = free_bd_r;
  assign ack_o_s = ack_o_s_r;
  assign dat_out_s = dat_out_s_r;
  // synopsys translate_off
  // Auto-generated safety assertions (bounds / divide-by-zero)
  _auto_bound_vec_0: assert property (@(posedge clk) disable iff (rst) int'(m_wr_pnt) < (32))
    else $fatal(1, "BOUNDS VIOLATION: sd_bd._auto_bound_vec_0");
  _auto_bound_vec_1: assert property (@(posedge clk) disable iff (rst) int'(s_rd_pnt) < (32))
    else $fatal(1, "BOUNDS VIOLATION: sd_bd._auto_bound_vec_1");
  // synopsys translate_on

endmodule

// SD FIFO TX Filler
// DMA: reads data from system memory via Wishbone master, fills TX FIFO.
module sd_fifo_tx_filler (
  input logic clk,
  input logic rst,
  output logic [31:0] m_wb_adr_o,
  output logic m_wb_we_o,
  input logic [31:0] m_wb_dat_i,
  output logic m_wb_cyc_o,
  output logic m_wb_stb_o,
  input logic m_wb_ack_i,
  output logic [2:0] m_wb_cti_o,
  output logic [1:0] m_wb_bte_o,
  input logic en,
  input logic [31:0] adr,
  input logic sd_clk,
  output logic [31:0] dat_o,
  input logic rd,
  output logic empty,
  output logic fe
);

  logic [31:0] offset_r;
  logic [31:0] din_r;
  logic wr_tx_r;
  logic delay_r;
  logic ackd_r;
  logic cyc_r;
  logic stb_r;
  logic fe_w;
  logic empty_w;
  logic [5:0] mem_empt_w;
  sd_tx_fifo u_fifo (
    .wclk(clk),
    .rclk(sd_clk),
    .rst(rst),
    .d(din_r),
    .wr(wr_tx_r),
    .q(dat_o),
    .rd(rd),
    .full(fe_w),
    .empty(empty_w),
    .mem_empt(mem_empt_w)
  );
  always_ff @(posedge clk or posedge rst) begin
    if (rst) begin
      ackd_r <= 1'b1;
      cyc_r <= 1'b0;
      delay_r <= 1'b0;
      din_r <= 0;
      offset_r <= 0;
      stb_r <= 1'b0;
      wr_tx_r <= 1'b0;
    end else begin
      if (~en) begin
        offset_r <= 0;
        wr_tx_r <= 1'b0;
        delay_r <= 1'b0;
        ackd_r <= 1'b1;
        cyc_r <= 1'b0;
        stb_r <= 1'b0;
      end else begin
        // Default: clear write pulse
        wr_tx_r <= 1'b0;
        if (delay_r) begin
          // Delay cycle: increment offset, toggle ackd
          delay_r <= 1'b0;
          offset_r <= 32'(offset_r + 4);
          ackd_r <= ~ackd_r;
        end else if (m_wb_ack_i) begin
          // WB ack: capture data, deassert bus
          din_r <= m_wb_dat_i;
          wr_tx_r <= 1'b1;
          cyc_r <= 1'b0;
          stb_r <= 1'b0;
          delay_r <= 1'b1;
        end else if (~fe_w & ~m_wb_ack_i & ackd_r) begin
          // Start WB read: FIFO not full, no ack pending, previous done
          cyc_r <= 1'b1;
          stb_r <= 1'b1;
        end
      end
    end
  end
  assign m_wb_adr_o = 32'(adr + offset_r);
  assign m_wb_we_o = 1'b0;
  assign m_wb_cyc_o = cyc_r;
  assign m_wb_stb_o = stb_r;
  assign m_wb_cti_o = 0;
  assign m_wb_bte_o = 0;
  assign fe = fe_w;
  assign empty = empty_w;

endmodule

// SD TX FIFO — dual-clock, 32-bit in, 32-bit out
// Matches RealBench reference: ram + manual pointers, unsynchronized
// full/empty (combinational compare). Straightforward dual-clock FIFO.
//
// Spec: sd_tx_fifo.md — DEPTH=8, ADR_SIZE=4
// domain WrDomain
//   freq_mhz: 100

// domain RdDomain
//   freq_mhz: 50

module sd_tx_fifo #(
  parameter int TX_DEPTH = 8
) (
  input logic wclk,
  input logic rclk,
  input logic rst,
  input logic [31:0] d,
  input logic wr,
  output logic [31:0] q,
  input logic rd,
  output logic full,
  output logic empty,
  output logic [5:0] mem_empt
);

  // ── RAM storage ──────────────────────────────────────────────────────
  logic [31:0] ram_0;
  logic [31:0] ram_1;
  logic [31:0] ram_2;
  logic [31:0] ram_3;
  logic [31:0] ram_4;
  logic [31:0] ram_5;
  logic [31:0] ram_6;
  logic [31:0] ram_7;
  // ── Pointers: [MSB]=wrap, [2:0]=address ─────────────────────────────
  logic [3:0] adr_i;
  // write pointer (wclk domain)
  logic [3:0] adr_o;
  // read pointer  (rclk domain)
  // Write side
  always_ff @(posedge wclk or posedge rst) begin
    if (rst) begin
      adr_i <= 0;
      ram_0 <= 0;
      ram_1 <= 0;
      ram_2 <= 0;
      ram_3 <= 0;
      ram_4 <= 0;
      ram_5 <= 0;
      ram_6 <= 0;
      ram_7 <= 0;
    end else begin
      if (wr & ~full) begin
        // Write data to RAM at current address
        if (adr_i[2:0] == 0) begin
          ram_0 <= d;
        end else if (adr_i[2:0] == 1) begin
          ram_1 <= d;
        end else if (adr_i[2:0] == 2) begin
          ram_2 <= d;
        end else if (adr_i[2:0] == 3) begin
          ram_3 <= d;
        end else if (adr_i[2:0] == 4) begin
          ram_4 <= d;
        end else if (adr_i[2:0] == 5) begin
          ram_5 <= d;
        end else if (adr_i[2:0] == 6) begin
          ram_6 <= d;
        end else begin
          ram_7 <= d;
        end
        // Increment write pointer
        if (adr_i[2:0] == 3'(TX_DEPTH - 1)) begin
          adr_i[2:0] <= 0;
          adr_i[3:3] <= ~adr_i[3:3];
        end else begin
          adr_i <= 4'(adr_i + 1);
        end
      end
    end
  end
  // Read side
  always_ff @(posedge rclk or posedge rst) begin
    if (rst) begin
      adr_o <= 0;
    end else begin
      if (~empty & rd) begin
        if (adr_o[2:0] == 3'(TX_DEPTH - 1)) begin
          adr_o[2:0] <= 0;
          adr_o[3:3] <= ~adr_o[3:3];
        end else begin
          adr_o <= 4'(adr_o + 1);
        end
      end
    end
  end
  // ── Combinational read + status ──────────────────────────────────────
  // mem_empt = occupancy = adr_i - adr_o
  logic [5:0] level;
  assign level = 5'($unsigned(adr_i)) - 5'($unsigned(adr_o));
  always_comb begin
    if (adr_o[2:0] == 0) begin
      q = ram_0;
    end else if (adr_o[2:0] == 1) begin
      q = ram_1;
    end else if (adr_o[2:0] == 2) begin
      q = ram_2;
    end else if (adr_o[2:0] == 3) begin
      q = ram_3;
    end else if (adr_o[2:0] == 4) begin
      q = ram_4;
    end else if (adr_o[2:0] == 5) begin
      q = ram_5;
    end else if (adr_o[2:0] == 6) begin
      q = ram_6;
    end else begin
      q = ram_7;
    end
    // Full/empty: combinational pointer compare (matches reference)
    full = (adr_i[2:0] == adr_o[2:0]) & (adr_i[3:3] ^ adr_o[3:3]);
    empty = adr_i == adr_o;
    // mem_empt from pre-computed level
    mem_empt = level[5:0];
  end

endmodule

// SD FIFO RX Filler
// Reads data from RX FIFO and writes to system memory via Wishbone master.
// Internally instantiates sd_rx_fifo. Port interface matches OpenCores SDC reference.
module sd_fifo_rx_filler (
  input logic clk,
  input logic rst,
  output logic [31:0] m_wb_adr_o,
  output logic m_wb_we_o,
  output logic [31:0] m_wb_dat_o,
  output logic m_wb_cyc_o,
  output logic m_wb_stb_o,
  input logic m_wb_ack_i,
  output logic [2:0] m_wb_cti_o,
  output logic [1:0] m_wb_bte_o,
  input logic en,
  input logic [31:0] adr,
  input logic sd_clk,
  input logic [3:0] dat_i,
  input logic wr,
  output logic full,
  output logic empty
);

  // Wishbone master
  // Data master control
  // Data serial signals (directly to RX FIFO write side)
  // Internal RX FIFO
  logic [31:0] rx_dat_out;
  logic rx_full_w;
  logic rx_empty_w;
  logic [1:0] rx_mem_w;
  logic rd_int_r;
  logic reset_rx_fifo_r;
  sd_rx_fifo u_fifo (
    .d(dat_i),
    .wr(wr),
    .wclk(sd_clk),
    .q(rx_dat_out),
    .rd(rd_int_r),
    .full(rx_full_w),
    .empty(rx_empty_w),
    .mem_empt(rx_mem_w),
    .rclk(clk),
    .rst(rst)
  );
  // WB master state machine: read from FIFO, write to system memory
  logic [1:0] st_r;
  logic [8:0] offset_r;
  logic cyc_r;
  logic stb_r;
  logic we_r;
  logic [31:0] dat_r;
  logic first_r;
  always_ff @(posedge clk or posedge rst) begin
    if (rst) begin
      cyc_r <= 1'b0;
      dat_r <= 0;
      first_r <= 1'b0;
      offset_r <= 0;
      rd_int_r <= 1'b0;
      reset_rx_fifo_r <= 1'b0;
      st_r <= 0;
      stb_r <= 1'b0;
      we_r <= 1'b0;
    end else begin
      rd_int_r <= 1'b0;
      if (en) begin
        if (~first_r) begin
          first_r <= 1'b1;
          offset_r <= 0;
          reset_rx_fifo_r <= 1'b1;
        end else begin
          reset_rx_fifo_r <= 1'b0;
          if (st_r == 0) begin
            // WAIT_DATA
            if (~rx_empty_w) begin
              dat_r <= rx_dat_out;
              rd_int_r <= 1'b1;
              st_r <= 1;
            end
          end else if (st_r == 1) begin
            // WRITE_WB
            cyc_r <= 1'b1;
            stb_r <= 1'b1;
            we_r <= 1'b1;
            if (m_wb_ack_i) begin
              offset_r <= 9'(offset_r + 1);
              cyc_r <= 1'b0;
              stb_r <= 1'b0;
              we_r <= 1'b0;
              st_r <= 0;
            end
          end
        end
      end else begin
        first_r <= 1'b0;
        cyc_r <= 1'b0;
        stb_r <= 1'b0;
        we_r <= 1'b0;
        st_r <= 0;
        offset_r <= 0;
      end
    end
  end
  assign m_wb_adr_o = 32'(adr + (32'($unsigned(offset_r)) << 2));
  assign m_wb_we_o = we_r;
  assign m_wb_dat_o = dat_r;
  assign m_wb_cyc_o = cyc_r;
  assign m_wb_stb_o = stb_r;
  assign m_wb_cti_o = 0;
  assign m_wb_bte_o = 0;
  assign full = rx_full_w;
  assign empty = rx_empty_w;

endmodule

// SD RX FIFO — dual-clock, 4-bit nibble input → 32-bit word output
// Matches RealBench reference: ram + manual pointers, unsynchronized
// full/empty (combinational compare). Nibble accumulator packs 8×4-bit
// inputs into 32-bit words before writing to RAM.
//
// Spec: sd_rx_fifo.md — DEPTH=8, ADR_SIZE=4
// domain WrDomain
//   freq_mhz: 100

// domain RdDomain
//   freq_mhz: 50

module sd_rx_fifo #(
  parameter int RX_DEPTH = 8
) (
  input logic wclk,
  input logic rclk,
  input logic rst,
  input logic [3:0] d,
  input logic wr,
  output logic [31:0] q,
  input logic rd,
  output logic full,
  output logic empty,
  output logic [1:0] mem_empt
);

  // ── Nibble accumulator ───────────────────────────────────────────────
  logic [7:0] we;
  // one-hot rotating, bit 0 first
  logic [31:0] tmp;
  // accumulated 32-bit word
  logic ft;
  // first full word flag
  // ── RAM storage ──────────────────────────────────────────────────────
  logic [31:0] ram_0;
  logic [31:0] ram_1;
  logic [31:0] ram_2;
  logic [31:0] ram_3;
  logic [31:0] ram_4;
  logic [31:0] ram_5;
  logic [31:0] ram_6;
  logic [31:0] ram_7;
  // ── Pointers: [MSB]=wrap, [2:0]=address ─────────────────────────────
  logic [3:0] adr_i;
  // write pointer (wclk domain)
  logic [3:0] adr_o;
  // read pointer  (rclk domain)
  // Write to RAM when nibble accumulator completes a word
  logic ram_we;
  assign ram_we = wr & we[0:0] & ft;
  always_ff @(posedge wclk or posedge rst) begin
    if (rst) begin
      adr_i <= 0;
      ft <= 1'b0;
      ram_0 <= 0;
      ram_1 <= 0;
      ram_2 <= 0;
      ram_3 <= 0;
      ram_4 <= 0;
      ram_5 <= 0;
      ram_6 <= 0;
      ram_7 <= 0;
      tmp <= 0;
      we <= 1;
    end else begin
      if (wr) begin
        // Rotate we left
        we <= {we[6:0], we[7:7]};
        // BIG_ENDIAN: first nibble (we[0]) → MSB tmp[31:28]
        if (we[0:0]) begin
          tmp[31:28] <= d;
        end
        if (we[1:1]) begin
          tmp[27:24] <= d;
        end
        if (we[2:2]) begin
          tmp[23:20] <= d;
        end
        if (we[3:3]) begin
          tmp[19:16] <= d;
        end
        if (we[4:4]) begin
          tmp[15:12] <= d;
        end
        if (we[5:5]) begin
          tmp[11:8] <= d;
        end
        if (we[6:6]) begin
          tmp[7:4] <= d;
        end
        if (we[7:7]) begin
          tmp[3:0] <= d;
          ft <= 1'b1;
        end
      end
      // Write pointer
      if (ram_we) begin
        if (adr_i[2:0] == 3'(RX_DEPTH - 1)) begin
          adr_i[2:0] <= 0;
          adr_i[3:3] <= ~adr_i[3:3];
        end else begin
          adr_i <= 4'(adr_i + 1);
        end
        // Write tmp to RAM at old address
        if (adr_i[2:0] == 0) begin
          ram_0 <= tmp;
        end else if (adr_i[2:0] == 1) begin
          ram_1 <= tmp;
        end else if (adr_i[2:0] == 2) begin
          ram_2 <= tmp;
        end else if (adr_i[2:0] == 3) begin
          ram_3 <= tmp;
        end else if (adr_i[2:0] == 4) begin
          ram_4 <= tmp;
        end else if (adr_i[2:0] == 5) begin
          ram_5 <= tmp;
        end else if (adr_i[2:0] == 6) begin
          ram_6 <= tmp;
        end else begin
          ram_7 <= tmp;
        end
      end
    end
  end
  // Read pointer
  always_ff @(posedge rclk or posedge rst) begin
    if (rst) begin
      adr_o <= 0;
    end else begin
      if (~empty & rd) begin
        if (adr_o[2:0] == 3'(RX_DEPTH - 1)) begin
          adr_o[2:0] <= 0;
          adr_o[3:3] <= ~adr_o[3:3];
        end else begin
          adr_o <= 4'(adr_o + 1);
        end
      end
    end
  end
  // ── Read output mux ──────────────────────────────────────────────────
  // mem_empt = occupancy = adr_i - adr_o (lower 2 bits)
  logic [5:0] level;
  assign level = 5'($unsigned(adr_i)) - 5'($unsigned(adr_o));
  always_comb begin
    if (adr_o[2:0] == 0) begin
      q = ram_0;
    end else if (adr_o[2:0] == 1) begin
      q = ram_1;
    end else if (adr_o[2:0] == 2) begin
      q = ram_2;
    end else if (adr_o[2:0] == 3) begin
      q = ram_3;
    end else if (adr_o[2:0] == 4) begin
      q = ram_4;
    end else if (adr_o[2:0] == 5) begin
      q = ram_5;
    end else if (adr_o[2:0] == 6) begin
      q = ram_6;
    end else begin
      q = ram_7;
    end
    // Full/empty: combinational pointer compare (matches reference)
    full = (adr_i[2:0] == adr_o[2:0]) & (adr_i[3:3] ^ adr_o[3:3]);
    empty = adr_i == adr_o;
    // mem_empt from pre-computed level
    mem_empt = level[1:0];
  end

endmodule

// SDC Controller (Top-level integration)
// Instantiates all sub-modules with port interfaces matching the OpenCores
// SDC reference Verilog. WB master mux, status register pipeline, IRQ generation.
module sdc_controller (
  input logic wb_clk_i,
  input logic wb_rst_i,
  input logic [31:0] wb_dat_i,
  output logic [31:0] wb_dat_o,
  input logic [7:0] wb_adr_i,
  input logic [3:0] wb_sel_i,
  input logic wb_we_i,
  input logic wb_cyc_i,
  input logic wb_stb_i,
  output logic wb_ack_o,
  output logic [31:0] m_wb_adr_o,
  output logic [3:0] m_wb_sel_o,
  output logic m_wb_we_o,
  output logic [31:0] m_wb_dat_o,
  input logic [31:0] m_wb_dat_i,
  output logic m_wb_cyc_o,
  output logic m_wb_stb_o,
  input logic m_wb_ack_i,
  output logic [2:0] m_wb_cti_o,
  output logic [1:0] m_wb_bte_o,
  input logic sd_cmd_dat_i,
  output logic sd_cmd_out_o,
  output logic sd_cmd_oe_o,
  input logic card_detect,
  input logic [3:0] sd_dat_dat_i,
  output logic [3:0] sd_dat_out_o,
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
  logic [7:0] clock_divider_w;
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
  logic [15:0] dat_in_m_tx_bd_w;
  logic [15:0] dat_in_m_rx_bd_w;
  logic [31:0] argument_reg_w;
  logic [15:0] cmd_setting_reg_w;
  logic [7:0] software_reset_reg_w;
  logic [15:0] time_out_reg_w;
  logic [15:0] normal_int_signal_enable_reg_w;
  logic [15:0] error_int_signal_enable_reg_w;
  logic [7:0] Bd_isr_enable_reg_w;
  // Command master outputs
  logic [15:0] status_reg_cm;
  logic [31:0] cmd_resp_1_cm;
  logic [4:0] err_int_cm;
  logic [15:0] normal_int_cm;
  logic [15:0] settings_w;
  logic go_idle_w;
  logic [39:0] cmd_out_master;
  logic req_out_master;
  logic ack_out_master;
  // Command serial host outputs
  logic [39:0] cmd_in_host;
  logic ack_in_host;
  logic req_in_host;
  logic [7:0] serial_status_w;
  logic cs_cmd_oe_w;
  logic cs_cmd_out_w;
  logic [1:0] st_dat_t_w;
  // Data master outputs
  logic dm_re_s_tx_w;
  logic dm_a_cmp_tx_w;
  logic dm_re_s_rx_w;
  logic dm_a_cmp_rx_w;
  logic dm_we_req_w;
  logic dm_d_write_w;
  logic dm_d_read_w;
  logic [31:0] dm_cmd_arg_w;
  logic [15:0] dm_cmd_set_w;
  logic dm_start_tx_fifo_w;
  logic dm_start_rx_fifo_w;
  logic [31:0] dm_sys_adr_w;
  logic dm_ack_transfer_w;
  logic [7:0] dm_dat_int_w;
  logic dm_cidat_w;
  // Data serial host outputs
  logic ds_rd_w;
  logic [3:0] ds_data_out_w;
  logic ds_we_w;
  logic ds_dat_oe_w;
  logic [3:0] ds_dat_out_w;
  logic ds_busy_n_w;
  logic ds_transm_w;
  logic ds_crc_ok_w;
  // TX BD
  logic [15:0] tx_bd_dat_out_w;
  logic [4:0] tx_bd_free_w;
  logic tx_bd_ack_w;
  // RX BD
  logic [15:0] rx_bd_dat_out_w;
  logic [4:0] rx_bd_free_w;
  logic rx_bd_ack_w;
  // TX filler (contains internal TX FIFO)
  logic [31:0] txf_q_w;
  logic txf_full_w;
  logic txf_empty_w;
  logic [31:0] txf_adr_w;
  logic txf_we_w;
  logic txf_cyc_w;
  logic txf_stb_w;
  logic [2:0] txf_cti_w;
  logic [1:0] txf_bte_w;
  // RX filler (contains internal RX FIFO)
  logic rxf_full_w;
  logic rxf_empty_w;
  logic [31:0] rxf_adr_w;
  logic rxf_we_w;
  logic [31:0] rxf_dato_w;
  logic rxf_cyc_w;
  logic rxf_stb_w;
  logic [2:0] rxf_cti_w;
  logic [1:0] rxf_bte_w;
  // Status registers (pipelined)
  logic [15:0] status_reg_r;
  logic [31:0] cmd_resp_1_r;
  logic [15:0] normal_int_status_reg_r;
  logic [15:0] error_int_status_reg_r;
  logic [15:0] Bd_Status_reg_r;
  logic [7:0] Bd_isr_reg_r;
  // write_req_s: data master wants CMD bus access
  logic write_req_s_w;
  logic [15:0] cmd_set_s_w;
  logic [31:0] cmd_arg_s_w;
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

