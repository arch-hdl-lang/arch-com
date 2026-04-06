// SD FIFO TX Filler
// Reads data from system memory via Wishbone master and writes to TX FIFO.
// Internally instantiates sd_tx_fifo. Port interface matches OpenCores SDC reference.
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

  // Wishbone master
  // Data master control
  // Data serial signals (directly to TX FIFO read side)
  // Internal signals
  logic reset_tx_fifo_r;
  logic [32-1:0] din_r;
  logic wr_tx_r;
  logic [9-1:0] we_r;
  logic [9-1:0] offset_r;
  logic first_r;
  logic ackd_r;
  logic delay_r;
  logic [6-1:0] mem_empt_w;
  logic fe_w;
  logic empty_w;
  // Internal TX FIFO instance
  sd_tx_fifo u_fifo (
    .d(din_r),
    .wr(wr_tx_r),
    .wclk(clk),
    .q(dat_o),
    .rd(rd),
    .full(fe_w),
    .empty(empty_w),
    .mem_empt(mem_empt_w),
    .rclk(sd_clk),
    .rst(rst)
  );
  logic m_wb_cyc_r;
  logic m_wb_stb_r;
  logic [3-1:0] m_wb_cti_r;
  logic [2-1:0] m_wb_bte_r;
  always_ff @(posedge clk or posedge rst) begin
    if (rst) begin
      ackd_r <= 1'b0;
      din_r <= 0;
      first_r <= 1'b0;
      m_wb_cyc_r <= 1'b0;
      m_wb_stb_r <= 1'b0;
      offset_r <= 0;
      reset_tx_fifo_r <= 1'b0;
      we_r <= 0;
      wr_tx_r <= 1'b0;
    end else begin
      wr_tx_r <= 1'b0;
      ackd_r <= 1'b0;
      if (en) begin
        // When enabled, read from WB and fill FIFO
        if (~first_r) begin
          // Initialize: set up for burst read
          first_r <= 1'b1;
          offset_r <= 0;
          we_r <= 0;
          reset_tx_fifo_r <= 1'b1;
        end else begin
          reset_tx_fifo_r <= 1'b0;
          if (~fe_w) begin
            // FIFO not full, do WB read
            m_wb_cyc_r <= 1'b1;
            m_wb_stb_r <= 1'b1;
            if (m_wb_ack_i) begin
              din_r <= m_wb_dat_i;
              wr_tx_r <= 1'b1;
              offset_r <= 9'(offset_r + 1);
              ackd_r <= 1'b1;
            end
          end else begin
            m_wb_cyc_r <= 1'b0;
            m_wb_stb_r <= 1'b0;
          end
        end
      end else begin
        first_r <= 1'b0;
        m_wb_cyc_r <= 1'b0;
        m_wb_stb_r <= 1'b0;
        offset_r <= 0;
      end
    end
  end
  assign m_wb_adr_o = 32'(adr + (32'($unsigned(offset_r)) << 2));
  assign m_wb_we_o = 1'b0;
  assign m_wb_cyc_o = m_wb_cyc_r;
  assign m_wb_stb_o = m_wb_stb_r;
  assign m_wb_cti_o = 0;
  assign m_wb_bte_o = 0;
  assign fe = fe_w;
  assign empty = empty_w;

endmodule

