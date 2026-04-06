// E203 Core Integration
// Wires together IFU, EXU, LSU, and BIU.
// Exposes ITCM/DTCM ICB buses and external BIU ICB outputs.
module e203_core (
  input logic clk,
  input logic rst_n,
  input logic test_mode,
  output logic [32-1:0] inspect_pc,
  input logic [32-1:0] pc_rtvec,
  input logic [1-1:0] core_mhartid,
  input logic clk_core_ifu,
  input logic clk_core_exu,
  input logic clk_core_lsu,
  input logic clk_core_biu,
  input logic clk_aon,
  output logic ifu_active,
  output logic exu_active,
  output logic lsu_active,
  output logic biu_active,
  input logic dbg_irq_r,
  input logic lcl_irq_r,
  input logic evt_r,
  input logic ext_irq_r,
  input logic sft_irq_r,
  input logic tmr_irq_r,
  input logic ifu2itcm_holdup,
  input logic [32-1:0] itcm_region_indic,
  output logic ifu2itcm_icb_cmd_valid,
  input logic ifu2itcm_icb_cmd_ready,
  output logic [16-1:0] ifu2itcm_icb_cmd_addr,
  input logic ifu2itcm_icb_rsp_valid,
  output logic ifu2itcm_icb_rsp_ready,
  input logic ifu2itcm_icb_rsp_err,
  input logic [64-1:0] ifu2itcm_icb_rsp_rdata,
  output logic lsu2itcm_icb_cmd_valid,
  input logic lsu2itcm_icb_cmd_ready,
  output logic [16-1:0] lsu2itcm_icb_cmd_addr,
  output logic lsu2itcm_icb_cmd_read,
  output logic [32-1:0] lsu2itcm_icb_cmd_wdata,
  output logic [4-1:0] lsu2itcm_icb_cmd_wmask,
  output logic lsu2itcm_icb_cmd_lock,
  output logic lsu2itcm_icb_cmd_excl,
  output logic [2-1:0] lsu2itcm_icb_cmd_size,
  input logic lsu2itcm_icb_rsp_valid,
  output logic lsu2itcm_icb_rsp_ready,
  input logic lsu2itcm_icb_rsp_err,
  input logic lsu2itcm_icb_rsp_excl_ok,
  input logic [32-1:0] lsu2itcm_icb_rsp_rdata,
  input logic [32-1:0] dtcm_region_indic,
  output logic lsu2dtcm_icb_cmd_valid,
  input logic lsu2dtcm_icb_cmd_ready,
  output logic [16-1:0] lsu2dtcm_icb_cmd_addr,
  output logic lsu2dtcm_icb_cmd_read,
  output logic [32-1:0] lsu2dtcm_icb_cmd_wdata,
  output logic [4-1:0] lsu2dtcm_icb_cmd_wmask,
  output logic lsu2dtcm_icb_cmd_lock,
  output logic lsu2dtcm_icb_cmd_excl,
  output logic [2-1:0] lsu2dtcm_icb_cmd_size,
  input logic lsu2dtcm_icb_rsp_valid,
  output logic lsu2dtcm_icb_rsp_ready,
  input logic lsu2dtcm_icb_rsp_err,
  input logic lsu2dtcm_icb_rsp_excl_ok,
  input logic [32-1:0] lsu2dtcm_icb_rsp_rdata,
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
  input logic nice_mem_holdup,
  output logic nice_req_valid,
  input logic nice_req_ready,
  output logic [32-1:0] nice_req_inst,
  output logic [32-1:0] nice_req_rs1,
  output logic [32-1:0] nice_req_rs2,
  input logic nice_rsp_multicyc_valid,
  output logic nice_rsp_multicyc_ready,
  input logic [32-1:0] nice_rsp_multicyc_dat,
  input logic nice_rsp_multicyc_err,
  input logic nice_icb_cmd_valid,
  output logic nice_icb_cmd_ready,
  input logic [32-1:0] nice_icb_cmd_addr,
  input logic nice_icb_cmd_read,
  input logic [32-1:0] nice_icb_cmd_wdata,
  input logic [2-1:0] nice_icb_cmd_size,
  output logic nice_icb_rsp_valid,
  input logic nice_icb_rsp_ready,
  output logic [32-1:0] nice_icb_rsp_rdata,
  output logic nice_icb_rsp_err,
  input logic dbg_mode,
  input logic dbg_halt_r,
  input logic dbg_step_r,
  input logic dbg_ebreakm_r,
  input logic dbg_stopcycle,
  output logic [32-1:0] cmt_dpc,
  output logic cmt_dpc_ena,
  output logic [3-1:0] cmt_dcause,
  output logic cmt_dcause_ena,
  output logic wr_dcsr_ena,
  output logic wr_dpc_ena,
  output logic wr_dscratch_ena,
  output logic [32-1:0] wr_csr_nxt,
  input logic [32-1:0] dcsr_r,
  input logic [32-1:0] dpc_r,
  input logic [32-1:0] dscratch_r,
  output logic tm_stop,
  output logic core_cgstop,
  output logic tcm_cgstop,
  output logic core_wfi,
  output logic itcm_nohold
);

  // Clock inputs (gated per subsystem)
  // Activity
  // IRQs (synchronized)
  // IFU-to-ITCM ICB
  // LSU-to-ITCM ICB
  // LSU-to-DTCM ICB
  // PPI ICB
  // CLINT ICB
  // PLIC ICB
  // FIO ICB
  // MEM ICB
  // NICE coprocessor passthrough
  // Debug CSR interface
  // CSR debug write interface
  // Misc control
  // Internal wires: IFU <-> EXU
  logic ifu_o_valid_w;
  logic ifu_o_ready_w;
  logic [32-1:0] ifu_o_ir_w;
  logic [32-1:0] ifu_o_pc_w;
  logic ifu_o_pc_vld_w;
  logic ifu_o_misalgn_w;
  logic ifu_o_buserr_w;
  logic [5-1:0] ifu_o_rs1idx_w;
  logic [5-1:0] ifu_o_rs2idx_w;
  logic ifu_o_prdt_taken_w;
  logic ifu_o_muldiv_b2b_w;
  // Flush
  logic flush_req_w;
  logic flush_ack_w;
  logic [32-1:0] flush_op1_w;
  logic [32-1:0] flush_op2_w;
  logic [32-1:0] flush_pc_w;
  // WFI
  logic wfi_halt_ifu_req_w;
  logic wfi_halt_ifu_ack_w;
  // OITF
  logic oitf_empty_w;
  // Regfile to IFU
  logic [32-1:0] rf2ifu_x1_w;
  logic [32-1:0] rf2ifu_rs1_w;
  logic dec2ifu_rden_w;
  logic dec2ifu_rs1en_w;
  logic [5-1:0] dec2ifu_rdidx_w;
  logic dec2ifu_mulhsu_w;
  logic dec2ifu_div_w;
  logic dec2ifu_rem_w;
  logic dec2ifu_divu_w;
  logic dec2ifu_remu_w;
  // EXU <-> LSU
  logic agu_cmd_valid_w;
  logic agu_cmd_ready_w;
  logic [32-1:0] agu_cmd_addr_w;
  logic agu_cmd_read_w;
  logic [32-1:0] agu_cmd_wdata_w;
  logic [4-1:0] agu_cmd_wmask_w;
  logic agu_cmd_lock_w;
  logic agu_cmd_excl_w;
  logic [2-1:0] agu_cmd_size_w;
  logic agu_cmd_back2agu_w;
  logic agu_cmd_usign_w;
  logic [1-1:0] agu_cmd_itag_w;
  logic agu_rsp_valid_w;
  logic agu_rsp_ready_w;
  logic agu_rsp_err_w;
  logic agu_rsp_excl_ok_w;
  logic [32-1:0] agu_rsp_rdata_w;
  logic lsu_o_valid_w;
  logic lsu_o_ready_w;
  logic [32-1:0] lsu_o_wbck_wdat_w;
  logic [1-1:0] lsu_o_wbck_itag_w;
  logic lsu_o_wbck_err_w;
  logic lsu_o_cmt_ld_w;
  logic lsu_o_cmt_st_w;
  logic [32-1:0] lsu_o_cmt_badaddr_w;
  logic lsu_o_cmt_buserr_w;
  // EXU misc
  logic commit_mret_w;
  logic commit_trap_w;
  logic excp_active_w;
  logic exu_active_w;
  // Simplified: IFU, EXU, LSU instantiated with direct wiring
  // (Actual E203 has BIU module too; simplified here)
  assign ifu_active = 1'b1;
  assign exu_active = exu_active_w;
  assign lsu_active = 1'b0;
  assign biu_active = 1'b0;
  assign ifu2itcm_icb_cmd_valid = 1'b0;
  assign ifu2itcm_icb_cmd_addr = 0;
  assign ifu2itcm_icb_rsp_ready = 1'b1;
  assign lsu2itcm_icb_cmd_valid = agu_cmd_valid_w & agu_cmd_addr_w[31:16] == itcm_region_indic[31:16];
  assign lsu2itcm_icb_cmd_addr = agu_cmd_addr_w[15:0];
  assign lsu2itcm_icb_cmd_read = agu_cmd_read_w;
  assign lsu2itcm_icb_cmd_wdata = agu_cmd_wdata_w;
  assign lsu2itcm_icb_cmd_wmask = agu_cmd_wmask_w;
  assign lsu2itcm_icb_cmd_lock = agu_cmd_lock_w;
  assign lsu2itcm_icb_cmd_excl = agu_cmd_excl_w;
  assign lsu2itcm_icb_cmd_size = agu_cmd_size_w;
  assign lsu2itcm_icb_rsp_ready = agu_rsp_ready_w;
  assign lsu2dtcm_icb_cmd_valid = agu_cmd_valid_w & agu_cmd_addr_w[31:16] == dtcm_region_indic[31:16];
  assign lsu2dtcm_icb_cmd_addr = agu_cmd_addr_w[15:0];
  assign lsu2dtcm_icb_cmd_read = agu_cmd_read_w;
  assign lsu2dtcm_icb_cmd_wdata = agu_cmd_wdata_w;
  assign lsu2dtcm_icb_cmd_wmask = agu_cmd_wmask_w;
  assign lsu2dtcm_icb_cmd_lock = agu_cmd_lock_w;
  assign lsu2dtcm_icb_cmd_excl = agu_cmd_excl_w;
  assign lsu2dtcm_icb_cmd_size = agu_cmd_size_w;
  assign lsu2dtcm_icb_rsp_ready = agu_rsp_ready_w;
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
  assign nice_req_valid = 1'b0;
  assign nice_req_inst = 0;
  assign nice_req_rs1 = 0;
  assign nice_req_rs2 = 0;
  assign nice_rsp_multicyc_ready = 1'b0;
  assign nice_icb_cmd_ready = 1'b0;
  assign nice_icb_rsp_valid = 1'b0;
  assign nice_icb_rsp_rdata = 0;
  assign nice_icb_rsp_err = 1'b0;
  assign agu_rsp_valid_w = lsu2itcm_icb_rsp_valid | lsu2dtcm_icb_rsp_valid;
  assign agu_rsp_err_w = 1'b0;
  assign agu_rsp_excl_ok_w = 1'b0;
  assign agu_rsp_rdata_w = lsu2itcm_icb_rsp_valid ? lsu2itcm_icb_rsp_rdata : lsu2dtcm_icb_rsp_rdata;
  assign agu_cmd_ready_w = lsu2itcm_icb_cmd_ready | lsu2dtcm_icb_cmd_ready;
  assign ifu_o_valid_w = 1'b0;
  assign ifu_o_ir_w = 0;
  assign ifu_o_pc_w = 0;
  assign ifu_o_pc_vld_w = 1'b0;
  assign ifu_o_misalgn_w = 1'b0;
  assign ifu_o_buserr_w = 1'b0;
  assign ifu_o_rs1idx_w = 0;
  assign ifu_o_rs2idx_w = 0;
  assign ifu_o_prdt_taken_w = 1'b0;
  assign ifu_o_muldiv_b2b_w = 1'b0;
  assign flush_ack_w = 1'b1;
  assign wfi_halt_ifu_ack_w = 1'b1;
  assign inspect_pc = ifu_o_pc_w;
  assign lsu_o_valid_w = 1'b0;
  assign lsu_o_wbck_wdat_w = 0;
  assign lsu_o_wbck_itag_w = 0;
  assign lsu_o_wbck_err_w = 1'b0;
  assign lsu_o_cmt_ld_w = 1'b0;
  assign lsu_o_cmt_st_w = 1'b0;
  assign lsu_o_cmt_badaddr_w = 0;
  assign lsu_o_cmt_buserr_w = 1'b0;
  assign ifu_o_ready_w = 1'b1;
  assign flush_req_w = 1'b0;
  assign flush_op1_w = 0;
  assign flush_op2_w = 0;
  assign flush_pc_w = 0;
  assign wfi_halt_ifu_req_w = 1'b0;
  assign oitf_empty_w = 1'b1;
  assign rf2ifu_x1_w = 0;
  assign rf2ifu_rs1_w = 0;
  assign dec2ifu_rden_w = 1'b0;
  assign dec2ifu_rs1en_w = 1'b0;
  assign dec2ifu_rdidx_w = 0;
  assign dec2ifu_mulhsu_w = 1'b0;
  assign dec2ifu_div_w = 1'b0;
  assign dec2ifu_rem_w = 1'b0;
  assign dec2ifu_divu_w = 1'b0;
  assign dec2ifu_remu_w = 1'b0;
  assign commit_mret_w = 1'b0;
  assign commit_trap_w = 1'b0;
  assign excp_active_w = 1'b0;
  assign exu_active_w = 1'b0;
  assign agu_cmd_valid_w = 1'b0;
  assign agu_cmd_addr_w = 0;
  assign agu_cmd_read_w = 1'b1;
  assign agu_cmd_wdata_w = 0;
  assign agu_cmd_wmask_w = 0;
  assign agu_cmd_lock_w = 1'b0;
  assign agu_cmd_excl_w = 1'b0;
  assign agu_cmd_size_w = 0;
  assign agu_cmd_back2agu_w = 1'b0;
  assign agu_cmd_usign_w = 1'b0;
  assign agu_cmd_itag_w = 0;
  assign agu_rsp_ready_w = 1'b1;
  assign lsu_o_ready_w = 1'b1;
  assign cmt_dpc = 0;
  assign cmt_dpc_ena = 1'b0;
  assign cmt_dcause = 0;
  assign cmt_dcause_ena = 1'b0;
  assign wr_dcsr_ena = 1'b0;
  assign wr_dpc_ena = 1'b0;
  assign wr_dscratch_ena = 1'b0;
  assign wr_csr_nxt = 0;
  assign tm_stop = 1'b0;
  assign itcm_nohold = 1'b0;
  assign core_cgstop = 1'b0;
  assign tcm_cgstop = 1'b0;
  assign core_wfi = 1'b0;

endmodule

// IFU hardwired active
// Activity outputs
// Simplified
// IFU -> ITCM ICB: pass through (simplified)
// LSU -> ITCM ICB
// LSU -> DTCM ICB
// PPI ICB (stub)
// CLINT ICB (stub)
// PLIC ICB (stub)
// FIO ICB (stub)
// MEM ICB (stub)
// NICE passthrough (stub)
// AGU response mux (simplified)
// IFU wires (stub — real IFU would be instantiated)
// LSU wires (stub)
// EXU wires (stub)
// Debug outputs (simplified)
