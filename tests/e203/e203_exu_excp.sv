// E203 Exception Handler
// Manages exceptions, interrupts, debug-mode entry, and WFI.
// Priority: longpipe_excp > debug_entry > IRQ > ALU_excp.
module e203_exu_excp (
  input logic clk,
  input logic rst_n,
  output logic commit_trap,
  output logic core_wfi,
  output logic wfi_halt_ifu_req,
  output logic wfi_halt_exu_req,
  input logic wfi_halt_ifu_ack,
  input logic wfi_halt_exu_ack,
  input logic amo_wait,
  output logic alu_excp_i_ready,
  input logic alu_excp_i_valid,
  input logic alu_excp_i_ld,
  input logic alu_excp_i_stamo,
  input logic alu_excp_i_misalgn,
  input logic alu_excp_i_buserr,
  input logic alu_excp_i_ecall,
  input logic alu_excp_i_ebreak,
  input logic alu_excp_i_wfi,
  input logic alu_excp_i_ifu_misalgn,
  input logic alu_excp_i_ifu_buserr,
  input logic alu_excp_i_ifu_ilegl,
  input logic [32-1:0] alu_excp_i_badaddr,
  input logic [32-1:0] alu_excp_i_pc,
  input logic [32-1:0] alu_excp_i_instr,
  input logic alu_excp_i_pc_vld,
  output logic longp_excp_i_ready,
  input logic longp_excp_i_valid,
  input logic longp_excp_i_ld,
  input logic longp_excp_i_st,
  input logic longp_excp_i_buserr,
  input logic longp_excp_i_insterr,
  input logic [32-1:0] longp_excp_i_badaddr,
  input logic [32-1:0] longp_excp_i_pc,
  input logic excpirq_flush_ack,
  output logic excpirq_flush_req,
  output logic nonalu_excpirq_flush_req_raw,
  output logic [32-1:0] excpirq_flush_add_op1,
  output logic [32-1:0] excpirq_flush_add_op2,
  output logic [32-1:0] excpirq_flush_pc,
  input logic [32-1:0] csr_mtvec_r,
  input logic cmt_dret_ena,
  input logic cmt_ena,
  output logic [32-1:0] cmt_badaddr,
  output logic [32-1:0] cmt_epc,
  output logic [32-1:0] cmt_cause,
  output logic cmt_badaddr_ena,
  output logic cmt_epc_ena,
  output logic cmt_cause_ena,
  output logic cmt_status_ena,
  output logic [32-1:0] cmt_dpc,
  output logic cmt_dpc_ena,
  output logic [3-1:0] cmt_dcause,
  output logic cmt_dcause_ena,
  input logic dbg_irq_r,
  input logic lcl_irq_r,
  input logic ext_irq_r,
  input logic sft_irq_r,
  input logic tmr_irq_r,
  input logic status_mie_r,
  input logic mtie_r,
  input logic msie_r,
  input logic meie_r,
  input logic dbg_mode,
  input logic dbg_halt_r,
  input logic dbg_step_r,
  input logic dbg_ebreakm_r,
  input logic oitf_empty,
  input logic u_mode,
  input logic s_mode,
  input logic h_mode,
  input logic m_mode,
  output logic excp_active
);

  // ALU exception inputs
  // Long-pipe exception inputs
  // Flush interface
  // CSR inputs
  // CSR outputs
  // IRQ inputs
  // CSR status
  // Debug
  // Privilege mode
  // Internal state
  logic wfi_flag_r;
  logic wfi_halt_req_r;
  logic step_req_r;
  // Wires for complex combinational logic
  logic irq_req;
  logic wfi_irq_req;
  logic irq_req_active_w;
  logic longp_need_flush;
  logic alu_need_flush;
  logic dbg_entry_req;
  logic nonalu_dbg_entry_req;
  logic nonalu_dbg_entry_req_raw_w;
  logic dbg_step_req;
  logic dbg_trig_req;
  logic dbg_ebrk_req;
  logic dbg_irq_req_w;
  logic dbg_halt_req;
  logic alu_ebreakm_flush_req;
  logic alu_excp_i_ebreak4excp;
  logic alu_excp_i_ebreak4dbg;
  logic longp_excp_flush_req;
  logic dbg_entry_flush_req;
  logic irq_flush_req;
  logic alu_excp_flush_req;
  logic all_excp_flush_req;
  logic excpirq_taken_ena;
  logic excp_taken_ena;
  logic irq_taken_ena;
  logic dbg_entry_taken_ena;
  logic wfi_flag_set;
  logic wfi_flag_clr;
  logic wfi_halt_req_set;
  logic [32-1:0] irq_cause_w;
  logic [32-1:0] excp_cause_w;
  logic excp_flush_by_alu_agu;
  logic excp_flush_by_longp_ldst;
  // WFI flag: set on 4-way handshake, clear on irq/dbg
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      wfi_flag_r <= 1'b0;
    end else begin
      if (wfi_flag_set | wfi_flag_clr) begin
        wfi_flag_r <= wfi_flag_set & ~wfi_flag_clr;
      end
    end
  end
  // WFI halt request: set on WFI commit, clear same as wfi_flag
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      wfi_halt_req_r <= 1'b0;
    end else begin
      if (wfi_halt_req_set | wfi_flag_clr) begin
        wfi_halt_req_r <= wfi_halt_req_set & ~wfi_flag_clr;
      end
    end
  end
  // Step request
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      step_req_r <= 1'b0;
    end else begin
      if (dbg_entry_taken_ena) begin
        step_req_r <= 1'b0;
      end else if (~dbg_mode & dbg_step_r & cmt_ena & ~dbg_entry_taken_ena) begin
        step_req_r <= 1'b1;
      end
    end
  end
  always_comb begin
    // Long-pipe exception always causes flush
    longp_need_flush = longp_excp_i_valid;
    // ebreak handling: ebreak4excp does NOT depend on alu_need_flush
    alu_excp_i_ebreak4excp = alu_excp_i_ebreak & (~dbg_ebreakm_r | dbg_mode);
    // ALU exceptions (computed before ebreak4dbg to break circular dep)
    alu_need_flush = alu_excp_i_misalgn | alu_excp_i_buserr | alu_excp_i_ebreak4excp | alu_excp_i_ecall | alu_excp_i_ifu_misalgn | alu_excp_i_ifu_buserr | alu_excp_i_ifu_ilegl;
    // ebreak4dbg depends on alu_need_flush (overridden by other ALU exceptions)
    alu_excp_i_ebreak4dbg = alu_excp_i_ebreak & ~alu_need_flush & dbg_ebreakm_r & ~dbg_mode;
    alu_ebreakm_flush_req = alu_excp_i_valid & alu_excp_i_ebreak4dbg;
    // Debug entry priority
    dbg_step_req = step_req_r;
    dbg_trig_req = 1'b0;
    // No trigger support
    dbg_ebrk_req = alu_ebreakm_flush_req & ~step_req_r;
    dbg_irq_req_w = dbg_irq_r & ~alu_ebreakm_flush_req & ~step_req_r;
    dbg_halt_req = dbg_halt_r & ~dbg_irq_r & ~alu_ebreakm_flush_req & ~step_req_r & ~dbg_step_r;
    dbg_entry_req = ~dbg_mode & (dbg_irq_req_w & ~amo_wait | dbg_halt_req & ~amo_wait | dbg_step_req | dbg_ebrk_req);
    nonalu_dbg_entry_req = ~dbg_mode & (dbg_irq_r & ~step_req_r & ~amo_wait | dbg_halt_r & ~dbg_irq_r & ~step_req_r & ~dbg_step_r & ~amo_wait | step_req_r);
    nonalu_dbg_entry_req_raw_w = ~dbg_mode & (dbg_irq_r | dbg_halt_r | step_req_r);
    // IRQ handling
    irq_req = ~(dbg_mode | dbg_step_r | ~status_mie_r | amo_wait) & (ext_irq_r & meie_r | sft_irq_r & msie_r | tmr_irq_r & mtie_r);
    wfi_irq_req = ~(dbg_mode | dbg_step_r) & (ext_irq_r & meie_r | sft_irq_r & msie_r | tmr_irq_r & mtie_r);
    irq_req_active_w = wfi_flag_r ? wfi_irq_req : irq_req;
    excp_active = irq_req_active_w | nonalu_dbg_entry_req_raw_w;
    // Flush request priority
    longp_excp_flush_req = longp_need_flush;
    dbg_entry_flush_req = dbg_entry_req & oitf_empty & alu_excp_i_pc_vld & ~longp_need_flush;
    irq_flush_req = irq_req & oitf_empty & alu_excp_i_pc_vld & ~dbg_entry_req & ~longp_need_flush;
    alu_excp_flush_req = alu_excp_i_valid & alu_need_flush & oitf_empty & ~irq_req & ~dbg_entry_req & ~longp_need_flush;
    all_excp_flush_req = longp_excp_flush_req | alu_excp_flush_req;
    excpirq_flush_req = longp_excp_flush_req | dbg_entry_flush_req | irq_flush_req | alu_excp_flush_req;
    nonalu_excpirq_flush_req_raw = longp_need_flush | nonalu_dbg_entry_req_raw_w | irq_req;
    // Taken enables
    excpirq_taken_ena = excpirq_flush_req & excpirq_flush_ack;
    excp_taken_ena = all_excp_flush_req & excpirq_taken_ena;
    irq_taken_ena = irq_flush_req & excpirq_taken_ena;
    dbg_entry_taken_ena = dbg_entry_flush_req & excpirq_taken_ena;
    commit_trap = excpirq_taken_ena;
    // WFI control
    // wfi_halt_req set on WFI commit (not in debug mode)
    wfi_halt_req_set = alu_excp_i_wfi & cmt_ena & ~dbg_mode;
    // wfi_flag_clr computed first (depends only on irq/dbg, no circularity)
    wfi_flag_clr = wfi_irq_req | dbg_entry_req;
    // wfi_halt outputs (depend on registered values and wfi_flag_clr)
    wfi_halt_ifu_req = wfi_halt_req_r & ~wfi_flag_clr;
    wfi_halt_exu_req = wfi_halt_req_r;
    // wfi_flag set on full 4-way handshake
    wfi_flag_set = wfi_halt_ifu_req & wfi_halt_ifu_ack & wfi_halt_exu_req & wfi_halt_exu_ack;
    core_wfi = wfi_flag_r & ~wfi_flag_clr;
    // Ready signals
    longp_excp_i_ready = excpirq_flush_ack;
    // alu_excp_i_ready: complex priority
    if (alu_ebreakm_flush_req) begin
      alu_excp_i_ready = excpirq_flush_ack & oitf_empty & alu_excp_i_pc_vld & ~longp_need_flush;
    end else if (alu_need_flush) begin
      alu_excp_i_ready = excpirq_flush_ack & oitf_empty & ~irq_req & ~nonalu_dbg_entry_req & ~longp_need_flush;
    end else begin
      alu_excp_i_ready = ~irq_req & ~nonalu_dbg_entry_req & ~longp_need_flush;
    end
    // Flush target
    if (dbg_entry_flush_req) begin
      excpirq_flush_add_op1 = 32'd2048;
      excpirq_flush_add_op2 = 0;
      excpirq_flush_pc = 32'd2048;
    end else if (all_excp_flush_req & dbg_mode) begin
      excpirq_flush_add_op1 = 32'd2056;
      excpirq_flush_add_op2 = 0;
      excpirq_flush_pc = 32'd2056;
    end else begin
      excpirq_flush_add_op1 = csr_mtvec_r;
      excpirq_flush_add_op2 = 0;
      excpirq_flush_pc = csr_mtvec_r;
    end
    // IRQ cause
    irq_cause_w = {1'd1, 27'd0, sft_irq_r & msie_r ? 4'd3 : tmr_irq_r & mtie_r ? 4'd7 : ext_irq_r & meie_r ? 4'd11 : 4'd0};
    // Exception cause helpers
    excp_flush_by_alu_agu = alu_excp_flush_req & alu_excp_i_ld & alu_excp_i_misalgn | alu_excp_flush_req & alu_excp_i_ld & alu_excp_i_buserr | alu_excp_flush_req & alu_excp_i_stamo & alu_excp_i_misalgn | alu_excp_flush_req & alu_excp_i_stamo & alu_excp_i_buserr;
    excp_flush_by_longp_ldst = longp_excp_flush_req & longp_excp_i_ld & longp_excp_i_buserr | longp_excp_flush_req & longp_excp_i_st & longp_excp_i_buserr;
    // Exception cause encoding
    if (alu_excp_flush_req & alu_excp_i_ifu_misalgn) begin
      excp_cause_w = 0;
    end else if (alu_excp_flush_req & alu_excp_i_ifu_buserr) begin
      excp_cause_w = 1;
    end else if (alu_excp_flush_req & alu_excp_i_ifu_ilegl) begin
      excp_cause_w = 2;
    end else if (alu_excp_flush_req & alu_excp_i_ebreak4excp) begin
      excp_cause_w = 3;
    end else if (alu_excp_flush_req & alu_excp_i_ld & alu_excp_i_misalgn) begin
      excp_cause_w = 4;
    end else if (longp_excp_flush_req & longp_excp_i_ld & longp_excp_i_buserr | alu_excp_flush_req & alu_excp_i_ld & alu_excp_i_buserr) begin
      excp_cause_w = 5;
    end else if (alu_excp_flush_req & alu_excp_i_stamo & alu_excp_i_misalgn) begin
      excp_cause_w = 6;
    end else if (longp_excp_flush_req & longp_excp_i_st & longp_excp_i_buserr | alu_excp_flush_req & alu_excp_i_stamo & alu_excp_i_buserr) begin
      excp_cause_w = 7;
    end else if (alu_excp_flush_req & alu_excp_i_ecall & u_mode) begin
      excp_cause_w = 8;
    end else if (alu_excp_flush_req & alu_excp_i_ecall & s_mode) begin
      excp_cause_w = 9;
    end else if (alu_excp_flush_req & alu_excp_i_ecall & h_mode) begin
      excp_cause_w = 10;
    end else if (alu_excp_flush_req & alu_excp_i_ecall & m_mode) begin
      excp_cause_w = 11;
    end else if (longp_excp_flush_req & longp_excp_i_insterr) begin
      excp_cause_w = 16;
    end else begin
      excp_cause_w = 31;
    end
    // CSR updates
    cmt_cause = excp_taken_ena ? excp_cause_w : irq_cause_w;
    cmt_epc = longp_excp_i_valid ? longp_excp_i_pc : alu_excp_i_pc;
    // Badaddr
    if (excp_flush_by_longp_ldst) begin
      cmt_badaddr = longp_excp_i_badaddr;
    end else if (excp_flush_by_alu_agu) begin
      cmt_badaddr = alu_excp_i_badaddr;
    end else if (alu_excp_flush_req & alu_excp_i_ebreak4excp | alu_excp_flush_req & alu_excp_i_ifu_misalgn | alu_excp_flush_req & alu_excp_i_ifu_buserr) begin
      cmt_badaddr = alu_excp_i_pc;
    end else if (alu_excp_flush_req & alu_excp_i_ifu_ilegl) begin
      cmt_badaddr = alu_excp_i_instr;
    end else begin
      cmt_badaddr = 0;
    end
    cmt_epc_ena = ~dbg_mode & (excp_taken_ena | irq_taken_ena);
    cmt_cause_ena = cmt_epc_ena;
    cmt_status_ena = cmt_epc_ena;
    cmt_badaddr_ena = cmt_epc_ena & excpirq_flush_req;
    // Debug CSR updates
    cmt_dpc = alu_excp_i_pc;
    cmt_dpc_ena = dbg_entry_taken_ena;
    cmt_dcause = dbg_entry_taken_ena ? dbg_trig_req ? 2 : dbg_ebrk_req ? 1 : dbg_irq_req_w ? 3 : dbg_step_req ? 4 : dbg_halt_req ? 5 : 0 : 0;
    cmt_dcause_ena = dbg_entry_taken_ena | cmt_dret_ena;
  end

endmodule

