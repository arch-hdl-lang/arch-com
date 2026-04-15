module axi_register #(
  parameter int ADDR_WIDTH = 32,
  parameter int DATA_WIDTH = 32
) (
  input logic clk_i,
  input logic rst_n_i,
  input logic [ADDR_WIDTH-1:0] awaddr_i,
  input logic awvalid_i,
  output logic awready_o,
  input logic [DATA_WIDTH-1:0] wdata_i,
  input logic [DATA_WIDTH / 8-1:0] wstrb_i,
  input logic wvalid_i,
  output logic wready_o,
  output logic [1:0] bresp_o,
  output logic bvalid_o,
  input logic bready_i,
  input logic [ADDR_WIDTH-1:0] araddr_i,
  input logic arvalid_i,
  output logic arready_o,
  output logic [DATA_WIDTH-1:0] rdata_o,
  output logic rvalid_o,
  output logic [1:0] rresp_o,
  input logic rready_i,
  output logic [19:0] beat_o,
  output logic start_o,
  output logic writeback_o,
  input logic done_i
);

  // Write address channel
  // Write data channel
  // Write response channel
  // Read address channel
  // Read data channel
  // Control outputs
  // Internal registers
  logic done_reg;
  // All-ones strobe check
  logic strb_all;
  assign strb_all = wstrb_i == ~(DATA_WIDTH / 8)'(0);
  // All logic in one seq block
  always_ff @(posedge clk_i or negedge rst_n_i) begin
    if ((!rst_n_i)) begin
      arready_o <= 1'b0;
      awready_o <= 1'b0;
      beat_o <= 0;
      bresp_o <= 0;
      bvalid_o <= 1'b0;
      done_reg <= 1'b0;
      rdata_o <= 0;
      rresp_o <= 0;
      rvalid_o <= 1'b0;
      start_o <= 1'b0;
      wready_o <= 1'b0;
      writeback_o <= 1'b0;
    end else begin
      // Latch done_i
      if (done_i) begin
        done_reg <= 1'b1;
      end
      // Write defaults: one-cycle pulses
      awready_o <= 1'b0;
      wready_o <= 1'b0;
      // bvalid: clear on handshake
      if (bvalid_o & bready_i) begin
        bvalid_o <= 1'b0;
      end
      // Accept simultaneous AW+W
      if (awvalid_i & wvalid_i) begin
        awready_o <= 1'b1;
        wready_o <= 1'b1;
        bvalid_o <= 1'b1;
        // Decode address and update registers
        if (awaddr_i[11:8] == 1) begin
          if (strb_all) begin
            beat_o <= 20'(wdata_i);
          end
          bresp_o <= 0;
        end else if (awaddr_i[11:8] == 2) begin
          if (strb_all) begin
            start_o <= wdata_i[0:0] == 1;
          end
          bresp_o <= 0;
        end else if (awaddr_i[11:8] == 3) begin
          if (strb_all) begin
            if (wdata_i[0:0] == 1) begin
              done_reg <= 1'b0;
            end
          end
          bresp_o <= 0;
        end else if (awaddr_i[11:8] == 4) begin
          if (strb_all) begin
            writeback_o <= wdata_i[0:0] == 1;
          end
          bresp_o <= 0;
        end else if (awaddr_i[11:8] == 5) begin
          bresp_o <= 2;
        end else begin
          bresp_o <= 2;
        end
      end
      // Read defaults
      arready_o <= 1'b0;
      // Clear rvalid when master acknowledges
      if (rvalid_o & rready_i) begin
        rvalid_o <= 1'b0;
      end
      // Accept read when not already responding
      if (arvalid_i & ~rvalid_o) begin
        arready_o <= 1'b1;
        rvalid_o <= 1'b1;
        if (araddr_i[11:8] == 1) begin
          rdata_o <= DATA_WIDTH'($unsigned(beat_o));
          rresp_o <= 0;
        end else if (araddr_i[11:8] == 2) begin
          rdata_o <= DATA_WIDTH'($unsigned(start_o));
          rresp_o <= 0;
        end else if (araddr_i[11:8] == 3) begin
          rdata_o <= DATA_WIDTH'($unsigned(done_reg));
          rresp_o <= 0;
        end else if (araddr_i[11:8] == 4) begin
          rdata_o <= DATA_WIDTH'($unsigned(writeback_o));
          rresp_o <= 0;
        end else if (araddr_i[11:8] == 5) begin
          rdata_o <= DATA_WIDTH'($unsigned(65537));
          rresp_o <= 0;
        end else begin
          rdata_o <= 0;
          rresp_o <= 2;
        end
      end
    end
  end

endmodule

