// E203 CPU Top
// Top-level wrapper: integrates e203_cpu + e203_srams.
module e203_cpu_top (
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
  input logic tcm_sd,
  input logic tcm_ds,
  output logic dbg_irq_r,
  output logic wr_dcsr_ena,
  output logic wr_dpc_ena,
  output logic wr_dscratch_ena,
  output logic [32-1:0] wr_csr_nxt,
  input logic [32-1:0] dcsr_r,
  input logic [32-1:0] dpc_r,
  input logic [32-1:0] dscratch_r,
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
  output logic ppi_icb_cmd_valid,
  input logic ppi_icb_cmd_ready,
  output logic [32-1:0] ppi_icb_cmd_addr,
  output logic ppi_icb_cmd_read,
  output logic [32-1:0] ppi_icb_cmd_wdata,
  output logic [4-1:0] ppi_icb_cmd_wmask,
  input logic ppi_icb_rsp_valid,
  output logic ppi_icb_rsp_ready,
  input logic ppi_icb_rsp_err,
  input logic [32-1:0] ppi_icb_rsp_rdata,
  output logic clint_icb_cmd_valid,
  input logic clint_icb_cmd_ready,
  output logic [32-1:0] clint_icb_cmd_addr,
  output logic clint_icb_cmd_read,
  output logic [32-1:0] clint_icb_cmd_wdata,
  output logic [4-1:0] clint_icb_cmd_wmask,
  input logic clint_icb_rsp_valid,
  output logic clint_icb_rsp_ready,
  input logic clint_icb_rsp_err,
  input logic [32-1:0] clint_icb_rsp_rdata,
  output logic plic_icb_cmd_valid,
  input logic plic_icb_cmd_ready,
  output logic [32-1:0] plic_icb_cmd_addr,
  output logic plic_icb_cmd_read,
  output logic [32-1:0] plic_icb_cmd_wdata,
  output logic [4-1:0] plic_icb_cmd_wmask,
  input logic plic_icb_rsp_valid,
  output logic plic_icb_rsp_ready,
  input logic plic_icb_rsp_err,
  input logic [32-1:0] plic_icb_rsp_rdata,
  output logic fio_icb_cmd_valid,
  input logic fio_icb_cmd_ready,
  output logic [32-1:0] fio_icb_cmd_addr,
  output logic fio_icb_cmd_read,
  output logic [32-1:0] fio_icb_cmd_wdata,
  output logic [4-1:0] fio_icb_cmd_wmask,
  input logic fio_icb_rsp_valid,
  output logic fio_icb_rsp_ready,
  input logic fio_icb_rsp_err,
  input logic [32-1:0] fio_icb_rsp_rdata,
  output logic mem_icb_cmd_valid,
  input logic mem_icb_cmd_ready,
  output logic [32-1:0] mem_icb_cmd_addr,
  output logic mem_icb_cmd_read,
  output logic [32-1:0] mem_icb_cmd_wdata,
  output logic [4-1:0] mem_icb_cmd_wmask,
  input logic mem_icb_rsp_valid,
  output logic mem_icb_rsp_ready,
  input logic mem_icb_rsp_err,
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
  // Debug
  // TCM power management
  // CSR debug write interface
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
  assign ppi_icb_rsp_ready = 1'b1;
  assign clint_icb_cmd_valid = 1'b0;
  assign clint_icb_cmd_addr = 0;
  assign clint_icb_cmd_read = 1'b1;
  assign clint_icb_cmd_wdata = 0;
  assign clint_icb_cmd_wmask = 0;
  assign clint_icb_rsp_ready = 1'b1;
  assign plic_icb_cmd_valid = 1'b0;
  assign plic_icb_cmd_addr = 0;
  assign plic_icb_cmd_read = 1'b1;
  assign plic_icb_cmd_wdata = 0;
  assign plic_icb_cmd_wmask = 0;
  assign plic_icb_rsp_ready = 1'b1;
  assign fio_icb_cmd_valid = 1'b0;
  assign fio_icb_cmd_addr = 0;
  assign fio_icb_cmd_read = 1'b1;
  assign fio_icb_cmd_wdata = 0;
  assign fio_icb_cmd_wmask = 0;
  assign fio_icb_rsp_ready = 1'b1;
  assign mem_icb_cmd_valid = 1'b0;
  assign mem_icb_cmd_addr = 0;
  assign mem_icb_cmd_read = 1'b1;
  assign mem_icb_cmd_wdata = 0;
  assign mem_icb_cmd_wmask = 0;
  assign mem_icb_rsp_ready = 1'b1;

endmodule

// Ext ITCM ICB (stub)
// Ext DTCM ICB (stub)
// PPI ICB (stub)
// CLINT ICB (stub)
// PLIC ICB (stub)
// FIO ICB (stub)
// MEM ICB (stub)
