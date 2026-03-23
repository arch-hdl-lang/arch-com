// E203 Interrupt Controller
// Manages external, software, and timer interrupts.
// Interfaces with CSR (mie/mip) and generates trap request to pipeline.
// Supports RISC-V machine-mode interrupts: MEI (11), MTI (7), MSI (3).
module IrqCtrl (
  input logic clk,
  input logic rst_n,
  input logic ext_irq_i,
  input logic sw_irq_i,
  input logic tmr_irq_i,
  input logic mstatus_mie,
  input logic mie_meie,
  input logic mie_mtie,
  input logic mie_msie,
  input logic pipe_flush_ack,
  input logic commit_valid,
  output logic irq_req,
  output logic [32-1:0] irq_cause,
  output logic mip_meip,
  output logic mip_mtip,
  output logic mip_msip
);

  // External interrupt sources
  // external interrupt (PLIC)
  // software interrupt
  // timer interrupt (CLINT)
  // CSR interface
  // global interrupt enable
  // external interrupt enable
  // timer interrupt enable
  // software interrupt enable
  // Pipeline interface
  // pipeline flushed, safe to trap
  // instruction committing (for precise traps)
  // Outputs
  // interrupt request to pipeline
  // mcause value (bit 31 = interrupt)
  // for CSR mip read
  // Interrupt pending (raw: source & enable)
  logic mei_pending;
  assign mei_pending = (ext_irq_i & mie_meie);
  logic mti_pending;
  assign mti_pending = (tmr_irq_i & mie_mtie);
  logic msi_pending;
  assign msi_pending = (sw_irq_i & mie_msie);
  // Any interrupt pending with global enable
  logic any_pending;
  assign any_pending = (((mei_pending | mti_pending) | msi_pending) & mstatus_mie);
  // Priority: MEI > MSI > MTI (per RISC-V privileged spec)
  logic sel_mei;
  assign sel_mei = mei_pending;
  logic sel_msi;
  assign sel_msi = (msi_pending & (~mei_pending));
  logic sel_mti;
  assign sel_mti = ((mti_pending & (~mei_pending)) & (~msi_pending));
  // mcause encoding: bit 31 = 1 (interrupt), bits[3:0] = cause code
  // MEI=11, MSI=3, MTI=7
  logic [32-1:0] cause_val;
  assign cause_val = (sel_mei) ? ('h8000000B) : ((sel_msi) ? ('h80000003) : ((sel_mti) ? ('h80000007) : (0)));
  assign irq_req = (any_pending & (pipe_flush_ack | commit_valid));
  assign irq_cause = cause_val;
  assign mip_meip = ext_irq_i;
  assign mip_mtip = tmr_irq_i;
  assign mip_msip = sw_irq_i;

endmodule

// Interrupt request: pending + pipeline ready to take trap
// mip bits for CSR read
