// AXI4-Lite slave register interface — PG021 Simple DMA register map.
// Implements DMACR, DMASR (with W1C), SA/DA, LENGTH for both channels.
module AxiLiteRegs (
  input logic clk,
  input logic rst,
  input logic [8-1:0] awaddr_i,
  input logic awvalid_i,
  output logic awready_o,
  input logic [32-1:0] wdata_i,
  input logic [4-1:0] wstrb_i,
  input logic wvalid_i,
  output logic wready_o,
  output logic [2-1:0] bresp_o,
  output logic bvalid_o,
  input logic bready_i,
  input logic [8-1:0] araddr_i,
  input logic arvalid_i,
  output logic arready_o,
  output logic [32-1:0] rdata_o,
  output logic [2-1:0] rresp_o,
  output logic rvalid_o,
  input logic rready_i,
  output logic mm2s_start,
  output logic [32-1:0] mm2s_src_addr,
  output logic [8-1:0] mm2s_num_beats,
  input logic mm2s_done,
  input logic mm2s_halted,
  input logic mm2s_idle,
  output logic s2mm_start,
  output logic [32-1:0] s2mm_dst_addr,
  output logic [8-1:0] s2mm_num_beats,
  input logic s2mm_done,
  input logic s2mm_halted,
  input logic s2mm_idle,
  output logic mm2s_introut,
  output logic s2mm_introut
);

  // AXI4-Lite slave interface
  // MM2S control outputs
  // MM2S status inputs
  // S2MM control outputs
  // S2MM status inputs
  // Interrupt outputs
  // Internal register storage
  logic [32-1:0] mm2s_dmacr_r;
  logic [32-1:0] mm2s_sa_r;
  logic [32-1:0] mm2s_length_r;
  logic mm2s_ioc_irq;
  logic [32-1:0] s2mm_dmacr_r;
  logic [32-1:0] s2mm_da_r;
  logic [32-1:0] s2mm_length_r;
  logic s2mm_ioc_irq;
  always_ff @(posedge clk) begin
    if (rst) begin
      arready_o <= 1'b0;
      awready_o <= 1'b0;
      bresp_o <= 0;
      bvalid_o <= 1'b0;
      mm2s_dmacr_r <= 0;
      mm2s_introut <= 1'b0;
      mm2s_ioc_irq <= 1'b0;
      mm2s_length_r <= 0;
      mm2s_num_beats <= 0;
      mm2s_sa_r <= 0;
      mm2s_src_addr <= 0;
      mm2s_start <= 1'b0;
      rdata_o <= 0;
      rresp_o <= 0;
      rvalid_o <= 1'b0;
      s2mm_da_r <= 0;
      s2mm_dmacr_r <= 0;
      s2mm_dst_addr <= 0;
      s2mm_introut <= 1'b0;
      s2mm_ioc_irq <= 1'b0;
      s2mm_length_r <= 0;
      s2mm_num_beats <= 0;
      s2mm_start <= 1'b0;
      wready_o <= 1'b0;
    end else begin
      // Clear one-cycle pulses
      mm2s_start <= 1'b0;
      s2mm_start <= 1'b0;
      awready_o <= 1'b0;
      wready_o <= 1'b0;
      arready_o <= 1'b0;
      // B channel: clear bvalid on handshake
      if (bvalid_o & bready_i) begin
        bvalid_o <= 1'b0;
      end
      // R channel: clear rvalid on handshake
      if (rvalid_o & rready_i) begin
        rvalid_o <= 1'b0;
      end
      // Hardware-set IOC_Irq on done pulses
      if (mm2s_done) begin
        mm2s_ioc_irq <= 1'b1;
      end
      if (s2mm_done) begin
        s2mm_ioc_irq <= 1'b1;
      end
      // ── Write path: simultaneous AW+W ──────────────────────────────────
      if (awvalid_i & wvalid_i) begin
        awready_o <= 1'b1;
        wready_o <= 1'b1;
        bvalid_o <= 1'b1;
        bresp_o <= 0;
        // Address decode (byte offsets)
        if (awaddr_i == 'h0) begin
          // MM2S_DMACR
          mm2s_dmacr_r <= wdata_i;
        end else if (awaddr_i == 'h4) begin
          // MM2S_DMASR — W1C: writing 1 to bit 12 clears IOC_Irq
          if (wdata_i[12:12] == 1) begin
            mm2s_ioc_irq <= 1'b0;
          end
        end else if (awaddr_i == 'h18) begin
          // MM2S_SA
          mm2s_sa_r <= wdata_i;
        end else if (awaddr_i == 'h28) begin
          // MM2S_LENGTH — triggers start if RS=1
          mm2s_length_r <= wdata_i;
          if (mm2s_dmacr_r[0:0] == 1) begin
            mm2s_start <= 1'b1;
            mm2s_src_addr <= mm2s_sa_r;
            mm2s_num_beats <= 8'(wdata_i[25:2]);
          end
        end else if (awaddr_i == 'h30) begin
          // S2MM_DMACR
          s2mm_dmacr_r <= wdata_i;
        end else if (awaddr_i == 'h34) begin
          // S2MM_DMASR — W1C
          if (wdata_i[12:12] == 1) begin
            s2mm_ioc_irq <= 1'b0;
          end
        end else if (awaddr_i == 'h48) begin
          // S2MM_DA
          s2mm_da_r <= wdata_i;
        end else if (awaddr_i == 'h58) begin
          // S2MM_LENGTH — triggers start if RS=1
          s2mm_length_r <= wdata_i;
          if (s2mm_dmacr_r[0:0] == 1) begin
            s2mm_start <= 1'b1;
            s2mm_dst_addr <= s2mm_da_r;
            s2mm_num_beats <= 8'(wdata_i[25:2]);
          end
        end
      end
      // ── Read path ──────────────────────────────────────────────────────
      if (arvalid_i & ~rvalid_o) begin
        arready_o <= 1'b1;
        rvalid_o <= 1'b1;
        rresp_o <= 0;
        if (araddr_i == 'h0) begin
          rdata_o <= mm2s_dmacr_r;
        end else if (araddr_i == 'h4) begin
          // MM2S_DMASR: compose from status inputs + IOC_Irq
          rdata_o <= {19'd0, mm2s_ioc_irq, 10'd0, mm2s_idle, mm2s_halted};
        end else if (araddr_i == 'h18) begin
          rdata_o <= mm2s_sa_r;
        end else if (araddr_i == 'h28) begin
          rdata_o <= mm2s_length_r;
        end else if (araddr_i == 'h30) begin
          rdata_o <= s2mm_dmacr_r;
        end else if (araddr_i == 'h34) begin
          rdata_o <= {19'd0, s2mm_ioc_irq, 10'd0, s2mm_idle, s2mm_halted};
        end else if (araddr_i == 'h48) begin
          rdata_o <= s2mm_da_r;
        end else if (araddr_i == 'h58) begin
          rdata_o <= s2mm_length_r;
        end else begin
          rdata_o <= 0;
          rresp_o <= 2;
        end
      end
      // Interrupt outputs: IRQ asserted when both flag and enable are set
      mm2s_introut <= mm2s_ioc_irq & mm2s_dmacr_r[12:12] == 1;
      s2mm_introut <= s2mm_ioc_irq & s2mm_dmacr_r[12:12] == 1;
    end
  end

endmodule

