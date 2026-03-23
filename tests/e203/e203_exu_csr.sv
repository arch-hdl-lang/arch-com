// E203 CSR Register File
// Implements machine-mode CSRs for RV32IM:
//   mstatus, mie, mtvec, mepc, mcause, mtval, mip, mscratch
//   mcycle (low/high), minstret (low/high)
// CSR read/write via APB-like interface with 12-bit CSR address.
// domain SysDomain
//   freq_mhz: 100

module ExuCsr #(
  parameter int XLEN = 32
) (
  input logic clk,
  input logic rst_n,
  input logic [12-1:0] csr_addr,
  input logic csr_wen,
  input logic [32-1:0] csr_wdata,
  output logic [32-1:0] csr_rdata,
  input logic trap_taken,
  input logic [32-1:0] trap_cause,
  input logic [32-1:0] trap_pc,
  input logic [32-1:0] trap_val,
  input logic mret_taken,
  input logic ext_irq,
  input logic sw_irq,
  input logic tmr_irq,
  output logic [32-1:0] mtvec_val,
  output logic [32-1:0] mepc_val,
  output logic mstatus_mie,
  output logic irq_pending
);

  // CSR read/write interface
  // Trap entry/exit
  // PC of trapping instruction
  // mtval (fault addr / instr)
  // Interrupt pending inputs
  // external interrupt
  // software interrupt
  // timer interrupt
  // Outputs to pipeline
  // trap vector base
  // return address for MRET
  // global interrupt enable
  // any enabled interrupt pending
  // ── CSR address constants ───────────────────────────────────────
  // mstatus=0x300, mie=0x304, mtvec=0x305, mscratch=0x340
  // mepc=0x341, mcause=0x342, mtval=0x343, mip=0x344
  // mcyclel=0xB00, mcycleh=0xB80, minstretl=0xB02, minstreth=0xB82
  // ── CSR registers ──────────────────────────────────────────────
  logic [32-1:0] mstatus_r = 0;
  logic [32-1:0] mie_r = 0;
  logic [32-1:0] mtvec_r = 0;
  logic [32-1:0] mscratch_r = 0;
  logic [32-1:0] mepc_r = 0;
  logic [32-1:0] mcause_r = 0;
  logic [32-1:0] mtval_r = 0;
  logic [32-1:0] mcycle_lo_r = 0;
  logic [32-1:0] mcycle_hi_r = 0;
  logic [32-1:0] minstret_lo_r = 0;
  logic [32-1:0] minstret_hi_r = 0;
  // ── mip is read-only (directly reflects external signals) ──────
  logic [32-1:0] mip_val;
  assign mip_val = {{20{1'b0}}, ext_irq, {3{1'b0}}, tmr_irq, {3{1'b0}}, sw_irq, {3{1'b0}}};
  // bit 11 = MEIP, bit 7 = MTIP, bit 3 = MSIP
  // ── mstatus fields ─────────────────────────────────────────────
  logic mie_bit;
  assign mie_bit = (mstatus_r[3:3] != 0);
  logic mpie_bit;
  assign mpie_bit = (mstatus_r[7:7] != 0);
  // ── Interrupt pending check ────────────────────────────────────
  logic [32-1:0] irq_en;
  assign irq_en = (mie_r & mip_val);
  logic any_irq;
  assign any_irq = ((irq_en != 0) & mie_bit);
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      mcause_r <= 0;
      mcycle_hi_r <= 0;
      mcycle_lo_r <= 0;
      mepc_r <= 0;
      mie_r <= 0;
      minstret_hi_r <= 0;
      minstret_lo_r <= 0;
      mscratch_r <= 0;
      mstatus_r <= 0;
      mtval_r <= 0;
      mtvec_r <= 0;
    end else begin
      if ((mcycle_lo_r == 'hFFFFFFFF)) begin
        mcycle_lo_r <= 0;
        mcycle_hi_r <= 32'((mcycle_hi_r + 1));
      end else begin
        mcycle_lo_r <= 32'((mcycle_lo_r + 1));
      end
      if (trap_taken) begin
        mepc_r <= trap_pc;
        mcause_r <= trap_cause;
        mtval_r <= trap_val;
        mstatus_r <= {mstatus_r[31:8], mie_bit, mstatus_r[6:4], 1'b0, mstatus_r[2:0]};
      end else if (mret_taken) begin
        mstatus_r <= {mstatus_r[31:8], 1'b1, mstatus_r[6:4], mpie_bit, mstatus_r[2:0]};
      end else if (csr_wen) begin
        if ((csr_addr == 'h300)) begin
          mstatus_r <= csr_wdata;
        end else if ((csr_addr == 'h304)) begin
          mie_r <= csr_wdata;
        end else if ((csr_addr == 'h305)) begin
          mtvec_r <= csr_wdata;
        end else if ((csr_addr == 'h340)) begin
          mscratch_r <= csr_wdata;
        end else if ((csr_addr == 'h341)) begin
          mepc_r <= csr_wdata;
        end else if ((csr_addr == 'h342)) begin
          mcause_r <= csr_wdata;
        end else if ((csr_addr == 'h343)) begin
          mtval_r <= csr_wdata;
        end else if ((csr_addr == 'hB00)) begin
          mcycle_lo_r <= csr_wdata;
        end else if ((csr_addr == 'hB80)) begin
          mcycle_hi_r <= csr_wdata;
        end else if ((csr_addr == 'hB02)) begin
          minstret_lo_r <= csr_wdata;
        end else if ((csr_addr == 'hB82)) begin
          minstret_hi_r <= csr_wdata;
        end
      end
    end
  end
  // ── mcycle auto-increment ──────────────────────────────────
  // ── Trap entry: save context ───────────────────────────────
  // Set MPIE = MIE, clear MIE
  // Restore: MIE = MPIE, MPIE = 1
  // ── CSR write ────────────────────────────────────────────
  // ── CSR read mux ───────────────────────────────────────────────
  always_comb begin
    if ((csr_addr == 'h300)) begin
      csr_rdata = mstatus_r;
    end else if ((csr_addr == 'h304)) begin
      csr_rdata = mie_r;
    end else if ((csr_addr == 'h305)) begin
      csr_rdata = mtvec_r;
    end else if ((csr_addr == 'h340)) begin
      csr_rdata = mscratch_r;
    end else if ((csr_addr == 'h341)) begin
      csr_rdata = mepc_r;
    end else if ((csr_addr == 'h342)) begin
      csr_rdata = mcause_r;
    end else if ((csr_addr == 'h343)) begin
      csr_rdata = mtval_r;
    end else if ((csr_addr == 'h344)) begin
      csr_rdata = mip_val;
    end else if ((csr_addr == 'hB00)) begin
      csr_rdata = mcycle_lo_r;
    end else if ((csr_addr == 'hB80)) begin
      csr_rdata = mcycle_hi_r;
    end else if ((csr_addr == 'hB02)) begin
      csr_rdata = minstret_lo_r;
    end else if ((csr_addr == 'hB82)) begin
      csr_rdata = minstret_hi_r;
    end else begin
      csr_rdata = 0;
    end
    mtvec_val = mtvec_r;
    mepc_val = mepc_r;
    mstatus_mie = mie_bit;
    irq_pending = any_irq;
  end

endmodule

