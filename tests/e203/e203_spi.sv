// E203 SPI Master Peripheral
// APB-accessible SPI master with configurable clock divider, CPOL/CPHA.
// Registers: txdata(0x00), rxdata(0x04), ctrl(0x08), div(0x0C), status(0x10)
// ctrl: bit 0=enable, bit 1=CPOL, bit 2=CPHA
// Single-byte shift-register transfer.
// domain SysDomain
//   freq_mhz: 100

module Spi (
  input logic clk,
  input logic rst_n,
  input logic psel,
  input logic penable,
  input logic [32-1:0] paddr,
  input logic [32-1:0] pwdata,
  input logic pwrite,
  output logic [32-1:0] prdata,
  output logic pready,
  output logic spi_sclk,
  output logic spi_mosi,
  input logic spi_miso,
  output logic spi_cs_n,
  output logic spi_irq
);

  // APB slave interface
  // SPI pins
  // Interrupt
  // ── Registers ──────────────────────────────────────────────────
  logic ctrl_en_r = 1'b0;
  logic ctrl_cpol_r = 1'b0;
  logic ctrl_cpha_r = 1'b0;
  logic [8-1:0] spi_div_r = 4;
  logic [8-1:0] div_cnt_r = 0;
  // Transfer state
  logic [8-1:0] tx_shift_r = 0;
  logic [8-1:0] rx_shift_r = 0;
  logic [4-1:0] bit_cnt_r = 0;
  logic busy_r = 1'b0;
  logic sclk_r = 1'b0;
  logic cs_n_r = 1'b1;
  logic done_r = 1'b0;
  logic phase_r = 1'b0;
  // 0=setup, 1=sample
  logic [8-1:0] reg_off;
  assign reg_off = paddr[7:0];
  logic apb_wr;
  assign apb_wr = ((psel & penable) & pwrite);
  logic div_tick;
  assign div_tick = (div_cnt_r == 0);
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      bit_cnt_r <= 0;
      busy_r <= 1'b0;
      cs_n_r <= 1'b1;
      ctrl_cpha_r <= 1'b0;
      ctrl_cpol_r <= 1'b0;
      ctrl_en_r <= 1'b0;
      div_cnt_r <= 0;
      done_r <= 1'b0;
      phase_r <= 1'b0;
      rx_shift_r <= 0;
      sclk_r <= 1'b0;
      spi_div_r <= 4;
      tx_shift_r <= 0;
    end else begin
      if ((div_cnt_r == 0)) begin
        div_cnt_r <= spi_div_r;
      end else begin
        div_cnt_r <= 8'((div_cnt_r - 1));
      end
      if ((((psel & penable) & (~pwrite)) & (reg_off == 'h10))) begin
        done_r <= 1'b0;
      end
      if ((((apb_wr & (reg_off == 'h0)) & (~busy_r)) & ctrl_en_r)) begin
        tx_shift_r <= pwdata[7:0];
        bit_cnt_r <= 8;
        busy_r <= 1'b1;
        cs_n_r <= 1'b0;
        phase_r <= 1'b0;
        sclk_r <= ctrl_cpol_r;
      end else if ((busy_r & div_tick)) begin
        if ((~phase_r)) begin
          sclk_r <= (~sclk_r);
          phase_r <= 1'b1;
        end else begin
          rx_shift_r <= {rx_shift_r[6:0], spi_miso};
          tx_shift_r <= {tx_shift_r[6:0], 1'b0};
          sclk_r <= (~sclk_r);
          phase_r <= 1'b0;
          bit_cnt_r <= 4'((bit_cnt_r - 1));
          if ((bit_cnt_r == 1)) begin
            busy_r <= 1'b0;
            cs_n_r <= 1'b1;
            done_r <= 1'b1;
            sclk_r <= ctrl_cpol_r;
          end
        end
      end
      if (apb_wr) begin
        if ((reg_off == 'h8)) begin
          ctrl_en_r <= (pwdata[0:0] != 0);
          ctrl_cpol_r <= (pwdata[1:1] != 0);
          ctrl_cpha_r <= (pwdata[2:2] != 0);
        end else if ((reg_off == 'hC)) begin
          spi_div_r <= pwdata[7:0];
        end
      end
    end
  end
  // Clock divider
  // Clear done on status read
  // Start transfer
  // Setup phase: drive MOSI, toggle clock
  // Sample phase: read MISO, shift
  // Register writes
  always_comb begin
    spi_sclk = sclk_r;
    spi_mosi = (tx_shift_r[7:7] != 0);
    spi_cs_n = cs_n_r;
    if ((reg_off == 'h0)) begin
      prdata = {busy_r, {31{1'b0}}};
    end else if ((reg_off == 'h4)) begin
      prdata = 32'($unsigned(rx_shift_r));
    end else if ((reg_off == 'h8)) begin
      prdata = 32'($unsigned({ctrl_cpha_r, ctrl_cpol_r, ctrl_en_r}));
    end else if ((reg_off == 'hC)) begin
      prdata = 32'($unsigned(spi_div_r));
    end else if ((reg_off == 'h10)) begin
      prdata = 32'($unsigned({done_r, busy_r}));
    end else begin
      prdata = 0;
    end
    pready = 1'b1;
    spi_irq = done_r;
  end

endmodule

// APB read
