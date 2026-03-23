// E203 GPIO Peripheral
// 32-bit GPIO with APB interface.
// Registers: output_val(0x00), output_en(0x04), input_val(0x08),
//            rise_ie(0x0C), rise_ip(0x10), fall_ie(0x14), fall_ip(0x18)
// Directly maps to FPGA I/O pins.
// domain SysDomain
//   freq_mhz: 100

module Gpio #(
  parameter int WIDTH = 32
) (
  input logic clk,
  input logic rst_n,
  input logic psel,
  input logic penable,
  input logic [32-1:0] paddr,
  input logic [32-1:0] pwdata,
  input logic pwrite,
  output logic [32-1:0] prdata,
  output logic pready,
  input logic [32-1:0] gpio_in,
  output logic [32-1:0] gpio_out,
  output logic [32-1:0] gpio_oe,
  output logic gpio_irq
);

  // APB slave interface
  // GPIO pins
  // output enable
  // interrupt output
  // ── Registers ──────────────────────────────────────────────────
  logic [32-1:0] out_val_r = 0;
  logic [32-1:0] out_en_r = 0;
  logic [32-1:0] rise_ie_r = 0;
  // rise interrupt enable
  logic [32-1:0] rise_ip_r = 0;
  // rise interrupt pending
  logic [32-1:0] fall_ie_r = 0;
  // fall interrupt enable
  logic [32-1:0] fall_ip_r = 0;
  // fall interrupt pending
  logic [32-1:0] gpio_prev_r = 0;
  // previous gpio_in value
  // Edge detection
  logic [32-1:0] rise_edge;
  assign rise_edge = (gpio_in & (~gpio_prev_r));
  logic [32-1:0] fall_edge;
  assign fall_edge = ((~gpio_in) & gpio_prev_r);
  // Register offset
  logic [8-1:0] reg_off;
  assign reg_off = paddr[7:0];
  logic apb_wr;
  assign apb_wr = ((psel & penable) & pwrite);
  logic apb_rd;
  assign apb_rd = ((psel & penable) & (~pwrite));
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      fall_ie_r <= 0;
      fall_ip_r <= 0;
      gpio_prev_r <= 0;
      out_en_r <= 0;
      out_val_r <= 0;
      rise_ie_r <= 0;
      rise_ip_r <= 0;
    end else begin
      gpio_prev_r <= gpio_in;
      rise_ip_r <= (rise_ip_r | rise_edge);
      fall_ip_r <= (fall_ip_r | fall_edge);
      if (apb_wr) begin
        if ((reg_off == 'h0)) begin
          out_val_r <= pwdata;
        end else if ((reg_off == 'h4)) begin
          out_en_r <= pwdata;
        end else if ((reg_off == 'hC)) begin
          rise_ie_r <= pwdata;
        end else if ((reg_off == 'h10)) begin
          rise_ip_r <= (rise_ip_r & (~pwdata));
        end else if ((reg_off == 'h14)) begin
          fall_ie_r <= pwdata;
        end else if ((reg_off == 'h18)) begin
          fall_ip_r <= (fall_ip_r & (~pwdata));
        end
      end
    end
  end
  // Set interrupt pending on edges
  // Write 1 to clear pending
  always_comb begin
    if ((reg_off == 'h0)) begin
      prdata = out_val_r;
    end else if ((reg_off == 'h4)) begin
      prdata = out_en_r;
    end else if ((reg_off == 'h8)) begin
      prdata = gpio_in;
    end else if ((reg_off == 'hC)) begin
      prdata = rise_ie_r;
    end else if ((reg_off == 'h10)) begin
      prdata = rise_ip_r;
    end else if ((reg_off == 'h14)) begin
      prdata = fall_ie_r;
    end else if ((reg_off == 'h18)) begin
      prdata = fall_ip_r;
    end else begin
      prdata = 0;
    end
    pready = 1'b1;
    gpio_out = out_val_r;
    gpio_oe = out_en_r;
    gpio_irq = (((rise_ip_r & rise_ie_r) | (fall_ip_r & fall_ie_r)) != 0);
  end

endmodule

// APB read
// single-cycle APB
