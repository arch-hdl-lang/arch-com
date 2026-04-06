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
  input logic [32-1:0] ARG_REG,
  input logic [14-1:0] CMD_SET_REG,
  input logic [16-1:0] TIMEOUT_REG,
  output logic [16-1:0] STATUS_REG,
  output logic [32-1:0] RESP_1_REG,
  output logic [5-1:0] ERR_INT_REG,
  output logic [16-1:0] NORMAL_INT_REG,
  input logic ERR_INT_RST,
  input logic NORMAL_INT_RST,
  output logic [16-1:0] settings,
  output logic go_idle_o,
  output logic [40-1:0] cmd_out,
  output logic req_out,
  output logic ack_out,
  input logic req_in,
  input logic ack_in,
  input logic [40-1:0] cmd_in,
  input logic [8-1:0] serial_status,
  input logic card_detect
);

  // --- Internal registers ---
  // Input synchronizers (2-stage)
  logic req_q;
  logic req_in_int;
  logic ack_q;
  logic ack_in_int;
  // Card detect debounce
  logic [4-1:0] debounce_r;
  logic card_present_r;
  // FSM state (one-hot: IDLE=001, SETUP=010, EXECUTE=100)
  logic [3-1:0] state_r;
  // Registered outputs & internals
  logic CRC_check_enable_r;
  logic index_check_enable_r;
  logic [7-1:0] response_size_r;
  logic [16-1:0] status_r;
  logic [16-1:0] Watchdog_Cnt_r;
  logic [16-1:0] STATUS_REG_r;
  logic [32-1:0] RESP_1_REG_r;
  logic [5-1:0] ERR_INT_REG_r;
  logic [16-1:0] NORMAL_INT_REG_r;
  logic [16-1:0] settings_r;
  logic go_idle_o_r;
  logic [40-1:0] cmd_out_r;
  logic req_out_r;
  logic ack_out_r;
  // Combinational signals computed each cycle (blocking equivalents)
  logic complete_w;
  logic [3-1:0] next_state_w;
  logic [16-1:0] Watchdog_next;
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
  logic [6-1:0] CMDI;
  assign CMDI = CMD_SET_REG[13:8];
  logic [2-1:0] WORD_SELECT;
  assign WORD_SELECT = CMD_SET_REG[7:6];
  logic CICE;
  assign CICE = CMD_SET_REG[4];
  logic CRCE;
  assign CRCE = CMD_SET_REG[3];
  logic [2-1:0] RTS;
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
        if (RTS == 2'd2 | RTS == 2'd3) begin
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
        if (RTS == 2'd2 | RTS == 2'd3) begin
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
            if (index_check_enable_r & cmd_out_r[37:32] != cmd_in[37:32]) begin
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

