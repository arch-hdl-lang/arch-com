// E203 HBirdv2 Execution Commit Unit
// Handles commit of ALU results, exception processing, trap/mret generation,
// WFI, pipeline flush, and debug mode interactions.
// Matches RealBench port interface.
module e203_exu_commit #(
  parameter int XLEN = 32
) (
  input logic clk,
  input logic rst_n,
  output logic commit_mret,
  output logic commit_trap,
  output logic core_wfi,
  output logic nonflush_cmt_ena,
  output logic excp_active,
  input logic amo_wait,
  output logic wfi_halt_ifu_req,
  output logic wfi_halt_exu_req,
  input logic wfi_halt_ifu_ack,
  input logic wfi_halt_exu_ack,
  input logic dbg_irq_r,
  input logic lcl_irq_r,
  input logic ext_irq_r,
  input logic sft_irq_r,
  input logic tmr_irq_r,
  input logic evt_r,
  input logic status_mie_r,
  input logic mtie_r,
  input logic msie_r,
  input logic meie_r,
  input logic alu_cmt_i_valid,
  output logic alu_cmt_i_ready,
  input logic [32-1:0] alu_cmt_i_pc,
  input logic [32-1:0] alu_cmt_i_instr,
  input logic alu_cmt_i_pc_vld,
  input logic [32-1:0] alu_cmt_i_imm,
  input logic alu_cmt_i_rv32,
  input logic alu_cmt_i_bjp,
  input logic alu_cmt_i_wfi,
  input logic alu_cmt_i_fencei,
  input logic alu_cmt_i_mret,
  input logic alu_cmt_i_dret,
  input logic alu_cmt_i_ecall,
  input logic alu_cmt_i_ebreak,
  input logic alu_cmt_i_ifu_misalgn,
  input logic alu_cmt_i_ifu_buserr,
  input logic alu_cmt_i_ifu_ilegl,
  input logic alu_cmt_i_bjp_prdt,
  input logic alu_cmt_i_bjp_rslv,
  input logic alu_cmt_i_misalgn,
  input logic alu_cmt_i_ld,
  input logic alu_cmt_i_stamo,
  input logic alu_cmt_i_buserr,
  input logic [32-1:0] alu_cmt_i_badaddr,
  output logic [32-1:0] cmt_badaddr,
  output logic cmt_badaddr_ena,
  output logic [32-1:0] cmt_epc,
  output logic cmt_epc_ena,
  output logic [32-1:0] cmt_cause,
  output logic cmt_cause_ena,
  output logic cmt_instret_ena,
  output logic cmt_status_ena,
  output logic [32-1:0] cmt_dpc,
  output logic cmt_dpc_ena,
  output logic [3-1:0] cmt_dcause,
  output logic cmt_dcause_ena,
  output logic cmt_mret_ena,
  input logic [32-1:0] csr_epc_r,
  input logic [32-1:0] csr_dpc_r,
  input logic [32-1:0] csr_mtvec_r,
  input logic dbg_mode,
  input logic dbg_halt_r,
  input logic dbg_step_r,
  input logic dbg_ebreakm_r,
  input logic oitf_empty,
  input logic u_mode,
  input logic s_mode,
  input logic h_mode,
  input logic m_mode,
  output logic longp_excp_i_ready,
  input logic longp_excp_i_valid,
  input logic longp_excp_i_ld,
  input logic longp_excp_i_st,
  input logic longp_excp_i_buserr,
  input logic [32-1:0] longp_excp_i_badaddr,
  input logic longp_excp_i_insterr,
  input logic [32-1:0] longp_excp_i_pc,
  output logic flush_pulse,
  output logic flush_req,
  input logic pipe_flush_ack,
  output logic pipe_flush_req,
  output logic [32-1:0] pipe_flush_add_op1,
  output logic [32-1:0] pipe_flush_add_op2,
  output logic [32-1:0] pipe_flush_pc
);

  // ── Commit status outputs ─────────────────────────────────────────
  // ── AMO wait ──────────────────────────────────────────────────────
  // ── WFI halt interface ────────────────────────────────────────────
  // ── Interrupt inputs ──────────────────────────────────────────────
  // ── ALU commit input channel ──────────────────────────────────────
  // ── CSR commit outputs ────────────────────────────────────────────
  // ── CSR read inputs ───────────────────────────────────────────────
  // ── Debug mode inputs ─────────────────────────────────────────────
  // ── Privilege mode inputs ─────────────────────────────────────────
  // ── Long-pipe exception input ─────────────────────────────────────
  // ── Flush outputs ─────────────────────────────────────────────────
  // ── WFI state register ────────────────────────────────────────────
  logic wfi_flag_r = 0;
  logic flush_req_r = 0;
  // ── Interrupt pending check ───────────────────────────────────────
  logic irq_ext_pend;
  assign irq_ext_pend = ext_irq_r & meie_r & status_mie_r;
  logic irq_sft_pend;
  assign irq_sft_pend = sft_irq_r & msie_r & status_mie_r;
  logic irq_tmr_pend;
  assign irq_tmr_pend = tmr_irq_r & mtie_r & status_mie_r;
  logic any_irq;
  assign any_irq = irq_ext_pend | irq_sft_pend | irq_tmr_pend | dbg_irq_r | lcl_irq_r | evt_r;
  // ── Commit logic ──────────────────────────────────────────────────
  logic cmt_ena;
  assign cmt_ena = alu_cmt_i_valid & ~amo_wait;
  // Exception conditions
  logic has_excp;
  assign has_excp = alu_cmt_i_ifu_misalgn | alu_cmt_i_ifu_buserr | alu_cmt_i_ifu_ilegl | alu_cmt_i_ecall | alu_cmt_i_ebreak | alu_cmt_i_misalgn | alu_cmt_i_buserr;
  logic need_flush;
  assign need_flush = alu_cmt_i_bjp | alu_cmt_i_fencei | alu_cmt_i_mret | alu_cmt_i_dret | alu_cmt_i_wfi | has_excp;
  // BJP misprediction
  logic bjp_mispred;
  assign bjp_mispred = alu_cmt_i_bjp & alu_cmt_i_bjp_prdt != alu_cmt_i_bjp_rslv;
  // Trap generation
  logic trap_ena;
  assign trap_ena = cmt_ena & has_excp;
  logic mret_ena;
  assign mret_ena = cmt_ena & alu_cmt_i_mret & ~has_excp;
  // PC increment for non-flush commit
  logic [32-1:0] pc_incr;
  assign pc_incr = alu_cmt_i_rv32 ? 4 : 2;
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      flush_req_r <= 0;
      wfi_flag_r <= 0;
    end else begin
      // WFI flag management
      if (cmt_ena & alu_cmt_i_wfi & ~any_irq) begin
        wfi_flag_r <= 1'b1;
      end else if (any_irq) begin
        wfi_flag_r <= 1'b0;
      end
      // Flush request register
      if (pipe_flush_ack) begin
        flush_req_r <= 1'b0;
      end else if (cmt_ena & need_flush) begin
        flush_req_r <= 1'b1;
      end
    end
  end
  always_comb begin
    // ALU commit ready: always accept (single-cycle)
    alu_cmt_i_ready = ~amo_wait;
    // Commit status
    commit_mret = mret_ena;
    commit_trap = trap_ena;
    core_wfi = wfi_flag_r;
    nonflush_cmt_ena = cmt_ena & ~need_flush;
    excp_active = trap_ena;
    // WFI halt requests
    wfi_halt_ifu_req = wfi_flag_r;
    wfi_halt_exu_req = wfi_flag_r;
    // CSR commit outputs
    cmt_epc_ena = trap_ena;
    cmt_epc = alu_cmt_i_pc;
    cmt_cause_ena = trap_ena;
    cmt_badaddr_ena = trap_ena & (alu_cmt_i_misalgn | alu_cmt_i_buserr);
    cmt_badaddr = alu_cmt_i_badaddr;
    cmt_instret_ena = cmt_ena & ~has_excp;
    cmt_status_ena = trap_ena | mret_ena;
    cmt_mret_ena = mret_ena;
    // Trap cause encoding (simplified)
    if (alu_cmt_i_ifu_misalgn) begin
      cmt_cause = 0;
    end else if (alu_cmt_i_ifu_buserr) begin
      cmt_cause = 1;
    end else if (alu_cmt_i_ifu_ilegl) begin
      cmt_cause = 2;
    end else if (alu_cmt_i_ebreak) begin
      cmt_cause = 3;
    end else if (alu_cmt_i_misalgn & alu_cmt_i_ld) begin
      cmt_cause = 4;
    end else if (alu_cmt_i_buserr & alu_cmt_i_ld) begin
      cmt_cause = 5;
    end else if (alu_cmt_i_misalgn & alu_cmt_i_stamo) begin
      cmt_cause = 6;
    end else if (alu_cmt_i_buserr & alu_cmt_i_stamo) begin
      cmt_cause = 7;
    end else if (alu_cmt_i_ecall & u_mode) begin
      cmt_cause = 8;
    end else if (alu_cmt_i_ecall & m_mode) begin
      cmt_cause = 11;
    end else begin
      cmt_cause = 0;
    end
    // Debug CSR outputs
    cmt_dpc_ena = trap_ena & dbg_mode;
    cmt_dpc = alu_cmt_i_pc;
    cmt_dcause_ena = trap_ena & dbg_mode;
    cmt_dcause = 0;
    // Long-pipe exception handling
    longp_excp_i_ready = ~alu_cmt_i_valid;
    // Flush outputs
    flush_pulse = cmt_ena & need_flush;
    flush_req = flush_req_r;
    pipe_flush_req = flush_req_r;
    // Flush target address
    if (trap_ena) begin
      pipe_flush_add_op1 = csr_mtvec_r;
      pipe_flush_add_op2 = 0;
      pipe_flush_pc = csr_mtvec_r;
    end else if (mret_ena) begin
      pipe_flush_add_op1 = csr_epc_r;
      pipe_flush_add_op2 = 0;
      pipe_flush_pc = csr_epc_r;
    end else if (alu_cmt_i_dret) begin
      pipe_flush_add_op1 = csr_dpc_r;
      pipe_flush_add_op2 = 0;
      pipe_flush_pc = csr_dpc_r;
    end else begin
      pipe_flush_add_op1 = alu_cmt_i_pc;
      pipe_flush_add_op2 = pc_incr;
      pipe_flush_pc = alu_cmt_i_pc;
    end
  end

endmodule

