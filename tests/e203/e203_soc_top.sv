// domain SysDomain
//   freq_mhz: 100

// domain SysDomain
//   freq_mhz: 100

// domain SysDomain
//   freq_mhz: 100

// domain SysDomain
//   freq_mhz: 100

// domain SysDomain
//   freq_mhz: 100

// domain SysDomain
//   freq_mhz: 100

// domain SysDomain
//   freq_mhz: 100

module e203_soc_top (
  input logic clk,
  input logic rst_n,
  input logic timer_rst,
  input logic [31:0] pc_rtvec,
  input logic itcm_wr_en,
  input logic [13:0] itcm_wr_addr,
  input logic [31:0] itcm_wr_data,
  input logic ext_cmd_valid,
  input logic [31:0] ext_cmd_addr,
  input logic [31:0] ext_cmd_wdata,
  input logic [3:0] ext_cmd_wmask,
  input logic ext_cmd_read,
  output logic ext_cmd_ready,
  output logic ext_rsp_valid,
  output logic [31:0] ext_rsp_rdata,
  output logic ext_rsp_err,
  input logic [31:0] gpio_in,
  output logic [31:0] gpio_out,
  output logic [31:0] gpio_oe,
  output logic uart_tx,
  input logic uart_rx,
  output logic spi_sclk,
  output logic spi_mosi,
  input logic spi_miso,
  output logic spi_cs_n,
  input logic [31:0] fio_in_0,
  input logic [31:0] fio_in_1,
  output logic [31:0] fio_out_0,
  output logic [31:0] fio_out_1,
  input logic dbg_psel,
  input logic dbg_penable,
  input logic [31:0] dbg_paddr,
  input logic [31:0] dbg_pwdata,
  input logic dbg_pwrite,
  output logic [31:0] dbg_prdata,
  output logic [31:0] inspect_pc,
  output logic core_wfi,
  output logic gpio_irq,
  output logic uart_irq,
  output logic spi_irq
);

  logic [15:0] ext2itcm_cmd_addr;
  logic [3:0] clint_reg_addr;
  logic clint_reg_wen;
  logic ifu2itcm_icb_cmd_valid;
  logic ifu2itcm_icb_cmd_ready;
  logic [15:0] ifu2itcm_icb_cmd_addr;
  logic ifu2itcm_icb_rsp_valid;
  logic ifu2itcm_icb_rsp_ready;
  logic ifu2itcm_icb_rsp_err;
  logic [63:0] ifu2itcm_icb_rsp_rdata;
  logic ifu2itcm_holdup;
  logic lsu2itcm_icb_cmd_valid;
  logic lsu2itcm_icb_cmd_ready;
  logic [15:0] lsu2itcm_icb_cmd_addr;
  logic lsu2itcm_icb_cmd_read;
  logic [31:0] lsu2itcm_icb_cmd_wdata;
  logic [3:0] lsu2itcm_icb_cmd_wmask;
  logic lsu2itcm_icb_rsp_valid;
  logic lsu2itcm_icb_rsp_ready;
  logic lsu2itcm_icb_rsp_err;
  logic [31:0] lsu2itcm_icb_rsp_rdata;
  logic lsu2dtcm_icb_cmd_valid;
  logic lsu2dtcm_icb_cmd_ready;
  logic [15:0] lsu2dtcm_icb_cmd_addr;
  logic lsu2dtcm_icb_cmd_read;
  logic [31:0] lsu2dtcm_icb_cmd_wdata;
  logic [3:0] lsu2dtcm_icb_cmd_wmask;
  logic lsu2dtcm_icb_rsp_valid;
  logic lsu2dtcm_icb_rsp_ready;
  logic lsu2dtcm_icb_rsp_err;
  logic [31:0] lsu2dtcm_icb_rsp_rdata;
  logic itcm_ram_cs;
  logic itcm_ram_we;
  logic [12:0] itcm_ram_addr;
  logic [7:0] itcm_ram_wem;
  logic [63:0] itcm_ram_din;
  logic [63:0] itcm_ram_dout;
  logic clk_itcm_ram_nc;
  logic dtcm_ram_cs;
  logic dtcm_ram_we;
  logic [13:0] dtcm_ram_addr;
  logic [3:0] dtcm_ram_wem;
  logic [31:0] dtcm_ram_din;
  logic [31:0] dtcm_ram_dout;
  logic clk_dtcm_ram_nc;
  logic itcm_active_nc;
  logic dtcm_active_nc;
  logic ppi_icb_cmd_valid;
  logic ppi_icb_cmd_ready;
  logic [31:0] ppi_icb_cmd_addr;
  logic ppi_icb_cmd_read;
  logic [31:0] ppi_icb_cmd_wdata;
  logic [3:0] ppi_icb_cmd_wmask;
  logic ppi_icb_rsp_valid;
  logic ppi_icb_rsp_ready;
  logic ppi_icb_rsp_err;
  logic [31:0] ppi_icb_rsp_rdata;
  logic clint_icb_cmd_valid;
  logic [31:0] clint_icb_cmd_addr;
  logic clint_icb_cmd_read;
  logic [31:0] clint_icb_cmd_wdata;
  logic clint_icb_rsp_ready;
  logic fio_icb_cmd_valid;
  logic fio_icb_cmd_ready;
  logic [31:0] fio_icb_cmd_addr;
  logic fio_icb_cmd_read;
  logic [31:0] fio_icb_cmd_wdata;
  logic [3:0] fio_icb_cmd_wmask;
  logic fio_icb_rsp_valid;
  logic fio_icb_rsp_ready;
  logic fio_icb_rsp_err;
  logic [31:0] fio_icb_rsp_rdata;
  logic mem_icb_cmd_valid;
  logic mem_icb_cmd_ready;
  logic [31:0] mem_icb_cmd_addr;
  logic mem_icb_cmd_read;
  logic [31:0] mem_icb_cmd_wdata;
  logic [3:0] mem_icb_cmd_wmask;
  logic mem_icb_rsp_valid;
  logic mem_icb_rsp_ready;
  logic mem_icb_rsp_err;
  logic [31:0] mem_icb_rsp_rdata;
  logic tm_stop_nc;
  logic core_cgstop_nc;
  logic tcm_cgstop;
  logic exu_active_nc;
  logic ifu_active_nc;
  logic lsu_active_nc;
  logic biu_active_nc;
  logic wr_dcsr_ena_nc;
  logic wr_dpc_ena_nc;
  logic wr_dscratch_ena_nc;
  logic [31:0] wr_csr_nxt_nc;
  logic [31:0] cmt_dpc_nc;
  logic cmt_dpc_ena_nc;
  logic [2:0] cmt_dcause_nc;
  logic cmt_dcause_ena_nc;
  logic lsu2itcm_icb_cmd_lock_nc;
  logic lsu2itcm_icb_cmd_excl_nc;
  logic [1:0] lsu2itcm_icb_cmd_size_nc;
  logic lsu2dtcm_icb_cmd_lock_nc;
  logic lsu2dtcm_icb_cmd_excl_nc;
  logic [1:0] lsu2dtcm_icb_cmd_size_nc;
  logic ppi_icb_cmd_lock_nc;
  logic ppi_icb_cmd_excl_nc;
  logic [1:0] ppi_icb_cmd_size_nc;
  logic [1:0] ppi_icb_cmd_burst_nc;
  logic [1:0] ppi_icb_cmd_beat_nc;
  logic [3:0] clint_icb_cmd_wmask_nc;
  logic clint_icb_cmd_lock_nc;
  logic clint_icb_cmd_excl_nc;
  logic [1:0] clint_icb_cmd_size_nc;
  logic [1:0] clint_icb_cmd_burst_nc;
  logic [1:0] clint_icb_cmd_beat_nc;
  logic plic_icb_cmd_valid_nc;
  logic [31:0] plic_icb_cmd_addr_nc;
  logic plic_icb_cmd_read_nc;
  logic [31:0] plic_icb_cmd_wdata_nc;
  logic [3:0] plic_icb_cmd_wmask_nc;
  logic plic_icb_cmd_lock_nc;
  logic plic_icb_cmd_excl_nc;
  logic [1:0] plic_icb_cmd_size_nc;
  logic [1:0] plic_icb_cmd_burst_nc;
  logic [1:0] plic_icb_cmd_beat_nc;
  logic plic_icb_rsp_ready_nc;
  logic fio_icb_cmd_lock_nc;
  logic fio_icb_cmd_excl_nc;
  logic [1:0] fio_icb_cmd_size_nc;
  logic [1:0] fio_icb_cmd_burst_nc;
  logic [1:0] fio_icb_cmd_beat_nc;
  logic mem_icb_cmd_lock_nc;
  logic mem_icb_cmd_excl_nc;
  logic [1:0] mem_icb_cmd_size_nc;
  logic [1:0] mem_icb_cmd_burst_nc;
  logic [1:0] mem_icb_cmd_beat_nc;
  logic nice_req_valid_nc;
  logic [31:0] nice_req_inst_nc;
  logic [31:0] nice_req_rs1_nc;
  logic [31:0] nice_req_rs2_nc;
  logic nice_rsp_multicyc_ready_nc;
  logic nice_icb_cmd_ready_nc;
  logic nice_icb_rsp_valid_nc;
  logic [31:0] nice_icb_rsp_rdata_nc;
  logic nice_icb_rsp_err_nc;
  logic tmr_irq_w;
  logic gpio_irq_w;
  logic uart_irq_w;
  logic spi_irq_w;
  logic irq_req_w;
  logic [31:0] irq_cause_w;
  logic irq_mip_meip;
  logic irq_mip_mtip;
  logic irq_mip_msip;
  logic dbg_halt_req;
  logic dbg_resume_req;
  logic [31:0] dbg_prdata_w;
  logic dbg_pready_w;
  logic [15:0] dbg_reg_addr_w;
  logic [31:0] dbg_reg_wdata_w;
  logic dbg_reg_wen_w;
  logic gpio_psel;
  logic gpio_penable;
  logic [31:0] gpio_paddr;
  logic [31:0] gpio_pwdata;
  logic gpio_pwrite;
  logic [31:0] gpio_prdata_w;
  logic gpio_pready_w;
  logic uart_psel;
  logic uart_penable;
  logic [31:0] uart_paddr;
  logic [31:0] uart_pwdata;
  logic uart_pwrite;
  logic [31:0] uart_prdata_w;
  logic uart_pready_w;
  logic spi_psel;
  logic spi_penable;
  logic [31:0] spi_paddr;
  logic [31:0] spi_pwdata;
  logic spi_pwrite;
  logic [31:0] spi_prdata_w;
  logic spi_pready_w;
  logic apb3_psel_w;
  logic apb3_penable_w;
  logic [31:0] apb3_paddr_w;
  logic [31:0] apb3_pwdata_w;
  logic apb3_pwrite_w;
  logic arbt_s_cmd_valid;
  logic [31:0] arbt_s_cmd_addr;
  logic [31:0] arbt_s_cmd_wdata;
  logic [3:0] arbt_s_cmd_wmask;
  logic arbt_s_cmd_read;
  logic arbt_s_rsp_ready;
  logic sram_ready;
  logic sram_rsp_valid;
  logic [31:0] sram_rsp_rdata;
  logic sram_rsp_err;
  logic [31:0] gpio_out_w;
  logic [31:0] gpio_oe_w;
  logic uart_tx_w;
  logic spi_sclk_w;
  logic spi_mosi_w;
  logic spi_cs_n_w;
  logic [31:0] fio_out_0_w;
  logic [31:0] fio_out_1_w;
  logic [31:0] fio_out_2_w;
  logic [31:0] fio_out_3_w;
  assign ext2itcm_cmd_addr = 16'($unsigned(itcm_wr_addr)) << 2;
  assign clint_reg_addr = clint_icb_cmd_addr[5:2];
  assign clint_reg_wen = clint_icb_cmd_valid & !clint_icb_cmd_read;
  logic [31:0] clint_reg_rdata;
  logic clint_rsp_valid_r;
  logic [31:0] clint_rsp_rdata_r;
  logic ext2itcm_cmd_ready_nc;
  logic ext2itcm_rsp_valid_nc;
  logic ext2itcm_rsp_err_nc;
  logic [31:0] ext2itcm_rsp_rdata_nc;
  logic ext2dtcm_cmd_ready_nc;
  logic ext2dtcm_rsp_valid_nc;
  logic ext2dtcm_rsp_err_nc;
  logic [31:0] ext2dtcm_rsp_rdata_nc;
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      clint_rsp_rdata_r <= 0;
      clint_rsp_valid_r <= 1'b0;
    end else begin
      clint_rsp_valid_r <= clint_icb_cmd_valid;
      clint_rsp_rdata_r <= clint_reg_rdata;
    end
  end
  e203_core_top core (
    .clk(clk),
    .rst_n(rst_n),
    .test_mode(1'b0),
    .inspect_pc(inspect_pc),
    .core_wfi(core_wfi),
    .tm_stop(tm_stop_nc),
    .core_cgstop(core_cgstop_nc),
    .tcm_cgstop(tcm_cgstop),
    .exu_active(exu_active_nc),
    .ifu_active(ifu_active_nc),
    .lsu_active(lsu_active_nc),
    .biu_active(biu_active_nc),
    .pc_rtvec(pc_rtvec),
    .core_mhartid(0),
    .dbg_irq_r(dbg_halt_req),
    .lcl_irq_r(1'b0),
    .evt_r(1'b0),
    .ext_irq_r(irq_mip_meip),
    .sft_irq_r(irq_mip_msip),
    .tmr_irq_r(irq_mip_mtip),
    .wr_dcsr_ena(wr_dcsr_ena_nc),
    .wr_dpc_ena(wr_dpc_ena_nc),
    .wr_dscratch_ena(wr_dscratch_ena_nc),
    .wr_csr_nxt(wr_csr_nxt_nc),
    .dcsr_r(0),
    .dpc_r(0),
    .dscratch_r(0),
    .cmt_dpc(cmt_dpc_nc),
    .cmt_dpc_ena(cmt_dpc_ena_nc),
    .cmt_dcause(cmt_dcause_nc),
    .cmt_dcause_ena(cmt_dcause_ena_nc),
    .dbg_mode(1'b0),
    .dbg_halt_r(dbg_halt_req),
    .dbg_step_r(1'b0),
    .dbg_ebreakm_r(1'b0),
    .dbg_stopcycle(1'b0),
    .itcm_region_indic(0),
    .ifu2itcm_holdup(ifu2itcm_holdup),
    .ifu2itcm_icb_cmd_valid(ifu2itcm_icb_cmd_valid),
    .ifu2itcm_icb_cmd_ready(ifu2itcm_icb_cmd_ready),
    .ifu2itcm_icb_cmd_addr(ifu2itcm_icb_cmd_addr),
    .ifu2itcm_icb_rsp_valid(ifu2itcm_icb_rsp_valid),
    .ifu2itcm_icb_rsp_ready(ifu2itcm_icb_rsp_ready),
    .ifu2itcm_icb_rsp_err(ifu2itcm_icb_rsp_err),
    .ifu2itcm_icb_rsp_rdata(ifu2itcm_icb_rsp_rdata),
    .lsu2itcm_icb_cmd_valid(lsu2itcm_icb_cmd_valid),
    .lsu2itcm_icb_cmd_ready(lsu2itcm_icb_cmd_ready),
    .lsu2itcm_icb_cmd_addr(lsu2itcm_icb_cmd_addr),
    .lsu2itcm_icb_cmd_read(lsu2itcm_icb_cmd_read),
    .lsu2itcm_icb_cmd_wdata(lsu2itcm_icb_cmd_wdata),
    .lsu2itcm_icb_cmd_wmask(lsu2itcm_icb_cmd_wmask),
    .lsu2itcm_icb_cmd_lock(lsu2itcm_icb_cmd_lock_nc),
    .lsu2itcm_icb_cmd_excl(lsu2itcm_icb_cmd_excl_nc),
    .lsu2itcm_icb_cmd_size(lsu2itcm_icb_cmd_size_nc),
    .lsu2itcm_icb_rsp_valid(lsu2itcm_icb_rsp_valid),
    .lsu2itcm_icb_rsp_ready(lsu2itcm_icb_rsp_ready),
    .lsu2itcm_icb_rsp_err(lsu2itcm_icb_rsp_err),
    .lsu2itcm_icb_rsp_excl_ok(1'b0),
    .lsu2itcm_icb_rsp_rdata(lsu2itcm_icb_rsp_rdata),
    .dtcm_region_indic('h90000000),
    .lsu2dtcm_icb_cmd_valid(lsu2dtcm_icb_cmd_valid),
    .lsu2dtcm_icb_cmd_ready(lsu2dtcm_icb_cmd_ready),
    .lsu2dtcm_icb_cmd_addr(lsu2dtcm_icb_cmd_addr),
    .lsu2dtcm_icb_cmd_read(lsu2dtcm_icb_cmd_read),
    .lsu2dtcm_icb_cmd_wdata(lsu2dtcm_icb_cmd_wdata),
    .lsu2dtcm_icb_cmd_wmask(lsu2dtcm_icb_cmd_wmask),
    .lsu2dtcm_icb_cmd_lock(lsu2dtcm_icb_cmd_lock_nc),
    .lsu2dtcm_icb_cmd_excl(lsu2dtcm_icb_cmd_excl_nc),
    .lsu2dtcm_icb_cmd_size(lsu2dtcm_icb_cmd_size_nc),
    .lsu2dtcm_icb_rsp_valid(lsu2dtcm_icb_rsp_valid),
    .lsu2dtcm_icb_rsp_ready(lsu2dtcm_icb_rsp_ready),
    .lsu2dtcm_icb_rsp_err(lsu2dtcm_icb_rsp_err),
    .lsu2dtcm_icb_rsp_excl_ok(1'b0),
    .lsu2dtcm_icb_rsp_rdata(lsu2dtcm_icb_rsp_rdata),
    .ppi_region_indic('h10000000),
    .ppi_icb_enable(1'b1),
    .ppi_icb_cmd_valid(ppi_icb_cmd_valid),
    .ppi_icb_cmd_ready(ppi_icb_cmd_ready),
    .ppi_icb_cmd_addr(ppi_icb_cmd_addr),
    .ppi_icb_cmd_read(ppi_icb_cmd_read),
    .ppi_icb_cmd_wdata(ppi_icb_cmd_wdata),
    .ppi_icb_cmd_wmask(ppi_icb_cmd_wmask),
    .ppi_icb_cmd_lock(ppi_icb_cmd_lock_nc),
    .ppi_icb_cmd_excl(ppi_icb_cmd_excl_nc),
    .ppi_icb_cmd_size(ppi_icb_cmd_size_nc),
    .ppi_icb_cmd_burst(ppi_icb_cmd_burst_nc),
    .ppi_icb_cmd_beat(ppi_icb_cmd_beat_nc),
    .ppi_icb_rsp_valid(ppi_icb_rsp_valid),
    .ppi_icb_rsp_ready(ppi_icb_rsp_ready),
    .ppi_icb_rsp_err(ppi_icb_rsp_err),
    .ppi_icb_rsp_excl_ok(1'b0),
    .ppi_icb_rsp_rdata(ppi_icb_rsp_rdata),
    .clint_region_indic('h2000000),
    .clint_icb_enable(1'b1),
    .clint_icb_cmd_valid(clint_icb_cmd_valid),
    .clint_icb_cmd_ready(1'b1),
    .clint_icb_cmd_addr(clint_icb_cmd_addr),
    .clint_icb_cmd_read(clint_icb_cmd_read),
    .clint_icb_cmd_wdata(clint_icb_cmd_wdata),
    .clint_icb_cmd_wmask(clint_icb_cmd_wmask_nc),
    .clint_icb_cmd_lock(clint_icb_cmd_lock_nc),
    .clint_icb_cmd_excl(clint_icb_cmd_excl_nc),
    .clint_icb_cmd_size(clint_icb_cmd_size_nc),
    .clint_icb_cmd_burst(clint_icb_cmd_burst_nc),
    .clint_icb_cmd_beat(clint_icb_cmd_beat_nc),
    .clint_icb_rsp_valid(clint_rsp_valid_r),
    .clint_icb_rsp_ready(clint_icb_rsp_ready),
    .clint_icb_rsp_err(1'b0),
    .clint_icb_rsp_excl_ok(1'b0),
    .clint_icb_rsp_rdata(clint_rsp_rdata_r),
    .plic_region_indic('hC000000),
    .plic_icb_enable(1'b0),
    .plic_icb_cmd_valid(plic_icb_cmd_valid_nc),
    .plic_icb_cmd_ready(1'b1),
    .plic_icb_cmd_addr(plic_icb_cmd_addr_nc),
    .plic_icb_cmd_read(plic_icb_cmd_read_nc),
    .plic_icb_cmd_wdata(plic_icb_cmd_wdata_nc),
    .plic_icb_cmd_wmask(plic_icb_cmd_wmask_nc),
    .plic_icb_cmd_lock(plic_icb_cmd_lock_nc),
    .plic_icb_cmd_excl(plic_icb_cmd_excl_nc),
    .plic_icb_cmd_size(plic_icb_cmd_size_nc),
    .plic_icb_cmd_burst(plic_icb_cmd_burst_nc),
    .plic_icb_cmd_beat(plic_icb_cmd_beat_nc),
    .plic_icb_rsp_valid(1'b0),
    .plic_icb_rsp_ready(plic_icb_rsp_ready_nc),
    .plic_icb_rsp_err(1'b0),
    .plic_icb_rsp_excl_ok(1'b0),
    .plic_icb_rsp_rdata(0),
    .fio_region_indic('h3000000),
    .fio_icb_enable(1'b1),
    .fio_icb_cmd_valid(fio_icb_cmd_valid),
    .fio_icb_cmd_ready(fio_icb_cmd_ready),
    .fio_icb_cmd_addr(fio_icb_cmd_addr),
    .fio_icb_cmd_read(fio_icb_cmd_read),
    .fio_icb_cmd_wdata(fio_icb_cmd_wdata),
    .fio_icb_cmd_wmask(fio_icb_cmd_wmask),
    .fio_icb_cmd_lock(fio_icb_cmd_lock_nc),
    .fio_icb_cmd_excl(fio_icb_cmd_excl_nc),
    .fio_icb_cmd_size(fio_icb_cmd_size_nc),
    .fio_icb_cmd_burst(fio_icb_cmd_burst_nc),
    .fio_icb_cmd_beat(fio_icb_cmd_beat_nc),
    .fio_icb_rsp_valid(fio_icb_rsp_valid),
    .fio_icb_rsp_ready(fio_icb_rsp_ready),
    .fio_icb_rsp_err(fio_icb_rsp_err),
    .fio_icb_rsp_excl_ok(1'b0),
    .fio_icb_rsp_rdata(fio_icb_rsp_rdata),
    .mem_icb_enable(1'b1),
    .mem_icb_cmd_valid(mem_icb_cmd_valid),
    .mem_icb_cmd_ready(mem_icb_cmd_ready),
    .mem_icb_cmd_addr(mem_icb_cmd_addr),
    .mem_icb_cmd_read(mem_icb_cmd_read),
    .mem_icb_cmd_wdata(mem_icb_cmd_wdata),
    .mem_icb_cmd_wmask(mem_icb_cmd_wmask),
    .mem_icb_cmd_lock(mem_icb_cmd_lock_nc),
    .mem_icb_cmd_excl(mem_icb_cmd_excl_nc),
    .mem_icb_cmd_size(mem_icb_cmd_size_nc),
    .mem_icb_cmd_burst(mem_icb_cmd_burst_nc),
    .mem_icb_cmd_beat(mem_icb_cmd_beat_nc),
    .mem_icb_rsp_valid(mem_icb_rsp_valid),
    .mem_icb_rsp_ready(mem_icb_rsp_ready),
    .mem_icb_rsp_err(mem_icb_rsp_err),
    .mem_icb_rsp_excl_ok(1'b0),
    .mem_icb_rsp_rdata(mem_icb_rsp_rdata),
    .nice_mem_holdup(1'b0),
    .nice_req_valid(nice_req_valid_nc),
    .nice_req_ready(1'b0),
    .nice_req_inst(nice_req_inst_nc),
    .nice_req_rs1(nice_req_rs1_nc),
    .nice_req_rs2(nice_req_rs2_nc),
    .nice_rsp_multicyc_valid(1'b0),
    .nice_rsp_multicyc_ready(nice_rsp_multicyc_ready_nc),
    .nice_rsp_multicyc_dat(0),
    .nice_rsp_multicyc_err(1'b0),
    .nice_icb_cmd_valid(1'b0),
    .nice_icb_cmd_ready(nice_icb_cmd_ready_nc),
    .nice_icb_cmd_addr(0),
    .nice_icb_cmd_read(1'b1),
    .nice_icb_cmd_wdata(0),
    .nice_icb_cmd_size(0),
    .nice_icb_rsp_valid(nice_icb_rsp_valid_nc),
    .nice_icb_rsp_ready(1'b1),
    .nice_icb_rsp_rdata(nice_icb_rsp_rdata_nc),
    .nice_icb_rsp_err(nice_icb_rsp_err_nc)
  );
  e203_itcm_ctrl itcm_ctrl (
    .clk(clk),
    .rst_n(rst_n),
    .test_mode(1'b0),
    .tcm_cgstop(tcm_cgstop),
    .itcm_active(itcm_active_nc),
    .ifu2itcm_icb_cmd_valid(ifu2itcm_icb_cmd_valid),
    .ifu2itcm_icb_cmd_ready(ifu2itcm_icb_cmd_ready),
    .ifu2itcm_icb_cmd_addr(ifu2itcm_icb_cmd_addr),
    .ifu2itcm_icb_cmd_read(1'b1),
    .ifu2itcm_icb_cmd_wdata(0),
    .ifu2itcm_icb_cmd_wmask(0),
    .ifu2itcm_icb_rsp_valid(ifu2itcm_icb_rsp_valid),
    .ifu2itcm_icb_rsp_ready(ifu2itcm_icb_rsp_ready),
    .ifu2itcm_icb_rsp_err(ifu2itcm_icb_rsp_err),
    .ifu2itcm_icb_rsp_rdata(ifu2itcm_icb_rsp_rdata),
    .ifu2itcm_holdup(ifu2itcm_holdup),
    .lsu2itcm_icb_cmd_valid(lsu2itcm_icb_cmd_valid),
    .lsu2itcm_icb_cmd_ready(lsu2itcm_icb_cmd_ready),
    .lsu2itcm_icb_cmd_addr(lsu2itcm_icb_cmd_addr),
    .lsu2itcm_icb_cmd_read(lsu2itcm_icb_cmd_read),
    .lsu2itcm_icb_cmd_wdata(lsu2itcm_icb_cmd_wdata),
    .lsu2itcm_icb_cmd_wmask(lsu2itcm_icb_cmd_wmask),
    .lsu2itcm_icb_rsp_valid(lsu2itcm_icb_rsp_valid),
    .lsu2itcm_icb_rsp_ready(lsu2itcm_icb_rsp_ready),
    .lsu2itcm_icb_rsp_err(lsu2itcm_icb_rsp_err),
    .lsu2itcm_icb_rsp_rdata(lsu2itcm_icb_rsp_rdata),
    .ext2itcm_icb_cmd_valid(itcm_wr_en),
    .ext2itcm_icb_cmd_ready(ext2itcm_cmd_ready_nc),
    .ext2itcm_icb_cmd_addr(ext2itcm_cmd_addr),
    .ext2itcm_icb_cmd_read(1'b0),
    .ext2itcm_icb_cmd_wdata(itcm_wr_data),
    .ext2itcm_icb_cmd_wmask(15),
    .ext2itcm_icb_rsp_valid(ext2itcm_rsp_valid_nc),
    .ext2itcm_icb_rsp_ready(1'b1),
    .ext2itcm_icb_rsp_err(ext2itcm_rsp_err_nc),
    .ext2itcm_icb_rsp_rdata(ext2itcm_rsp_rdata_nc),
    .itcm_ram_cs(itcm_ram_cs),
    .itcm_ram_we(itcm_ram_we),
    .itcm_ram_addr(itcm_ram_addr),
    .itcm_ram_wem(itcm_ram_wem),
    .itcm_ram_din(itcm_ram_din),
    .itcm_ram_dout(itcm_ram_dout),
    .clk_itcm_ram(clk_itcm_ram_nc)
  );
  e203_itcm_ram itcm_ram (
    .clk(clk),
    .rst_n(rst_n),
    .sd(1'b0),
    .ds(1'b0),
    .ls(1'b0),
    .cs(itcm_ram_cs),
    .we(itcm_ram_we),
    .addr(itcm_ram_addr),
    .wem(itcm_ram_wem),
    .din(itcm_ram_din),
    .dout(itcm_ram_dout)
  );
  e203_dtcm_ctrl dtcm_ctrl (
    .clk(clk),
    .rst_n(rst_n),
    .test_mode(1'b0),
    .tcm_cgstop(tcm_cgstop),
    .dtcm_active(dtcm_active_nc),
    .lsu2dtcm_icb_cmd_valid(lsu2dtcm_icb_cmd_valid),
    .lsu2dtcm_icb_cmd_ready(lsu2dtcm_icb_cmd_ready),
    .lsu2dtcm_icb_cmd_addr(lsu2dtcm_icb_cmd_addr),
    .lsu2dtcm_icb_cmd_read(lsu2dtcm_icb_cmd_read),
    .lsu2dtcm_icb_cmd_wdata(lsu2dtcm_icb_cmd_wdata),
    .lsu2dtcm_icb_cmd_wmask(lsu2dtcm_icb_cmd_wmask),
    .lsu2dtcm_icb_rsp_valid(lsu2dtcm_icb_rsp_valid),
    .lsu2dtcm_icb_rsp_ready(lsu2dtcm_icb_rsp_ready),
    .lsu2dtcm_icb_rsp_err(lsu2dtcm_icb_rsp_err),
    .lsu2dtcm_icb_rsp_rdata(lsu2dtcm_icb_rsp_rdata),
    .ext2dtcm_icb_cmd_valid(1'b0),
    .ext2dtcm_icb_cmd_ready(ext2dtcm_cmd_ready_nc),
    .ext2dtcm_icb_cmd_addr(0),
    .ext2dtcm_icb_cmd_read(1'b1),
    .ext2dtcm_icb_cmd_wdata(0),
    .ext2dtcm_icb_cmd_wmask(0),
    .ext2dtcm_icb_rsp_valid(ext2dtcm_rsp_valid_nc),
    .ext2dtcm_icb_rsp_ready(1'b1),
    .ext2dtcm_icb_rsp_err(ext2dtcm_rsp_err_nc),
    .ext2dtcm_icb_rsp_rdata(ext2dtcm_rsp_rdata_nc),
    .dtcm_ram_cs(dtcm_ram_cs),
    .dtcm_ram_we(dtcm_ram_we),
    .dtcm_ram_addr(dtcm_ram_addr),
    .dtcm_ram_wem(dtcm_ram_wem),
    .dtcm_ram_din(dtcm_ram_din),
    .dtcm_ram_dout(dtcm_ram_dout),
    .clk_dtcm_ram(clk_dtcm_ram_nc)
  );
  e203_dtcm_ram dtcm_ram (
    .clk(clk),
    .rst_n(rst_n),
    .sd(1'b0),
    .ds(1'b0),
    .ls(1'b0),
    .cs(dtcm_ram_cs),
    .we(dtcm_ram_we),
    .addr(dtcm_ram_addr),
    .wem(dtcm_ram_wem),
    .din(dtcm_ram_din),
    .dout(dtcm_ram_dout)
  );
  e203_clint_timer timer (
    .clk(clk),
    .rst(timer_rst),
    .reg_addr(clint_reg_addr),
    .reg_wdata(clint_icb_cmd_wdata),
    .reg_wen(clint_reg_wen),
    .reg_rdata(clint_reg_rdata),
    .tmr_irq(tmr_irq_w)
  );
  e203_icb_arbt arbt (
    .clk(clk),
    .rst_n(rst_n),
    .m0_cmd_valid(mem_icb_cmd_valid),
    .m0_cmd_addr(mem_icb_cmd_addr),
    .m0_cmd_wdata(mem_icb_cmd_wdata),
    .m0_cmd_wmask(mem_icb_cmd_wmask),
    .m0_cmd_read(mem_icb_cmd_read),
    .m0_cmd_ready(mem_icb_cmd_ready),
    .m0_rsp_valid(mem_icb_rsp_valid),
    .m0_rsp_ready(mem_icb_rsp_ready),
    .m0_rsp_rdata(mem_icb_rsp_rdata),
    .m0_rsp_err(mem_icb_rsp_err),
    .m1_cmd_valid(ext_cmd_valid),
    .m1_cmd_addr(ext_cmd_addr),
    .m1_cmd_wdata(ext_cmd_wdata),
    .m1_cmd_wmask(ext_cmd_wmask),
    .m1_cmd_read(ext_cmd_read),
    .m1_cmd_ready(ext_cmd_ready),
    .m1_rsp_valid(ext_rsp_valid),
    .m1_rsp_ready(1'b1),
    .m1_rsp_rdata(ext_rsp_rdata),
    .m1_rsp_err(ext_rsp_err),
    .s_cmd_valid(arbt_s_cmd_valid),
    .s_cmd_ready(sram_ready),
    .s_cmd_addr(arbt_s_cmd_addr),
    .s_cmd_wdata(arbt_s_cmd_wdata),
    .s_cmd_wmask(arbt_s_cmd_wmask),
    .s_cmd_read(arbt_s_cmd_read),
    .s_rsp_valid(sram_rsp_valid),
    .s_rsp_ready(arbt_s_rsp_ready),
    .s_rsp_rdata(sram_rsp_rdata),
    .s_rsp_err(sram_rsp_err)
  );
  e203_sram_ctrl sram (
    .clk(clk),
    .rst_n(rst_n),
    .icb_cmd_valid(arbt_s_cmd_valid),
    .icb_cmd_ready(sram_ready),
    .icb_cmd_addr(arbt_s_cmd_addr),
    .icb_cmd_wdata(arbt_s_cmd_wdata),
    .icb_cmd_wmask(arbt_s_cmd_wmask),
    .icb_cmd_read(arbt_s_cmd_read),
    .icb_rsp_valid(sram_rsp_valid),
    .icb_rsp_ready(arbt_s_rsp_ready),
    .icb_rsp_rdata(sram_rsp_rdata),
    .icb_rsp_err(sram_rsp_err)
  );
  e203_fio fio (
    .clk(clk),
    .rst_n(rst_n),
    .icb_cmd_valid(fio_icb_cmd_valid),
    .icb_cmd_ready(fio_icb_cmd_ready),
    .icb_cmd_addr(fio_icb_cmd_addr),
    .icb_cmd_wdata(fio_icb_cmd_wdata),
    .icb_cmd_wmask(fio_icb_cmd_wmask),
    .icb_cmd_read(fio_icb_cmd_read),
    .icb_rsp_valid(fio_icb_rsp_valid),
    .icb_rsp_ready(fio_icb_rsp_ready),
    .icb_rsp_rdata(fio_icb_rsp_rdata),
    .icb_rsp_err(fio_icb_rsp_err),
    .fio_in_0(fio_in_0),
    .fio_in_1(fio_in_1),
    .fio_out_0(fio_out_0_w),
    .fio_out_1(fio_out_1_w),
    .fio_out_2(fio_out_2_w),
    .fio_out_3(fio_out_3_w)
  );
  e203_ppi ppi (
    .clk(clk),
    .rst_n(rst_n),
    .icb_cmd_valid(ppi_icb_cmd_valid),
    .icb_cmd_ready(ppi_icb_cmd_ready),
    .icb_cmd_addr(ppi_icb_cmd_addr),
    .icb_cmd_wdata(ppi_icb_cmd_wdata),
    .icb_cmd_wmask(ppi_icb_cmd_wmask),
    .icb_cmd_read(ppi_icb_cmd_read),
    .icb_rsp_valid(ppi_icb_rsp_valid),
    .icb_rsp_ready(ppi_icb_rsp_ready),
    .icb_rsp_rdata(ppi_icb_rsp_rdata),
    .icb_rsp_err(ppi_icb_rsp_err),
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
  e203_gpio gpio_p (
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
  e203_uart uart_p (
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
  e203_spi spi_p (
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
  e203_irq_ctrl irq (
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
    .commit_valid(1'b0),
    .irq_req(irq_req_w),
    .irq_cause(irq_cause_w),
    .mip_meip(irq_mip_meip),
    .mip_mtip(irq_mip_mtip),
    .mip_msip(irq_mip_msip)
  );
  e203_debug_module dbg (
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
  assign gpio_out = gpio_out_w;
  assign gpio_oe = gpio_oe_w;
  assign gpio_irq = gpio_irq_w;
  assign uart_tx = uart_tx_w;
  assign uart_irq = uart_irq_w;
  assign spi_sclk = spi_sclk_w;
  assign spi_mosi = spi_mosi_w;
  assign spi_cs_n = spi_cs_n_w;
  assign spi_irq = spi_irq_w;
  assign fio_out_0 = fio_out_0_w;
  assign fio_out_1 = fio_out_1_w;
  assign dbg_prdata = dbg_prdata_w;

endmodule

