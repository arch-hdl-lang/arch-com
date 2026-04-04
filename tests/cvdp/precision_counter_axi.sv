module precision_counter_axi #(
  parameter int C_S_AXI_DATA_WIDTH = 32,
  parameter int C_S_AXI_ADDR_WIDTH = 8
) (
  input logic axi_aclk,
  input logic axi_aresetn,
  input logic [C_S_AXI_ADDR_WIDTH-1:0] axi_awaddr,
  input logic axi_awvalid,
  output logic axi_awready,
  input logic [C_S_AXI_DATA_WIDTH-1:0] axi_wdata,
  input logic [C_S_AXI_DATA_WIDTH / 8-1:0] axi_wstrb,
  input logic axi_wvalid,
  output logic axi_wready,
  output logic [2-1:0] axi_bresp,
  output logic axi_bvalid,
  input logic axi_bready,
  input logic [C_S_AXI_ADDR_WIDTH-1:0] axi_araddr,
  input logic axi_arvalid,
  output logic axi_arready,
  output logic [C_S_AXI_DATA_WIDTH-1:0] axi_rdata,
  output logic [2-1:0] axi_rresp,
  output logic axi_rvalid,
  input logic axi_rready,
  output logic axi_ap_done,
  output logic irq
);

  logic [C_S_AXI_DATA_WIDTH-1:0] slv_reg_ctl;
  logic [C_S_AXI_DATA_WIDTH-1:0] slv_reg_t;
  logic [C_S_AXI_DATA_WIDTH-1:0] slv_reg_v;
  logic [C_S_AXI_DATA_WIDTH-1:0] slv_reg_irq_mask;
  logic [C_S_AXI_DATA_WIDTH-1:0] slv_reg_irq_thresh;
  logic aw_en;
  logic [C_S_AXI_ADDR_WIDTH-1:0] aw_addr_latched;
  logic [C_S_AXI_ADDR_WIDTH-1:0] ar_addr_latched;
  always_ff @(posedge axi_aclk or negedge axi_aresetn) begin
    if ((!axi_aresetn)) begin
      ar_addr_latched <= 0;
      aw_addr_latched <= 0;
      aw_en <= 1'b1;
      axi_ap_done <= 1'b0;
      axi_arready <= 1'b0;
      axi_awready <= 1'b0;
      axi_bresp <= 0;
      axi_bvalid <= 1'b0;
      axi_rdata <= 0;
      axi_rresp <= 0;
      axi_rvalid <= 1'b0;
      axi_wready <= 1'b0;
      irq <= 1'b0;
      slv_reg_ctl <= 0;
      slv_reg_irq_mask <= 0;
      slv_reg_irq_thresh <= 0;
      slv_reg_t <= 0;
      slv_reg_v <= 0;
    end else begin
      // === Write address/data handshake ===
      if (~axi_awready & axi_awvalid & axi_wvalid & aw_en) begin
        axi_awready <= 1'b1;
        axi_wready <= 1'b1;
        aw_en <= 1'b0;
        aw_addr_latched <= axi_awaddr;
      end else begin
        axi_awready <= 1'b0;
        axi_wready <= 1'b0;
        if (axi_bvalid & axi_bready) begin
          aw_en <= 1'b1;
        end
      end
      // === Write response ===
      if (axi_awready & axi_wready & axi_awvalid & axi_wvalid & ~axi_bvalid) begin
        axi_bvalid <= 1'b1;
        if (aw_addr_latched == 'h0) begin
          axi_bresp <= 'b0;
        end else if (aw_addr_latched == 'h10) begin
          axi_bresp <= 'b0;
        end else if (aw_addr_latched == 'h20) begin
          axi_bresp <= 'b0;
        end else if (aw_addr_latched == 'h24) begin
          axi_bresp <= 'b0;
        end else if (aw_addr_latched == 'h28) begin
          axi_bresp <= 'b0;
        end else begin
          axi_bresp <= 'b10;
        end
      end else if (axi_bvalid & axi_bready) begin
        axi_bvalid <= 1'b0;
      end
      // === Register writes + counter ===
      // Gate counter during write setup (~awready & awvalid & wvalid & aw_en)
      if (axi_awready & axi_wready & axi_awvalid & axi_wvalid) begin
        if (aw_addr_latched == 'h0) begin
          slv_reg_ctl <= axi_wdata;
          slv_reg_t <= 0;
        end else if (aw_addr_latched == 'h10) begin
          slv_reg_t <= axi_wdata;
        end else if (aw_addr_latched == 'h20) begin
          slv_reg_v <= axi_wdata;
        end else if (aw_addr_latched == 'h24) begin
          slv_reg_irq_mask <= axi_wdata;
        end else if (aw_addr_latched == 'h28) begin
          slv_reg_irq_thresh <= axi_wdata;
        end
      end else if (~axi_awready & axi_awvalid & axi_wvalid & aw_en) begin
      end else if (axi_arvalid & ~axi_rvalid) begin
      end else if (slv_reg_ctl[0:0] == 1) begin
        // Write setup cycle - don't run counter
        // Read setup cycles - don't run counter
        if (slv_reg_v != 0) begin
          slv_reg_v <= C_S_AXI_DATA_WIDTH'(slv_reg_v - 1);
        end else begin
          slv_reg_t <= C_S_AXI_DATA_WIDTH'(slv_reg_t + 1);
        end
      end
      // === ap_done ===
      if (slv_reg_v == 0) begin
        axi_ap_done <= 1'b1;
      end else begin
        axi_ap_done <= 1'b0;
      end
      // === IRQ ===
      if (slv_reg_irq_mask[0:0] == 1 & slv_reg_ctl[0:0] == 1 & slv_reg_v == slv_reg_irq_thresh) begin
        irq <= 1'b1;
      end else begin
        irq <= 1'b0;
      end
      // === Read address handshake (cycle 1) ===
      if (~axi_arready & axi_arvalid & ~axi_rvalid) begin
        axi_arready <= 1'b1;
        ar_addr_latched <= axi_araddr;
      end else begin
        axi_arready <= 1'b0;
      end
      // === Read data response (cycle 2: one cycle after arready) ===
      if (axi_arready & axi_arvalid & ~axi_rvalid) begin
        axi_rvalid <= 1'b1;
        if (ar_addr_latched == 'h0) begin
          axi_rdata <= slv_reg_ctl;
          axi_rresp <= 'b0;
        end else if (ar_addr_latched == 'h4) begin
          axi_rdata <= 0;
          axi_rresp <= 'b0;
        end else if (ar_addr_latched == 'h8) begin
          axi_rdata <= 0;
          axi_rresp <= 'b0;
        end else if (ar_addr_latched == 'hC) begin
          if (slv_reg_v == 0) begin
            axi_rdata <= 1;
          end else begin
            axi_rdata <= 0;
          end
          axi_rresp <= 'b0;
        end else if (ar_addr_latched == 'h10) begin
          axi_rdata <= slv_reg_t;
          axi_rresp <= 'b0;
        end else if (ar_addr_latched == 'h20) begin
          axi_rdata <= slv_reg_v;
          axi_rresp <= 'b0;
        end else if (ar_addr_latched == 'h24) begin
          axi_rdata <= slv_reg_irq_mask;
          axi_rresp <= 'b0;
        end else if (ar_addr_latched == 'h28) begin
          axi_rdata <= slv_reg_irq_thresh;
          axi_rresp <= 'b0;
        end else begin
          axi_rdata <= 0;
          axi_rresp <= 'b10;
        end
      end else if (axi_rvalid & axi_rready & ~axi_arvalid) begin
        axi_rvalid <= 1'b0;
      end
    end
  end

endmodule

