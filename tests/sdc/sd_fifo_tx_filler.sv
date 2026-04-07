// SD FIFO TX Filler
// DMA: reads data from system memory via Wishbone master, fills TX FIFO.
module sd_fifo_tx_filler (
  input logic clk,
  input logic rst,
  output logic [32-1:0] m_wb_adr_o,
  output logic m_wb_we_o,
  input logic [32-1:0] m_wb_dat_i,
  output logic m_wb_cyc_o,
  output logic m_wb_stb_o,
  input logic m_wb_ack_i,
  output logic [3-1:0] m_wb_cti_o,
  output logic [2-1:0] m_wb_bte_o,
  input logic en,
  input logic [32-1:0] adr,
  input logic sd_clk,
  output logic [32-1:0] dat_o,
  input logic rd,
  output logic empty,
  output logic fe
);

  logic [32-1:0] offset_r;
  logic [32-1:0] din_r;
  logic wr_tx_r;
  logic delay_r;
  logic ackd_r;
  logic cyc_r;
  logic stb_r;
  logic fe_w;
  logic empty_w;
  logic [6-1:0] mem_empt_w;
  sd_tx_fifo u_fifo (
    .wclk(clk),
    .rclk(sd_clk),
    .rst(rst | ~en),
    .d(din_r),
    .wr(wr_tx_r),
    .q(dat_o),
    .rd(rd),
    .full(fe_w),
    .empty(empty_w),
    .mem_empt(mem_empt_w)
  );
  always_ff @(posedge clk or posedge rst) begin
    if (rst) begin
      ackd_r <= 1'b1;
      cyc_r <= 1'b0;
      delay_r <= 1'b0;
      din_r <= 0;
      offset_r <= 0;
      stb_r <= 1'b0;
      wr_tx_r <= 1'b0;
    end else begin
      if (~en) begin
        offset_r <= 0;
        wr_tx_r <= 1'b0;
        delay_r <= 1'b0;
        ackd_r <= 1'b1;
        cyc_r <= 1'b0;
        stb_r <= 1'b0;
      end else begin
        // Default: clear write pulse
        wr_tx_r <= 1'b0;
        if (delay_r) begin
          // Delay cycle: increment offset, toggle ackd
          delay_r <= 1'b0;
          offset_r <= 32'(offset_r + 4);
          ackd_r <= ~ackd_r;
        end else if (m_wb_ack_i) begin
          // WB ack: capture data, deassert bus
          din_r <= m_wb_dat_i;
          wr_tx_r <= 1'b1;
          cyc_r <= 1'b0;
          stb_r <= 1'b0;
          delay_r <= 1'b1;
        end else if (~fe_w & ~m_wb_ack_i & ackd_r) begin
          // Start WB read: FIFO not full, no ack pending, previous done
          cyc_r <= 1'b1;
          stb_r <= 1'b1;
        end
      end
    end
  end
  assign m_wb_adr_o = 32'(adr + offset_r);
  assign m_wb_we_o = 1'b0;
  assign m_wb_cyc_o = cyc_r;
  assign m_wb_stb_o = stb_r;
  assign m_wb_cti_o = 0;
  assign m_wb_bte_o = 0;
  assign fe = fe_w;
  assign empty = empty_w;

endmodule

