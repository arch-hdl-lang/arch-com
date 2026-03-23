// E203 UART Peripheral (simplified)
// APB-accessible UART with TX/RX, configurable baud divisor.
// Registers: txdata(0x00), rxdata(0x04), txctrl(0x08), rxctrl(0x0C),
//            div(0x10), status(0x14)
// TX: write to txdata starts transmission.
// RX: read from rxdata returns received byte.
// Uses shift-register based serial TX/RX.
// domain SysDomain
//   freq_mhz: 100

module Uart (
  input logic clk,
  input logic rst_n,
  input logic psel,
  input logic penable,
  input logic [32-1:0] paddr,
  input logic [32-1:0] pwdata,
  input logic pwrite,
  output logic [32-1:0] prdata,
  output logic pready,
  output logic uart_tx,
  input logic uart_rx,
  output logic uart_irq
);

  // APB slave interface
  // UART pins
  // Interrupt
  // ── Baud rate divider ──────────────────────────────────────────
  logic [16-1:0] baud_div_r = 1;
  // configurable; default=1 for sim
  logic [16-1:0] baud_cnt_r = 0;
  logic baud_tick;
  assign baud_tick = (baud_cnt_r == 0);
  // ── TX state ───────────────────────────────────────────────────
  logic [10-1:0] tx_shift_r = 'h3FF;
  // idle=all 1s: {stop, data[7:0], start}
  logic [4-1:0] tx_cnt_r = 0;
  // bit counter (0=idle)
  logic tx_busy_r = 1'b0;
  // ── RX state ───────────────────────────────────────────────────
  logic [8-1:0] rx_shift_r = 0;
  logic [4-1:0] rx_cnt_r = 0;
  logic rx_valid_r = 1'b0;
  logic [8-1:0] rx_data_r = 0;
  logic rx_prev_r = 1'b1;
  // previous rx pin
  // ── TX/RX enable ───────────────────────────────────────────────
  logic txen_r = 1'b0;
  logic rxen_r = 1'b0;
  // APB decode
  logic [8-1:0] reg_off;
  assign reg_off = paddr[7:0];
  logic apb_wr;
  assign apb_wr = ((psel & penable) & pwrite);
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      baud_cnt_r <= 0;
      baud_div_r <= 1;
      rx_cnt_r <= 0;
      rx_data_r <= 0;
      rx_prev_r <= 1'b1;
      rx_shift_r <= 0;
      rx_valid_r <= 1'b0;
      rxen_r <= 1'b0;
      tx_busy_r <= 1'b0;
      tx_cnt_r <= 0;
      tx_shift_r <= 'h3FF;
      txen_r <= 1'b0;
    end else begin
      if ((baud_cnt_r == 0)) begin
        baud_cnt_r <= baud_div_r;
      end else begin
        baud_cnt_r <= 16'((baud_cnt_r - 1));
      end
      if (((apb_wr & (reg_off == 'h0)) & (~tx_busy_r))) begin
        tx_shift_r <= {1'b1, pwdata[7:0], 1'b0};
        tx_cnt_r <= 10;
        tx_busy_r <= 1'b1;
      end else if ((tx_busy_r & baud_tick)) begin
        tx_shift_r <= {1'b1, tx_shift_r[9:1]};
        tx_cnt_r <= 4'((tx_cnt_r - 1));
        if ((tx_cnt_r == 1)) begin
          tx_busy_r <= 1'b0;
        end
      end
      rx_prev_r <= uart_rx;
      if (rxen_r) begin
        if ((rx_cnt_r == 0)) begin
          if ((rx_prev_r & (~uart_rx))) begin
            rx_cnt_r <= 9;
          end
        end else if (baud_tick) begin
          if ((rx_cnt_r > 1)) begin
            rx_shift_r <= {uart_rx, rx_shift_r[7:1]};
          end
          rx_cnt_r <= 4'((rx_cnt_r - 1));
          if ((rx_cnt_r == 1)) begin
            rx_data_r <= {uart_rx, rx_shift_r[7:1]};
            rx_valid_r <= 1'b1;
          end
        end
      end
      if ((((psel & penable) & (~pwrite)) & (reg_off == 'h4))) begin
        rx_valid_r <= 1'b0;
      end
      if (apb_wr) begin
        if ((reg_off == 'h8)) begin
          txen_r <= (pwdata[0:0] != 0);
        end else if ((reg_off == 'hC)) begin
          rxen_r <= (pwdata[0:0] != 0);
        end else if ((reg_off == 'h10)) begin
          baud_div_r <= pwdata[15:0];
        end
      end
    end
  end
  // Baud counter
  // ── TX logic ─────────────────────────────────────────────────
  // Load TX shift register: {1(stop), data[7:0], 0(start)}
  // Shift out LSB first
  // ── RX logic ─────────────────────────────────────────────────
  // Wait for start bit (falling edge)
  // 1 start + 8 data
  // Sample data bits
  // Clear rx_valid on read
  // Register writes
  always_comb begin
    if (tx_busy_r) begin
      uart_tx = (tx_shift_r[0:0] != 0);
    end else begin
      uart_tx = 1'b1;
    end
    if ((reg_off == 'h0)) begin
      prdata = {tx_busy_r, {31{1'b0}}};
    end else if ((reg_off == 'h4)) begin
      prdata = {(~rx_valid_r), {23{1'b0}}, rx_data_r};
    end else if ((reg_off == 'h8)) begin
      prdata = 32'($unsigned(txen_r));
    end else if ((reg_off == 'hC)) begin
      prdata = 32'($unsigned(rxen_r));
    end else if ((reg_off == 'h10)) begin
      prdata = 32'($unsigned(baud_div_r));
    end else if ((reg_off == 'h14)) begin
      prdata = 32'($unsigned({tx_busy_r, rx_valid_r}));
    end else begin
      prdata = 0;
    end
    pready = 1'b1;
    uart_irq = rx_valid_r;
  end

endmodule

// TX pin
// idle high
// APB read
// bit 31 = tx_busy (full)
// bit 31 = empty
// Status: bit 0 = tx_busy, bit 1 = rx_valid
