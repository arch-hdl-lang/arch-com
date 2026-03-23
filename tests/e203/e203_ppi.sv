// E203 Private Peripheral Interface
// ICB-to-APB bridge with address-based demux to multiple peripherals.
// Decodes ICB address to select one of N APB slaves.
// Base addresses: GPIO=0x10012000, UART=0x10013000, SPI=0x10014000, Timer=0x02000000
module Ppi #(
  parameter int NUM_SLAVES = 4
) (
  input logic clk,
  input logic rst_n,
  input logic icb_cmd_valid,
  output logic icb_cmd_ready,
  input logic [32-1:0] icb_cmd_addr,
  input logic [32-1:0] icb_cmd_wdata,
  input logic [4-1:0] icb_cmd_wmask,
  input logic icb_cmd_read,
  output logic icb_rsp_valid,
  input logic icb_rsp_ready,
  output logic [32-1:0] icb_rsp_rdata,
  output logic icb_rsp_err,
  output logic apb0_psel,
  output logic apb0_penable,
  output logic [32-1:0] apb0_paddr,
  output logic [32-1:0] apb0_pwdata,
  output logic apb0_pwrite,
  input logic [32-1:0] apb0_prdata,
  input logic apb0_pready,
  output logic apb1_psel,
  output logic apb1_penable,
  output logic [32-1:0] apb1_paddr,
  output logic [32-1:0] apb1_pwdata,
  output logic apb1_pwrite,
  input logic [32-1:0] apb1_prdata,
  input logic apb1_pready,
  output logic apb2_psel,
  output logic apb2_penable,
  output logic [32-1:0] apb2_paddr,
  output logic [32-1:0] apb2_pwdata,
  output logic apb2_pwrite,
  input logic [32-1:0] apb2_prdata,
  input logic apb2_pready,
  output logic apb3_psel,
  output logic apb3_penable,
  output logic [32-1:0] apb3_paddr,
  output logic [32-1:0] apb3_pwdata,
  output logic apb3_pwrite,
  input logic [32-1:0] apb3_prdata,
  input logic apb3_pready
);

  // ICB slave (from BIU)
  // APB master 0 (GPIO)
  // APB master 1 (UART)
  // APB master 2 (SPI)
  // APB master 3 (Timer/CLINT)
  // ── Address decode ─────────────────────────────────────────────
  logic [20-1:0] addr_hi;
  assign addr_hi = icb_cmd_addr[31:12];
  logic sel_gpio;
  assign sel_gpio = (addr_hi == 'h10012);
  logic sel_uart;
  assign sel_uart = (addr_hi == 'h10013);
  logic sel_spi;
  assign sel_spi = (addr_hi == 'h10014);
  logic sel_timer;
  assign sel_timer = (icb_cmd_addr[31:24] == 'h2);
  // ── FSM: IDLE → SETUP → ACCESS ────────────────────────────────
  logic [2-1:0] fsm_st = 0;
  logic [3-1:0] sel_r = 0;
  // latched slave select
  logic [32-1:0] cmd_addr_r = 0;
  logic [32-1:0] cmd_wdata_r = 0;
  logic cmd_read_r = 1'b0;
  logic rsp_valid_r = 1'b0;
  logic [32-1:0] rsp_rdata_r = 0;
  // Encode slave select
  logic [3-1:0] sel_enc;
  assign sel_enc = (sel_gpio) ? (1) : ((sel_uart) ? (2) : ((sel_spi) ? (3) : ((sel_timer) ? (4) : (0))));
  // Selected slave's pready and prdata
  logic [32-1:0] sel_prdata;
  logic sel_pready;
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      cmd_addr_r <= 0;
      cmd_read_r <= 1'b0;
      cmd_wdata_r <= 0;
      fsm_st <= 0;
      rsp_rdata_r <= 0;
      rsp_valid_r <= 1'b0;
      sel_r <= 0;
    end else begin
      if ((fsm_st == 0)) begin
        if (icb_cmd_valid) begin
          sel_r <= sel_enc;
          cmd_addr_r <= icb_cmd_addr;
          cmd_wdata_r <= icb_cmd_wdata;
          cmd_read_r <= icb_cmd_read;
          fsm_st <= 1;
        end
        if ((rsp_valid_r & icb_rsp_ready)) begin
          rsp_valid_r <= 1'b0;
        end
      end else if ((fsm_st == 1)) begin
        fsm_st <= 2;
      end else if ((fsm_st == 2)) begin
        if (sel_pready) begin
          rsp_rdata_r <= sel_prdata;
          rsp_valid_r <= 1'b1;
          fsm_st <= 0;
        end
      end
    end
  end
  logic in_setup;
  assign in_setup = (fsm_st == 1);
  logic in_access;
  assign in_access = (fsm_st == 2);
  always_comb begin
    apb0_paddr = cmd_addr_r;
    apb1_paddr = cmd_addr_r;
    apb2_paddr = cmd_addr_r;
    apb3_paddr = cmd_addr_r;
    apb0_pwdata = cmd_wdata_r;
    apb1_pwdata = cmd_wdata_r;
    apb2_pwdata = cmd_wdata_r;
    apb3_pwdata = cmd_wdata_r;
    apb0_pwrite = (~cmd_read_r);
    apb1_pwrite = (~cmd_read_r);
    apb2_pwrite = (~cmd_read_r);
    apb3_pwrite = (~cmd_read_r);
    apb0_psel = ((sel_r == 1) & (in_setup | in_access));
    apb0_penable = ((sel_r == 1) & in_access);
    apb1_psel = ((sel_r == 2) & (in_setup | in_access));
    apb1_penable = ((sel_r == 2) & in_access);
    apb2_psel = ((sel_r == 3) & (in_setup | in_access));
    apb2_penable = ((sel_r == 3) & in_access);
    apb3_psel = ((sel_r == 4) & (in_setup | in_access));
    apb3_penable = ((sel_r == 4) & in_access);
    if ((sel_r == 1)) begin
      sel_prdata = apb0_prdata;
      sel_pready = apb0_pready;
    end else if ((sel_r == 2)) begin
      sel_prdata = apb1_prdata;
      sel_pready = apb1_pready;
    end else if ((sel_r == 3)) begin
      sel_prdata = apb2_prdata;
      sel_pready = apb2_pready;
    end else if ((sel_r == 4)) begin
      sel_prdata = apb3_prdata;
      sel_pready = apb3_pready;
    end else begin
      sel_prdata = 0;
      sel_pready = 1'b1;
    end
    icb_cmd_ready = ((fsm_st == 0) & (~rsp_valid_r));
    icb_rsp_valid = rsp_valid_r;
    icb_rsp_rdata = rsp_rdata_r;
    icb_rsp_err = 1'b0;
  end

endmodule

// APB common signals
// Per-slave psel/penable
// Read data mux
// ICB interface
