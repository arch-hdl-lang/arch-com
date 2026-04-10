// AXI4-Lite slave register interface — PG021 register map.
// Supports Simple DMA mode and Scatter-Gather mode.
module AxiLiteRegs (
  input logic clk,
  input logic rst,
  input logic axil_aw_valid,
  output logic axil_aw_ready,
  input logic [8-1:0] axil_aw_addr,
  input logic axil_w_valid,
  output logic axil_w_ready,
  input logic [32-1:0] axil_w_data,
  input logic [4-1:0] axil_w_strb,
  output logic axil_b_valid,
  input logic axil_b_ready,
  output logic [2-1:0] axil_b_resp,
  input logic axil_ar_valid,
  output logic axil_ar_ready,
  input logic [8-1:0] axil_ar_addr,
  output logic axil_r_valid,
  input logic axil_r_ready,
  output logic [32-1:0] axil_r_data,
  output logic [2-1:0] axil_r_resp,
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
  output logic mm2s_sg_start,
  output logic [32-1:0] mm2s_curdesc_o,
  output logic [32-1:0] mm2s_taildesc_o,
  input logic mm2s_sg_done,
  output logic s2mm_sg_start,
  output logic [32-1:0] s2mm_curdesc_o,
  output logic [32-1:0] s2mm_taildesc_o,
  input logic s2mm_sg_done,
  input logic mm2s_sg_active,
  input logic s2mm_sg_active,
  output logic mm2s_introut,
  output logic s2mm_introut
);

  // AXI4-Lite slave interface
  // MM2S direct control outputs (simple DMA mode)
  // MM2S status inputs
  // S2MM direct control outputs (simple DMA mode)
  // S2MM status inputs
  // Scatter-Gather control outputs
  // SG active flags
  // Interrupt outputs
  // Internal registers for AXI-Lite outputs (bus port can't be port reg)
  logic awready_r;
  logic wready_r;
  logic [2-1:0] bresp_r;
  logic bvalid_r;
  logic arready_r;
  logic [32-1:0] rdata_r;
  logic [2-1:0] rresp_r;
  logic rvalid_r;
  // DMA register storage
  logic [32-1:0] mm2s_dmacr_r;
  logic [32-1:0] mm2s_sa_r;
  logic [32-1:0] mm2s_length_r;
  logic mm2s_ioc_irq;
  logic [32-1:0] mm2s_curdesc_r;
  logic [32-1:0] mm2s_taildesc_r;
  logic [32-1:0] s2mm_dmacr_r;
  logic [32-1:0] s2mm_da_r;
  logic [32-1:0] s2mm_length_r;
  logic s2mm_ioc_irq;
  logic [32-1:0] s2mm_curdesc_r;
  logic [32-1:0] s2mm_taildesc_r;
  // Drive AXI-Lite bus outputs from registers
  assign axil_aw_ready = awready_r;
  assign axil_w_ready = wready_r;
  assign axil_b_resp = bresp_r;
  assign axil_b_valid = bvalid_r;
  assign axil_ar_ready = arready_r;
  assign axil_r_data = rdata_r;
  assign axil_r_resp = rresp_r;
  assign axil_r_valid = rvalid_r;
  always_ff @(posedge clk) begin
    if (rst) begin
      arready_r <= 1'b0;
      awready_r <= 1'b0;
      bresp_r <= 0;
      bvalid_r <= 1'b0;
      mm2s_curdesc_o <= 0;
      mm2s_curdesc_r <= 0;
      mm2s_dmacr_r <= 0;
      mm2s_introut <= 1'b0;
      mm2s_ioc_irq <= 1'b0;
      mm2s_length_r <= 0;
      mm2s_num_beats <= 0;
      mm2s_sa_r <= 0;
      mm2s_sg_start <= 1'b0;
      mm2s_src_addr <= 0;
      mm2s_start <= 1'b0;
      mm2s_taildesc_o <= 0;
      mm2s_taildesc_r <= 0;
      rdata_r <= 0;
      rresp_r <= 0;
      rvalid_r <= 1'b0;
      s2mm_curdesc_o <= 0;
      s2mm_curdesc_r <= 0;
      s2mm_da_r <= 0;
      s2mm_dmacr_r <= 0;
      s2mm_dst_addr <= 0;
      s2mm_introut <= 1'b0;
      s2mm_ioc_irq <= 1'b0;
      s2mm_length_r <= 0;
      s2mm_num_beats <= 0;
      s2mm_sg_start <= 1'b0;
      s2mm_start <= 1'b0;
      s2mm_taildesc_o <= 0;
      s2mm_taildesc_r <= 0;
      wready_r <= 1'b0;
    end else begin
      // Clear one-cycle pulses
      mm2s_start <= 1'b0;
      s2mm_start <= 1'b0;
      mm2s_sg_start <= 1'b0;
      s2mm_sg_start <= 1'b0;
      awready_r <= 1'b0;
      wready_r <= 1'b0;
      arready_r <= 1'b0;
      // B channel: clear bvalid on handshake
      if (bvalid_r & axil_b_ready) begin
        bvalid_r <= 1'b0;
      end
      // R channel: clear rvalid on handshake
      if (rvalid_r & axil_r_ready) begin
        rvalid_r <= 1'b0;
      end
      // Hardware-set IOC_Irq
      if (mm2s_sg_active) begin
        if (mm2s_sg_done) begin
          mm2s_ioc_irq <= 1'b1;
        end
      end else if (mm2s_done) begin
        mm2s_ioc_irq <= 1'b1;
      end
      if (s2mm_sg_active) begin
        if (s2mm_sg_done) begin
          s2mm_ioc_irq <= 1'b1;
        end
      end else if (s2mm_done) begin
        s2mm_ioc_irq <= 1'b1;
      end
      // ── Write path: simultaneous AW+W ──────────────────────────────────
      if (axil_aw_valid & axil_w_valid) begin
        awready_r <= 1'b1;
        wready_r <= 1'b1;
        bvalid_r <= 1'b1;
        bresp_r <= 0;
        if (axil_aw_addr == 'h0) begin
          mm2s_dmacr_r <= axil_w_data;
        end else if (axil_aw_addr == 'h4) begin
          if (axil_w_data[12:12] == 1) begin
            mm2s_ioc_irq <= 1'b0;
          end
        end else if (axil_aw_addr == 'h8) begin
          mm2s_curdesc_r <= axil_w_data;
          mm2s_curdesc_o <= axil_w_data;
        end else if (axil_aw_addr == 'h10) begin
          mm2s_taildesc_r <= axil_w_data;
          mm2s_taildesc_o <= axil_w_data;
          if (mm2s_dmacr_r[0:0] == 1) begin
            mm2s_sg_start <= 1'b1;
            mm2s_curdesc_o <= mm2s_curdesc_r;
          end
        end else if (axil_aw_addr == 'h18) begin
          mm2s_sa_r <= axil_w_data;
        end else if (axil_aw_addr == 'h28) begin
          mm2s_length_r <= axil_w_data;
          if (mm2s_dmacr_r[0:0] == 1) begin
            mm2s_start <= 1'b1;
            mm2s_src_addr <= mm2s_sa_r;
            mm2s_num_beats <= 8'(axil_w_data[25:2]);
          end
        end else if (axil_aw_addr == 'h30) begin
          s2mm_dmacr_r <= axil_w_data;
        end else if (axil_aw_addr == 'h34) begin
          if (axil_w_data[12:12] == 1) begin
            s2mm_ioc_irq <= 1'b0;
          end
        end else if (axil_aw_addr == 'h38) begin
          s2mm_curdesc_r <= axil_w_data;
          s2mm_curdesc_o <= axil_w_data;
        end else if (axil_aw_addr == 'h40) begin
          s2mm_taildesc_r <= axil_w_data;
          s2mm_taildesc_o <= axil_w_data;
          if (s2mm_dmacr_r[0:0] == 1) begin
            s2mm_sg_start <= 1'b1;
            s2mm_curdesc_o <= s2mm_curdesc_r;
          end
        end else if (axil_aw_addr == 'h48) begin
          s2mm_da_r <= axil_w_data;
        end else if (axil_aw_addr == 'h58) begin
          s2mm_length_r <= axil_w_data;
          if (s2mm_dmacr_r[0:0] == 1) begin
            s2mm_start <= 1'b1;
            s2mm_dst_addr <= s2mm_da_r;
            s2mm_num_beats <= 8'(axil_w_data[25:2]);
          end
        end
      end
      // ── Read path ──────────────────────────────────────────────────────
      if (axil_ar_valid & ~rvalid_r) begin
        arready_r <= 1'b1;
        rvalid_r <= 1'b1;
        rresp_r <= 0;
        if (axil_ar_addr == 'h0) begin
          rdata_r <= mm2s_dmacr_r;
        end else if (axil_ar_addr == 'h4) begin
          rdata_r <= {19'd0, mm2s_ioc_irq, 10'd0, mm2s_idle, mm2s_halted};
        end else if (axil_ar_addr == 'h8) begin
          rdata_r <= mm2s_curdesc_r;
        end else if (axil_ar_addr == 'h10) begin
          rdata_r <= mm2s_taildesc_r;
        end else if (axil_ar_addr == 'h18) begin
          rdata_r <= mm2s_sa_r;
        end else if (axil_ar_addr == 'h28) begin
          rdata_r <= mm2s_length_r;
        end else if (axil_ar_addr == 'h30) begin
          rdata_r <= s2mm_dmacr_r;
        end else if (axil_ar_addr == 'h34) begin
          rdata_r <= {19'd0, s2mm_ioc_irq, 10'd0, s2mm_idle, s2mm_halted};
        end else if (axil_ar_addr == 'h38) begin
          rdata_r <= s2mm_curdesc_r;
        end else if (axil_ar_addr == 'h40) begin
          rdata_r <= s2mm_taildesc_r;
        end else if (axil_ar_addr == 'h48) begin
          rdata_r <= s2mm_da_r;
        end else if (axil_ar_addr == 'h58) begin
          rdata_r <= s2mm_length_r;
        end else begin
          rdata_r <= 0;
          rresp_r <= 2;
        end
      end
      // Interrupt outputs
      mm2s_introut <= mm2s_ioc_irq & mm2s_dmacr_r[12:12] == 1;
      s2mm_introut <= s2mm_ioc_irq & s2mm_dmacr_r[12:12] == 1;
    end
  end

endmodule

