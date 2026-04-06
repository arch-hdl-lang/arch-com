// E203 HBirdv2 CSR Register File
// Machine-mode CSRs for RV32IM with debug support.
// Matches RealBench port interface.
module e203_exu_csr #(
  parameter int XLEN = 32
) (
  input logic clk,
  input logic rst_n,
  input logic clk_aon,
  input logic nonflush_cmt_ena,
  input logic csr_ena,
  input logic csr_wr_en,
  input logic csr_rd_en,
  input logic [12-1:0] csr_idx,
  output logic csr_access_ilgl,
  output logic [32-1:0] read_csr_dat,
  input logic [32-1:0] wbck_csr_dat,
  output logic nice_xs_off,
  output logic tm_stop,
  output logic core_cgstop,
  output logic tcm_cgstop,
  output logic itcm_nohold,
  output logic mdv_nob2b,
  input logic core_mhartid,
  input logic ext_irq_r,
  input logic sft_irq_r,
  input logic tmr_irq_r,
  output logic status_mie_r,
  output logic mtie_r,
  output logic msie_r,
  output logic meie_r,
  output logic wr_dcsr_ena,
  output logic wr_dpc_ena,
  output logic wr_dscratch_ena,
  input logic [32-1:0] dcsr_r,
  input logic [32-1:0] dpc_r,
  input logic [32-1:0] dscratch_r,
  output logic [32-1:0] wr_csr_nxt,
  input logic dbg_mode,
  input logic dbg_stopcycle,
  output logic u_mode,
  output logic s_mode,
  output logic h_mode,
  output logic m_mode,
  input logic [32-1:0] cmt_badaddr,
  input logic cmt_badaddr_ena,
  input logic [32-1:0] cmt_epc,
  input logic cmt_epc_ena,
  input logic [32-1:0] cmt_cause,
  input logic cmt_cause_ena,
  input logic cmt_status_ena,
  input logic cmt_instret_ena,
  input logic cmt_mret_ena,
  output logic [32-1:0] csr_epc_r,
  output logic [32-1:0] csr_dpc_r,
  output logic [32-1:0] csr_mtvec_r
);

  // ── CSR access interface ──────────────────────────────────────────
  // ── Control outputs ───────────────────────────────────────────────
  // ── Hart ID ───────────────────────────────────────────────────────
  // ── Interrupt status outputs ──────────────────────────────────────
  // ── Debug CSR interface ───────────────────────────────────────────
  // ── Debug mode ────────────────────────────────────────────────────
  // ── Privilege mode outputs ────────────────────────────────────────
  // ── Commit inputs ─────────────────────────────────────────────────
  // ── CSR vector outputs ────────────────────────────────────────────
  // ── CSR registers ─────────────────────────────────────────────────
  logic [32-1:0] mstatus_r = 0;
  logic [32-1:0] mie_r_reg = 0;
  logic [32-1:0] mtvec_r_reg = 0;
  logic [32-1:0] mscratch_r = 0;
  logic [32-1:0] mepc_r = 0;
  logic [32-1:0] mcause_r = 0;
  logic [32-1:0] mtval_r = 0;
  logic [32-1:0] mcycle_lo_r = 0;
  logic [32-1:0] mcycle_hi_r = 0;
  logic [32-1:0] minstret_lo_r = 0;
  logic [32-1:0] minstret_hi_r = 0;
  // mip is read-only
  logic [32-1:0] mip_val;
  assign mip_val = {{20{1'b0}}, ext_irq_r, {3{1'b0}}, tmr_irq_r, {3{1'b0}}, sft_irq_r, {3{1'b0}}};
  // mstatus fields
  logic mie_bit;
  assign mie_bit = mstatus_r[3:3] != 0;
  logic mpie_bit;
  assign mpie_bit = mstatus_r[7:7] != 0;
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      mcause_r <= 0;
      mcycle_hi_r <= 0;
      mcycle_lo_r <= 0;
      mepc_r <= 0;
      mie_r_reg <= 0;
      minstret_hi_r <= 0;
      minstret_lo_r <= 0;
      mscratch_r <= 0;
      mstatus_r <= 0;
      mtval_r <= 0;
      mtvec_r_reg <= 0;
    end else begin
      // mcycle auto-increment
      if (mcycle_lo_r == 'hFFFFFFFF) begin
        mcycle_lo_r <= 0;
        mcycle_hi_r <= 32'(mcycle_hi_r + 1);
      end else begin
        mcycle_lo_r <= 32'(mcycle_lo_r + 1);
      end
      // minstret increment on commit
      if (cmt_instret_ena) begin
        if (minstret_lo_r == 'hFFFFFFFF) begin
          minstret_lo_r <= 0;
          minstret_hi_r <= 32'(minstret_hi_r + 1);
        end else begin
          minstret_lo_r <= 32'(minstret_lo_r + 1);
        end
      end
      // Trap entry
      if (cmt_epc_ena) begin
        mepc_r <= cmt_epc;
      end
      if (cmt_cause_ena) begin
        mcause_r <= cmt_cause;
      end
      if (cmt_badaddr_ena) begin
        mtval_r <= cmt_badaddr;
      end
      if (cmt_status_ena & ~cmt_mret_ena) begin
        // Save MPIE = MIE, clear MIE
        mstatus_r <= {mstatus_r[31:8], mie_bit, mstatus_r[6:4], 1'b0, mstatus_r[2:0]};
      end else if (cmt_mret_ena) begin
        // Restore MIE = MPIE, MPIE = 1
        mstatus_r <= {mstatus_r[31:8], 1'b1, mstatus_r[6:4], mpie_bit, mstatus_r[2:0]};
      end else if (csr_ena & csr_wr_en & csr_idx == 'h300) begin
        mstatus_r <= wbck_csr_dat;
      end
      // CSR writes (non-mstatus)
      if (csr_ena & csr_wr_en) begin
        if (csr_idx == 'h304) begin
          mie_r_reg <= wbck_csr_dat;
        end else if (csr_idx == 'h305) begin
          mtvec_r_reg <= wbck_csr_dat;
        end else if (csr_idx == 'h340) begin
          mscratch_r <= wbck_csr_dat;
        end else if (csr_idx == 'h341) begin
          mepc_r <= wbck_csr_dat;
        end else if (csr_idx == 'h342) begin
          mcause_r <= wbck_csr_dat;
        end else if (csr_idx == 'h343) begin
          mtval_r <= wbck_csr_dat;
        end else if (csr_idx == 'hB00) begin
          mcycle_lo_r <= wbck_csr_dat;
        end else if (csr_idx == 'hB80) begin
          mcycle_hi_r <= wbck_csr_dat;
        end else if (csr_idx == 'hB02) begin
          minstret_lo_r <= wbck_csr_dat;
        end else if (csr_idx == 'hB82) begin
          minstret_hi_r <= wbck_csr_dat;
        end
      end
    end
  end
  always_comb begin
    // CSR read mux
    if (csr_idx == 'h300) begin
      read_csr_dat = mstatus_r;
    end else if (csr_idx == 'h304) begin
      read_csr_dat = mie_r_reg;
    end else if (csr_idx == 'h305) begin
      read_csr_dat = mtvec_r_reg;
    end else if (csr_idx == 'h340) begin
      read_csr_dat = mscratch_r;
    end else if (csr_idx == 'h341) begin
      read_csr_dat = mepc_r;
    end else if (csr_idx == 'h342) begin
      read_csr_dat = mcause_r;
    end else if (csr_idx == 'h343) begin
      read_csr_dat = mtval_r;
    end else if (csr_idx == 'h344) begin
      read_csr_dat = mip_val;
    end else if (csr_idx == 'hB00) begin
      read_csr_dat = mcycle_lo_r;
    end else if (csr_idx == 'hB80) begin
      read_csr_dat = mcycle_hi_r;
    end else if (csr_idx == 'hB02) begin
      read_csr_dat = minstret_lo_r;
    end else if (csr_idx == 'hB82) begin
      read_csr_dat = minstret_hi_r;
    end else if (csr_idx == 'hF11) begin
      read_csr_dat = 0;
    end else if (csr_idx == 'hF14) begin
      read_csr_dat = 32'($unsigned(core_mhartid));
    end else if (csr_idx == 'h7B0) begin
      read_csr_dat = dcsr_r;
    end else if (csr_idx == 'h7B1) begin
      read_csr_dat = dpc_r;
    end else if (csr_idx == 'h7B2) begin
      read_csr_dat = dscratch_r;
    end else begin
      read_csr_dat = 0;
    end
    // CSR output values
    csr_epc_r = mepc_r;
    csr_dpc_r = dpc_r;
    csr_mtvec_r = mtvec_r_reg;
    // Interrupt enables from mie register
    status_mie_r = mie_bit;
    meie_r = mie_r_reg[11:11] != 0;
    mtie_r = mie_r_reg[7:7] != 0;
    msie_r = mie_r_reg[3:3] != 0;
    // Control outputs (simplified)
    nice_xs_off = 1'b0;
    tm_stop = dbg_stopcycle;
    core_cgstop = dbg_stopcycle;
    tcm_cgstop = dbg_stopcycle;
    itcm_nohold = 1'b0;
    mdv_nob2b = 1'b0;
    // Privilege mode (machine mode only in E203)
    m_mode = 1'b1;
    s_mode = 1'b0;
    h_mode = 1'b0;
    u_mode = 1'b0;
    // CSR access illegal check (simplified)
    csr_access_ilgl = 1'b0;
    // Debug CSR write enables
    wr_dcsr_ena = csr_ena & csr_wr_en & csr_idx == 'h7B0;
    wr_dpc_ena = csr_ena & csr_wr_en & csr_idx == 'h7B1;
    wr_dscratch_ena = csr_ena & csr_wr_en & csr_idx == 'h7B2;
    wr_csr_nxt = wbck_csr_dat;
  end

endmodule

