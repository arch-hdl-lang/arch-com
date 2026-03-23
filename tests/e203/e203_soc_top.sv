// E203 SoC Top-Level Integration
// Connects: CoreTop + ICB Arbiter + SRAM + PPI + FIO + GPIO + UART + SPI
//           + DebugModule + IrqCtrl
// The core's internal BIU handles ITCM/DTCM. External bus accesses
// (peripherals, SRAM) go through the ICB fabric.
module SocTop (
  input logic clk,
  input logic rst_n,
  input logic itcm_wr_en,
  input logic [14-1:0] itcm_wr_addr,
  input logic [32-1:0] itcm_wr_data,
  input logic ext_cmd_valid,
  input logic [32-1:0] ext_cmd_addr,
  input logic [32-1:0] ext_cmd_wdata,
  input logic [4-1:0] ext_cmd_wmask,
  input logic ext_cmd_read,
  output logic ext_cmd_ready,
  output logic ext_rsp_valid,
  output logic [32-1:0] ext_rsp_rdata,
  output logic ext_rsp_err,
  input logic [32-1:0] gpio_in,
  output logic [32-1:0] gpio_out,
  output logic [32-1:0] gpio_oe,
  output logic uart_tx,
  input logic uart_rx,
  output logic spi_sclk,
  output logic spi_mosi,
  input logic spi_miso,
  output logic spi_cs_n,
  input logic [32-1:0] fio_in_0,
  input logic [32-1:0] fio_in_1,
  output logic [32-1:0] fio_out_0,
  output logic [32-1:0] fio_out_1,
  input logic dbg_psel,
  input logic dbg_penable,
  input logic [32-1:0] dbg_paddr,
  input logic [32-1:0] dbg_pwdata,
  input logic dbg_pwrite,
  output logic [32-1:0] dbg_prdata,
  output logic core_commit,
  output logic [32-1:0] core_instr,
  output logic [32-1:0] core_pc,
  output logic core_valid,
  output logic gpio_irq,
  output logic uart_irq,
  output logic spi_irq
);

  // ── ITCM loader (testbench) ────────────────────────────────────
  // ── External ICB master (from core BIU, directly wired for SoC) ──
  // ── External pins ──────────────────────────────────────────────
  // ── Debug interface ────────────────────────────────────────────
  // ── Status ─────────────────────────────────────────────────────
  // ── Bus interconnect wires (driven in comb, consumed by inst) ──
  logic bus_cmd_ready;
  logic bus_rsp_valid;
  logic [32-1:0] bus_rsp_rdata;
  logic bus_rsp_err;
  // ══════════════════════════════════════════════════════════════
  // CPU Core
  // ══════════════════════════════════════════════════════════════
  logic core_commit_w;
  logic [32-1:0] core_instr_w;
  logic [32-1:0] core_pc_w;
  logic core_valid_w;
  logic tmr_irq_w;
  CoreTop core (
    .clk(clk),
    .rst_n(rst_n),
    .itcm_wr_en(itcm_wr_en),
    .itcm_wr_addr(itcm_wr_addr),
    .itcm_wr_data(itcm_wr_data),
    .exu_redirect(1'b0),
    .exu_redirect_pc(0),
    .commit_valid(core_commit_w),
    .o_instr(core_instr_w),
    .o_pc(core_pc_w),
    .o_valid(core_valid_w),
    .tmr_irq(tmr_irq_w)
  );
  // ══════════════════════════════════════════════════════════════
  // ICB Bus Arbiter: M0=external port, M1=unused → Slave bus
  // ══════════════════════════════════════════════════════════════
  logic arbt_m0_ready;
  logic arbt_m0_rsp_valid;
  logic [32-1:0] arbt_m0_rsp_rdata;
  logic arbt_m0_rsp_err;
  logic arbt_m1_ready;
  logic arbt_m1_rsp_valid;
  logic [32-1:0] arbt_m1_rsp_rdata;
  logic arbt_m1_rsp_err;
  logic bus_cmd_valid;
  logic [32-1:0] bus_cmd_addr;
  logic [32-1:0] bus_cmd_wdata;
  logic [4-1:0] bus_cmd_wmask;
  logic bus_cmd_read;
  logic bus_rsp_ready;
  IcbArbt arbt (
    .clk(clk),
    .rst_n(rst_n),
    .m0_cmd_valid(ext_cmd_valid),
    .m0_cmd_addr(ext_cmd_addr),
    .m0_cmd_wdata(ext_cmd_wdata),
    .m0_cmd_wmask(ext_cmd_wmask),
    .m0_cmd_read(ext_cmd_read),
    .m0_cmd_ready(arbt_m0_ready),
    .m0_rsp_valid(arbt_m0_rsp_valid),
    .m0_rsp_ready(1'b1),
    .m0_rsp_rdata(arbt_m0_rsp_rdata),
    .m0_rsp_err(arbt_m0_rsp_err),
    .m1_cmd_valid(1'b0),
    .m1_cmd_addr(0),
    .m1_cmd_wdata(0),
    .m1_cmd_wmask(0),
    .m1_cmd_read(1'b0),
    .m1_cmd_ready(arbt_m1_ready),
    .m1_rsp_valid(arbt_m1_rsp_valid),
    .m1_rsp_ready(1'b1),
    .m1_rsp_rdata(arbt_m1_rsp_rdata),
    .m1_rsp_err(arbt_m1_rsp_err),
    .s_cmd_valid(bus_cmd_valid),
    .s_cmd_ready(bus_cmd_ready),
    .s_cmd_addr(bus_cmd_addr),
    .s_cmd_wdata(bus_cmd_wdata),
    .s_cmd_wmask(bus_cmd_wmask),
    .s_cmd_read(bus_cmd_read),
    .s_rsp_valid(bus_rsp_valid),
    .s_rsp_ready(bus_rsp_ready),
    .s_rsp_rdata(bus_rsp_rdata),
    .s_rsp_err(bus_rsp_err)
  );
  // Address decode
  logic sel_sram;
  assign sel_sram = (bus_cmd_addr[31:28] == 2);
  logic sel_ppi;
  assign sel_ppi = (bus_cmd_addr[31:28] == 1);
  logic sel_fio;
  assign sel_fio = (bus_cmd_addr[31:28] == 3);
  // ══════════════════════════════════════════════════════════════
  // SRAM Controller
  // ══════════════════════════════════════════════════════════════
  logic sram_ready;
  logic sram_rsp_valid;
  logic [32-1:0] sram_rsp_rdata;
  logic sram_rsp_err;
  SramCtrl sram (
    .clk(clk),
    .rst_n(rst_n),
    .icb_cmd_valid((bus_cmd_valid & sel_sram)),
    .icb_cmd_ready(sram_ready),
    .icb_cmd_addr(bus_cmd_addr),
    .icb_cmd_wdata(bus_cmd_wdata),
    .icb_cmd_wmask(bus_cmd_wmask),
    .icb_cmd_read(bus_cmd_read),
    .icb_rsp_valid(sram_rsp_valid),
    .icb_rsp_ready(bus_rsp_ready),
    .icb_rsp_rdata(sram_rsp_rdata),
    .icb_rsp_err(sram_rsp_err)
  );
  // ══════════════════════════════════════════════════════════════
  // Fast I/O
  // ══════════════════════════════════════════════════════════════
  logic fio_ready;
  logic fio_rsp_valid;
  logic [32-1:0] fio_rsp_rdata;
  logic fio_rsp_err;
  logic [32-1:0] fio_out_0_w;
  logic [32-1:0] fio_out_1_w;
  logic [32-1:0] fio_out_2_w;
  logic [32-1:0] fio_out_3_w;
  Fio fio (
    .clk(clk),
    .rst_n(rst_n),
    .icb_cmd_valid((bus_cmd_valid & sel_fio)),
    .icb_cmd_ready(fio_ready),
    .icb_cmd_addr(bus_cmd_addr),
    .icb_cmd_wdata(bus_cmd_wdata),
    .icb_cmd_wmask(bus_cmd_wmask),
    .icb_cmd_read(bus_cmd_read),
    .icb_rsp_valid(fio_rsp_valid),
    .icb_rsp_ready(bus_rsp_ready),
    .icb_rsp_rdata(fio_rsp_rdata),
    .icb_rsp_err(fio_rsp_err),
    .fio_in_0(fio_in_0),
    .fio_in_1(fio_in_1),
    .fio_out_0(fio_out_0_w),
    .fio_out_1(fio_out_1_w),
    .fio_out_2(fio_out_2_w),
    .fio_out_3(fio_out_3_w)
  );
  // ══════════════════════════════════════════════════════════════
  // PPI: ICB → APB bridge with 4-slave address decode
  // ══════════════════════════════════════════════════════════════
  logic ppi_ready;
  logic ppi_rsp_valid;
  logic [32-1:0] ppi_rsp_rdata;
  logic ppi_rsp_err;
  logic gpio_psel;
  logic gpio_penable;
  logic [32-1:0] gpio_paddr;
  logic [32-1:0] gpio_pwdata;
  logic gpio_pwrite;
  logic uart_psel;
  logic uart_penable;
  logic [32-1:0] uart_paddr;
  logic [32-1:0] uart_pwdata;
  logic uart_pwrite;
  logic spi_psel;
  logic spi_penable;
  logic [32-1:0] spi_paddr;
  logic [32-1:0] spi_pwdata;
  logic spi_pwrite;
  logic apb3_psel_w;
  logic apb3_penable_w;
  logic [32-1:0] apb3_paddr_w;
  logic [32-1:0] apb3_pwdata_w;
  logic apb3_pwrite_w;
  Ppi ppi (
    .clk(clk),
    .rst_n(rst_n),
    .icb_cmd_valid((bus_cmd_valid & sel_ppi)),
    .icb_cmd_ready(ppi_ready),
    .icb_cmd_addr(bus_cmd_addr),
    .icb_cmd_wdata(bus_cmd_wdata),
    .icb_cmd_wmask(bus_cmd_wmask),
    .icb_cmd_read(bus_cmd_read),
    .icb_rsp_valid(ppi_rsp_valid),
    .icb_rsp_ready(bus_rsp_ready),
    .icb_rsp_rdata(ppi_rsp_rdata),
    .icb_rsp_err(ppi_rsp_err),
    .apb0_psel(gpio_psel),
    .apb0_penable(gpio_penable),
    .apb0_paddr(gpio_paddr),
    .apb0_pwdata(gpio_pwdata),
    .apb0_pwrite(gpio_pwrite),
    .apb0_prdata(gpio_prdata_w),
    .apb0_pready(gpio_pready_w),
    .apb1_psel(uart_psel),
    .apb1_penable(uart_penable),
    .apb1_paddr(uart_paddr),
    .apb1_pwdata(uart_pwdata),
    .apb1_pwrite(uart_pwrite),
    .apb1_prdata(uart_prdata_w),
    .apb1_pready(uart_pready_w),
    .apb2_psel(spi_psel),
    .apb2_penable(spi_penable),
    .apb2_paddr(spi_paddr),
    .apb2_pwdata(spi_pwdata),
    .apb2_pwrite(spi_pwrite),
    .apb2_prdata(spi_prdata_w),
    .apb2_pready(spi_pready_w),
    .apb3_psel(apb3_psel_w),
    .apb3_penable(apb3_penable_w),
    .apb3_paddr(apb3_paddr_w),
    .apb3_pwdata(apb3_pwdata_w),
    .apb3_pwrite(apb3_pwrite_w),
    .apb3_prdata(0),
    .apb3_pready(1'b1)
  );
  // ══════════════════════════════════════════════════════════════
  // GPIO Peripheral
  // ══════════════════════════════════════════════════════════════
  logic [32-1:0] gpio_prdata_w;
  logic gpio_pready_w;
  logic [32-1:0] gpio_out_w;
  logic [32-1:0] gpio_oe_w;
  logic gpio_irq_w;
  Gpio gpio_p (
    .clk(clk),
    .rst_n(rst_n),
    .psel(gpio_psel),
    .penable(gpio_penable),
    .paddr(gpio_paddr),
    .pwdata(gpio_pwdata),
    .pwrite(gpio_pwrite),
    .prdata(gpio_prdata_w),
    .pready(gpio_pready_w),
    .gpio_in(gpio_in),
    .gpio_out(gpio_out_w),
    .gpio_oe(gpio_oe_w),
    .gpio_irq(gpio_irq_w)
  );
  // ══════════════════════════════════════════════════════════════
  // UART Peripheral
  // ══════════════════════════════════════════════════════════════
  logic [32-1:0] uart_prdata_w;
  logic uart_pready_w;
  logic uart_tx_w;
  logic uart_irq_w;
  Uart uart_p (
    .clk(clk),
    .rst_n(rst_n),
    .psel(uart_psel),
    .penable(uart_penable),
    .paddr(uart_paddr),
    .pwdata(uart_pwdata),
    .pwrite(uart_pwrite),
    .prdata(uart_prdata_w),
    .pready(uart_pready_w),
    .uart_tx(uart_tx_w),
    .uart_rx(uart_rx),
    .uart_irq(uart_irq_w)
  );
  // ══════════════════════════════════════════════════════════════
  // SPI Peripheral
  // ══════════════════════════════════════════════════════════════
  logic [32-1:0] spi_prdata_w;
  logic spi_pready_w;
  logic spi_sclk_w;
  logic spi_mosi_w;
  logic spi_cs_n_w;
  logic spi_irq_w;
  Spi spi_p (
    .clk(clk),
    .rst_n(rst_n),
    .psel(spi_psel),
    .penable(spi_penable),
    .paddr(spi_paddr),
    .pwdata(spi_pwdata),
    .pwrite(spi_pwrite),
    .prdata(spi_prdata_w),
    .pready(spi_pready_w),
    .spi_sclk(spi_sclk_w),
    .spi_mosi(spi_mosi_w),
    .spi_miso(spi_miso),
    .spi_cs_n(spi_cs_n_w),
    .spi_irq(spi_irq_w)
  );
  // ══════════════════════════════════════════════════════════════
  // Interrupt Controller
  // ══════════════════════════════════════════════════════════════
  logic irq_req_w;
  logic [32-1:0] irq_cause_w;
  logic irq_mip_meip;
  logic irq_mip_mtip;
  logic irq_mip_msip;
  IrqCtrl irq (
    .clk(clk),
    .rst_n(rst_n),
    .ext_irq_i(gpio_irq_w),
    .sw_irq_i(1'b0),
    .tmr_irq_i(tmr_irq_w),
    .mstatus_mie(1'b1),
    .mie_meie(1'b1),
    .mie_mtie(1'b1),
    .mie_msie(1'b1),
    .pipe_flush_ack(1'b0),
    .commit_valid(core_commit_w),
    .irq_req(irq_req_w),
    .irq_cause(irq_cause_w),
    .mip_meip(irq_mip_meip),
    .mip_mtip(irq_mip_mtip),
    .mip_msip(irq_mip_msip)
  );
  // ══════════════════════════════════════════════════════════════
  // Debug Module
  // ══════════════════════════════════════════════════════════════
  logic [32-1:0] dbg_prdata_w;
  logic dbg_pready_w;
  logic dbg_halt_req;
  logic dbg_resume_req;
  logic [16-1:0] dbg_reg_addr_w;
  logic [32-1:0] dbg_reg_wdata_w;
  logic dbg_reg_wen_w;
  DebugModule dbg (
    .clk(clk),
    .rst_n(rst_n),
    .psel(dbg_psel),
    .penable(dbg_penable),
    .paddr(dbg_paddr),
    .pwdata(dbg_pwdata),
    .pwrite(dbg_pwrite),
    .prdata(dbg_prdata_w),
    .pready(dbg_pready_w),
    .hart_halted(1'b0),
    .hart_running(1'b1),
    .halt_req(dbg_halt_req),
    .resume_req(dbg_resume_req),
    .dbg_reg_addr(dbg_reg_addr_w),
    .dbg_reg_wdata(dbg_reg_wdata_w),
    .dbg_reg_wen(dbg_reg_wen_w),
    .dbg_reg_rdata(0)
  );
  // ══════════════════════════════════════════════════════════════
  // Bus response mux
  // ══════════════════════════════════════════════════════════════
  always_comb begin
    if (sel_sram) begin
      bus_cmd_ready = sram_ready;
      bus_rsp_valid = sram_rsp_valid;
      bus_rsp_rdata = sram_rsp_rdata;
      bus_rsp_err = sram_rsp_err;
    end else if (sel_ppi) begin
      bus_cmd_ready = ppi_ready;
      bus_rsp_valid = ppi_rsp_valid;
      bus_rsp_rdata = ppi_rsp_rdata;
      bus_rsp_err = ppi_rsp_err;
    end else if (sel_fio) begin
      bus_cmd_ready = fio_ready;
      bus_rsp_valid = fio_rsp_valid;
      bus_rsp_rdata = fio_rsp_rdata;
      bus_rsp_err = fio_rsp_err;
    end else begin
      bus_cmd_ready = 1'b1;
      bus_rsp_valid = 1'b0;
      bus_rsp_rdata = 0;
      bus_rsp_err = 1'b1;
    end
    ext_cmd_ready = arbt_m0_ready;
    ext_rsp_valid = arbt_m0_rsp_valid;
    ext_rsp_rdata = arbt_m0_rsp_rdata;
    ext_rsp_err = arbt_m0_rsp_err;
    core_commit = core_commit_w;
    core_instr = core_instr_w;
    core_pc = core_pc_w;
    core_valid = core_valid_w;
    gpio_out = gpio_out_w;
    gpio_oe = gpio_oe_w;
    gpio_irq = gpio_irq_w;
    uart_tx = uart_tx_w;
    uart_irq = uart_irq_w;
    spi_sclk = spi_sclk_w;
    spi_mosi = spi_mosi_w;
    spi_cs_n = spi_cs_n_w;
    spi_irq = spi_irq_w;
    fio_out_0 = fio_out_0_w;
    fio_out_1 = fio_out_1_w;
    dbg_prdata = dbg_prdata_w;
  end

endmodule

