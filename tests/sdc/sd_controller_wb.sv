// SD Controller Wishbone Slave Interface
// Register bank for SD controller. Matches OpenCores SDC reference port interface.
// Uses RAM_MEM_WIDTH=16 (ACTEL config from sd_defines.v).
module sd_controller_wb (
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
  output logic we_m_tx_bd,
  output logic new_cmd,
  output logic we_ack,
  output logic int_ack,
  output logic cmd_int_busy,
  output logic we_m_rx_bd,
  output logic int_busy,
  input logic write_req_s,
  input logic [16-1:0] cmd_set_s,
  input logic [32-1:0] cmd_arg_s,
  output logic [32-1:0] argument_reg,
  output logic [16-1:0] cmd_setting_reg,
  output logic [8-1:0] software_reset_reg,
  output logic [16-1:0] time_out_reg,
  output logic [16-1:0] normal_int_signal_enable_reg,
  output logic [16-1:0] error_int_signal_enable_reg,
  output logic [8-1:0] clock_divider,
  output logic [8-1:0] Bd_isr_enable_reg,
  input logic [16-1:0] status_reg,
  input logic [32-1:0] cmd_resp_1,
  input logic [16-1:0] normal_int_status_reg,
  input logic [16-1:0] error_int_status_reg,
  input logic [16-1:0] Bd_Status_reg,
  input logic [8-1:0] Bd_isr_reg,
  output logic Bd_isr_reset,
  output logic normal_isr_reset,
  output logic error_isr_reset,
  output logic [16-1:0] dat_in_m_rx_bd,
  output logic [16-1:0] dat_in_m_tx_bd
);

  // Wishbone slave
  // Control outputs
  // Data master cmd write request
  // Bus-accessible registers (outputs)
  // Status inputs
  // Register control
  // --- Internal registers ---
  logic [32-1:0] argument_reg_r;
  logic [16-1:0] cmd_setting_reg_r;
  logic [8-1:0] software_reset_reg_r;
  logic [16-1:0] time_out_reg_r;
  logic [16-1:0] normal_int_signal_enable_reg_r;
  logic [16-1:0] error_int_signal_enable_reg_r;
  logic [8-1:0] clock_divider_r;
  logic [8-1:0] Bd_isr_enable_reg_r;
  logic [2-1:0] we_r;
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
  logic [16-1:0] dat_in_m_rx_bd_r;
  logic [16-1:0] dat_in_m_tx_bd_r;
  // Combinational int_ack (blocking in reference)
  logic int_ack_w;
  // Combinational wb_ack (blocking in reference)
  logic wb_ack_comb;
  logic [32-1:0] wb_dat_o_r;
  // Register address constants
  // argument=0x00, command=0x04, status=0x08, resp1=0x0c,
  // controller=0x1c, block=0x20, power=0x24, software=0x28,
  // timeout=0x2c, normal_isr=0x30, error_isr=0x34, normal_iser=0x38,
  // error_iser=0x3c, capa=0x48, clock_d=0x4c, bd_status=0x50,
  // bd_isr=0x54, bd_iser=0x58, bd_rx=0x60, bd_tx=0x80
  // Parameters (from sd_defines.v with ACTEL + SD_BUS_WIDTH_4 + SUPPLY_VOLTAGE_3_3)
  logic [8-1:0] power_controll_reg;
  assign power_controll_reg = 8'd15;
  logic [32-1:0] block_size_reg;
  assign block_size_reg = 512;
  logic [16-1:0] controll_setting_reg;
  assign controll_setting_reg = 16'd2;
  logic [16-1:0] capabilies_reg;
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
      if (wb_stb_i & wb_cyc_i | wb_ack_o_r) begin
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
    if (wb_stb_i & wb_cyc_i | wb_ack_o_r) begin
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
  logic [32-1:0] wb_dat_o_rd;
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

