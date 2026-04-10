// SPI FSM — serializes 16-bit data via SPI protocol (MSB first).
// States: Idle(00) → Transmit(01) → ClkToggle(10), Error(11).
module spi_fsm (
  input logic i_clk,
  input logic i_rst_b,
  input logic [16-1:0] i_data_in,
  input logic i_enable,
  input logic i_fault,
  input logic i_clear,
  output logic o_spi_cs_b,
  output logic o_spi_clk,
  output logic o_spi_data,
  output logic [5-1:0] o_bits_left,
  output logic o_done,
  output logic [2-1:0] o_fsm_state
);

  logic [16-1:0] shift_r;
  logic [2-1:0] state_r;
  always_ff @(posedge i_clk or negedge i_rst_b) begin
    if ((!i_rst_b)) begin
      o_bits_left <= 5'd16;
      o_done <= 1'b0;
      o_fsm_state <= 0;
      o_spi_clk <= 1'b0;
      o_spi_cs_b <= 1'b1;
      o_spi_data <= 1'b0;
      shift_r <= 0;
      state_r <= 0;
    end else begin
      o_done <= 1'b0;
      if (i_clear) begin
        state_r <= 0;
        o_spi_cs_b <= 1'b1;
        o_spi_clk <= 1'b0;
        o_spi_data <= 1'b0;
        o_bits_left <= 5'd16;
        o_fsm_state <= 0;
      end else if (i_fault & state_r != 2'd3) begin
        state_r <= 2'd3;
        o_spi_cs_b <= 1'b1;
        o_spi_clk <= 1'b0;
        o_spi_data <= 1'b0;
        o_bits_left <= 5'd16;
        o_fsm_state <= 2'd3;
      end else if (~i_enable & state_r != 2'd3) begin
        state_r <= 0;
        o_spi_cs_b <= 1'b1;
        o_spi_clk <= 1'b0;
        o_spi_data <= 1'b0;
        o_bits_left <= 5'd16;
        o_fsm_state <= 0;
      end else if (state_r == 0) begin
        // IDLE: wait for enable to start transmission
        if (i_enable) begin
          state_r <= 2'd1;
          o_spi_cs_b <= 1'b0;
          o_spi_data <= i_data_in[15];
          shift_r <= i_data_in;
          o_fsm_state <= 2'd1;
          o_bits_left <= 5'd15;
        end
      end else if (state_r == 2'd1) begin
        // TRANSMIT: data is on the line, raise spi_clk
        o_spi_clk <= 1'b1;
        o_fsm_state <= 2'd2;
        state_r <= 2'd2;
      end else if (state_r == 2'd2) begin
        // CLK_TOGGLE: lower spi_clk, shift to next bit
        o_spi_clk <= 1'b0;
        if (o_bits_left == 0) begin
          // Transmission complete
          o_done <= 1'b1;
          o_spi_cs_b <= 1'b1;
          o_spi_data <= 1'b0;
          o_bits_left <= 5'd16;
          o_fsm_state <= 0;
          state_r <= 0;
        end else begin
          o_spi_data <= shift_r[14];
          shift_r <= shift_r << 1;
          o_bits_left <= 5'(o_bits_left - 1);
          o_fsm_state <= 2'd1;
          state_r <= 2'd1;
        end
      end else if (state_r == 2'd3) begin
        // ERROR: hold safe values
        o_spi_cs_b <= 1'b1;
        o_spi_clk <= 1'b0;
        o_spi_data <= 1'b0;
        o_bits_left <= 5'd16;
        o_fsm_state <= 2'd3;
      end
    end
  end

endmodule

