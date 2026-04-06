// E203 CPU Integration
// Integrates: reset_ctrl + irq_sync + core + itcm_ctrl + dtcm_ctrl
module e203_cpu #(
  parameter int MASTER = 1
) (
  input logic clk,
  input logic rst_n,
  input logic test_mode,
  input logic [32-1:0] pc_rtvec,
  input logic [1-1:0] core_mhartid,
  input logic dbg_irq_a,
  input logic ext_irq_a,
  input logic sft_irq_a,
  input logic tmr_irq_a,
  input logic dbg_mode,
  input logic dbg_halt_r,
  input logic dbg_step_r,
  input logic dbg_ebreakm_r,
  input logic dbg_stopcycle,
  output logic [32-1:0] cmt_dpc,
  output logic cmt_dpc_ena,
  output logic [3-1:0] cmt_dcause,
  output logic cmt_dcause_ena,
  output logic dbg_irq_r,
  output logic wr_dcsr_ena,
  output logic wr_dpc_ena,
  output logic wr_dscratch_ena,
  output logic [32-1:0] wr_csr_nxt,
  input logic [32-1:0] dcsr_r,
  input logic [32-1:0] dpc_r,
  input logic [32-1:0] dscratch_r,
  output logic itcm_ls,
  output logic itcm_ram_cs,
  output logic itcm_ram_we,
  output logic [13-1:0] itcm_ram_addr,
  output logic [8-1:0] itcm_ram_wem,
  output logic [64-1:0] itcm_ram_din,
  input logic [64-1:0] itcm_ram_dout,
  output logic clk_itcm_ram,
  output logic rst_itcm,
  output logic dtcm_ls,
  output logic dtcm_ram_cs,
  output logic dtcm_ram_we,
  output logic [14-1:0] dtcm_ram_addr,
  output logic [4-1:0] dtcm_ram_wem,
  output logic [32-1:0] dtcm_ram_din,
  input logic [32-1:0] dtcm_ram_dout,
  output logic clk_dtcm_ram,
  output logic rst_dtcm,
  input logic ext2itcm_icb_cmd_valid,
  output logic ext2itcm_icb_cmd_ready,
  input logic [16-1:0] ext2itcm_icb_cmd_addr,
  input logic ext2itcm_icb_cmd_read,
  input logic [32-1:0] ext2itcm_icb_cmd_wdata,
  input logic [4-1:0] ext2itcm_icb_cmd_wmask,
  output logic ext2itcm_icb_rsp_valid,
  input logic ext2itcm_icb_rsp_ready,
  output logic ext2itcm_icb_rsp_err,
  output logic [32-1:0] ext2itcm_icb_rsp_rdata,
  input logic ext2dtcm_icb_cmd_valid,
  output logic ext2dtcm_icb_cmd_ready,
  input logic [16-1:0] ext2dtcm_icb_cmd_addr,
  input logic ext2dtcm_icb_cmd_read,
  input logic [32-1:0] ext2dtcm_icb_cmd_wdata,
  input logic [4-1:0] ext2dtcm_icb_cmd_wmask,
  output logic ext2dtcm_icb_rsp_valid,
  input logic ext2dtcm_icb_rsp_ready,
  output logic ext2dtcm_icb_rsp_err,
  output logic [32-1:0] ext2dtcm_icb_rsp_rdata,
  input logic [32-1:0] ppi_region_indic,
  input logic ppi_icb_enable,
  output logic ppi_icb_cmd_valid,
  input logic ppi_icb_cmd_ready,
  output logic [32-1:0] ppi_icb_cmd_addr,
  output logic ppi_icb_cmd_read,
  output logic [32-1:0] ppi_icb_cmd_wdata,
  output logic [4-1:0] ppi_icb_cmd_wmask,
  output logic ppi_icb_cmd_lock,
  output logic ppi_icb_cmd_excl,
  output logic [2-1:0] ppi_icb_cmd_size,
  input logic ppi_icb_rsp_valid,
  output logic ppi_icb_rsp_ready,
  input logic ppi_icb_rsp_err,
  input logic ppi_icb_rsp_excl_ok,
  input logic [32-1:0] ppi_icb_rsp_rdata,
  input logic [32-1:0] clint_region_indic,
  input logic clint_icb_enable,
  output logic clint_icb_cmd_valid,
  input logic clint_icb_cmd_ready,
  output logic [32-1:0] clint_icb_cmd_addr,
  output logic clint_icb_cmd_read,
  output logic [32-1:0] clint_icb_cmd_wdata,
  output logic [4-1:0] clint_icb_cmd_wmask,
  output logic clint_icb_cmd_lock,
  output logic clint_icb_cmd_excl,
  output logic [2-1:0] clint_icb_cmd_size,
  input logic clint_icb_rsp_valid,
  output logic clint_icb_rsp_ready,
  input logic clint_icb_rsp_err,
  input logic clint_icb_rsp_excl_ok,
  input logic [32-1:0] clint_icb_rsp_rdata,
  input logic [32-1:0] plic_region_indic,
  input logic plic_icb_enable,
  output logic plic_icb_cmd_valid,
  input logic plic_icb_cmd_ready,
  output logic [32-1:0] plic_icb_cmd_addr,
  output logic plic_icb_cmd_read,
  output logic [32-1:0] plic_icb_cmd_wdata,
  output logic [4-1:0] plic_icb_cmd_wmask,
  output logic plic_icb_cmd_lock,
  output logic plic_icb_cmd_excl,
  output logic [2-1:0] plic_icb_cmd_size,
  input logic plic_icb_rsp_valid,
  output logic plic_icb_rsp_ready,
  input logic plic_icb_rsp_err,
  input logic plic_icb_rsp_excl_ok,
  input logic [32-1:0] plic_icb_rsp_rdata,
  input logic [32-1:0] fio_region_indic,
  input logic fio_icb_enable,
  output logic fio_icb_cmd_valid,
  input logic fio_icb_cmd_ready,
  output logic [32-1:0] fio_icb_cmd_addr,
  output logic fio_icb_cmd_read,
  output logic [32-1:0] fio_icb_cmd_wdata,
  output logic [4-1:0] fio_icb_cmd_wmask,
  output logic fio_icb_cmd_lock,
  output logic fio_icb_cmd_excl,
  output logic [2-1:0] fio_icb_cmd_size,
  input logic fio_icb_rsp_valid,
  output logic fio_icb_rsp_ready,
  input logic fio_icb_rsp_err,
  input logic fio_icb_rsp_excl_ok,
  input logic [32-1:0] fio_icb_rsp_rdata,
  input logic mem_icb_enable,
  output logic mem_icb_cmd_valid,
  input logic mem_icb_cmd_ready,
  output logic [32-1:0] mem_icb_cmd_addr,
  output logic mem_icb_cmd_read,
  output logic [32-1:0] mem_icb_cmd_wdata,
  output logic [4-1:0] mem_icb_cmd_wmask,
  output logic mem_icb_cmd_lock,
  output logic mem_icb_cmd_excl,
  output logic [2-1:0] mem_icb_cmd_size,
  output logic [2-1:0] mem_icb_cmd_burst,
  output logic [2-1:0] mem_icb_cmd_beat,
  input logic mem_icb_rsp_valid,
  output logic mem_icb_rsp_ready,
  input logic mem_icb_rsp_err,
  input logic mem_icb_rsp_excl_ok,
  input logic [32-1:0] mem_icb_rsp_rdata,
  output logic [32-1:0] inspect_pc,
  output logic inspect_dbg_irq,
  output logic inspect_mem_cmd_valid,
  output logic inspect_mem_cmd_ready,
  output logic inspect_mem_rsp_valid,
  output logic inspect_mem_rsp_ready,
  output logic inspect_core_clk,
  output logic core_csr_clk,
  output logic core_wfi,
  output logic tm_stop
);

  // Async IRQ inputs
  // Debug interface
  // ITCM load/store indicator
  // ITCM RAM interface
  // DTCM load/store indicator
  // DTCM RAM interface
  // External debug ICB to ITCM
  // External debug ICB to DTCM
  // PPI ICB
  // CLINT ICB
  // PLIC ICB
  // FIO ICB
  // MEM ICB
  // Inspect/diagnostic outputs
  // Stub integration: all outputs tied to zero/false
  assign inspect_pc = 0;
  assign inspect_dbg_irq = 1'b0;
  assign inspect_mem_cmd_valid = 1'b0;
  assign inspect_mem_cmd_ready = 1'b0;
  assign inspect_mem_rsp_valid = 1'b0;
  assign inspect_mem_rsp_ready = 1'b0;
  assign inspect_core_clk = 1'b0;
  assign core_csr_clk = 1'b0;
  assign core_wfi = 1'b0;
  assign tm_stop = 1'b0;
  assign dbg_irq_r = 1'b0;
  assign cmt_dpc = 0;
  assign cmt_dpc_ena = 1'b0;
  assign cmt_dcause = 0;
  assign cmt_dcause_ena = 1'b0;
  assign wr_dcsr_ena = 1'b0;
  assign wr_dpc_ena = 1'b0;
  assign wr_dscratch_ena = 1'b0;
  assign wr_csr_nxt = 0;
  assign rst_itcm = 1'b0;
  assign rst_dtcm = 1'b0;
  assign itcm_ls = 1'b0;
  assign itcm_ram_cs = 1'b0;
  assign itcm_ram_we = 1'b0;
  assign itcm_ram_addr = 0;
  assign itcm_ram_wem = 0;
  assign itcm_ram_din = 0;
  assign clk_itcm_ram = 1'b0;
  assign dtcm_ls = 1'b0;
  assign dtcm_ram_cs = 1'b0;
  assign dtcm_ram_we = 1'b0;
  assign dtcm_ram_addr = 0;
  assign dtcm_ram_wem = 0;
  assign dtcm_ram_din = 0;
  assign clk_dtcm_ram = 1'b0;
  assign ext2itcm_icb_cmd_ready = 1'b0;
  assign ext2itcm_icb_rsp_valid = 1'b0;
  assign ext2itcm_icb_rsp_err = 1'b0;
  assign ext2itcm_icb_rsp_rdata = 0;
  assign ext2dtcm_icb_cmd_ready = 1'b0;
  assign ext2dtcm_icb_rsp_valid = 1'b0;
  assign ext2dtcm_icb_rsp_err = 1'b0;
  assign ext2dtcm_icb_rsp_rdata = 0;
  assign ppi_icb_cmd_valid = 1'b0;
  assign ppi_icb_cmd_addr = 0;
  assign ppi_icb_cmd_read = 1'b1;
  assign ppi_icb_cmd_wdata = 0;
  assign ppi_icb_cmd_wmask = 0;
  assign ppi_icb_cmd_lock = 1'b0;
  assign ppi_icb_cmd_excl = 1'b0;
  assign ppi_icb_cmd_size = 0;
  assign ppi_icb_rsp_ready = 1'b1;
  assign clint_icb_cmd_valid = 1'b0;
  assign clint_icb_cmd_addr = 0;
  assign clint_icb_cmd_read = 1'b1;
  assign clint_icb_cmd_wdata = 0;
  assign clint_icb_cmd_wmask = 0;
  assign clint_icb_cmd_lock = 1'b0;
  assign clint_icb_cmd_excl = 1'b0;
  assign clint_icb_cmd_size = 0;
  assign clint_icb_rsp_ready = 1'b1;
  assign plic_icb_cmd_valid = 1'b0;
  assign plic_icb_cmd_addr = 0;
  assign plic_icb_cmd_read = 1'b1;
  assign plic_icb_cmd_wdata = 0;
  assign plic_icb_cmd_wmask = 0;
  assign plic_icb_cmd_lock = 1'b0;
  assign plic_icb_cmd_excl = 1'b0;
  assign plic_icb_cmd_size = 0;
  assign plic_icb_rsp_ready = 1'b1;
  assign fio_icb_cmd_valid = 1'b0;
  assign fio_icb_cmd_addr = 0;
  assign fio_icb_cmd_read = 1'b1;
  assign fio_icb_cmd_wdata = 0;
  assign fio_icb_cmd_wmask = 0;
  assign fio_icb_cmd_lock = 1'b0;
  assign fio_icb_cmd_excl = 1'b0;
  assign fio_icb_cmd_size = 0;
  assign fio_icb_rsp_ready = 1'b1;
  assign mem_icb_cmd_valid = 1'b0;
  assign mem_icb_cmd_addr = 0;
  assign mem_icb_cmd_read = 1'b1;
  assign mem_icb_cmd_wdata = 0;
  assign mem_icb_cmd_wmask = 0;
  assign mem_icb_cmd_lock = 1'b0;
  assign mem_icb_cmd_excl = 1'b0;
  assign mem_icb_cmd_size = 0;
  assign mem_icb_cmd_burst = 0;
  assign mem_icb_cmd_beat = 0;
  assign mem_icb_rsp_ready = 1'b1;

endmodule

// ITCM (stub)
// DTCM (stub)
// Ext ITCM ICB (stub)
// Ext DTCM ICB (stub)
// PPI ICB (stub)
// CLINT ICB (stub)
// PLIC ICB (stub)
// FIO ICB (stub)
// MEM ICB (stub)
