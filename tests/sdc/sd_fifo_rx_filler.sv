// SD FIFO RX Filler
// Reads data from RX FIFO and writes to system memory via Wishbone master.
// Internally instantiates sd_rx_fifo. Port interface matches OpenCores SDC reference.
module sd_fifo_rx_filler (
  input logic clk,
  input logic rst,
  output logic [32-1:0] m_wb_adr_o,
  output logic m_wb_we_o,
  output logic [32-1:0] m_wb_dat_o,
  output logic m_wb_cyc_o,
  output logic m_wb_stb_o,
  input logic m_wb_ack_i,
  output logic [3-1:0] m_wb_cti_o,
  output logic [2-1:0] m_wb_bte_o,
  input logic en,
  input logic [32-1:0] adr,
  input logic sd_clk,
  input logic [4-1:0] dat_i,
  input logic wr,
  output logic full,
  output logic empty
);

  // Wishbone master
  // Data master control
  // Data serial signals (directly to RX FIFO write side)
  // Internal RX FIFO
  logic [32-1:0] rx_dat_out;
  logic rx_full_w;
  logic rx_empty_w;
  logic [2-1:0] rx_mem_w;
  logic rd_int_r;
  logic reset_rx_fifo_r;
  sd_rx_fifo u_fifo (
    .d(dat_i),
    .wr(wr),
    .wclk(sd_clk),
    .q(rx_dat_out),
    .rd(rd_int_r),
    .full(rx_full_w),
    .empty(rx_empty_w),
    .mem_empt(rx_mem_w),
    .rclk(clk),
    .rst(rst)
  );
  // WB master state machine: read from FIFO, write to system memory
  logic [2-1:0] st_r;
  logic [9-1:0] offset_r;
  logic cyc_r;
  logic stb_r;
  logic we_r;
  logic [32-1:0] dat_r;
  logic first_r;
  always_ff @(posedge clk or posedge rst) begin
    if (rst) begin
      cyc_r <= 1'b0;
      dat_r <= 0;
      first_r <= 1'b0;
      offset_r <= 0;
      rd_int_r <= 1'b0;
      reset_rx_fifo_r <= 1'b0;
      st_r <= 0;
      stb_r <= 1'b0;
      we_r <= 1'b0;
    end else begin
      rd_int_r <= 1'b0;
      if (en) begin
        if (~first_r) begin
          first_r <= 1'b1;
          offset_r <= 0;
          reset_rx_fifo_r <= 1'b1;
        end else begin
          reset_rx_fifo_r <= 1'b0;
          if (st_r == 0) begin
            // WAIT_DATA
            if (~rx_empty_w) begin
              dat_r <= rx_dat_out;
              rd_int_r <= 1'b1;
              st_r <= 1;
            end
          end else if (st_r == 1) begin
            // WRITE_WB
            cyc_r <= 1'b1;
            stb_r <= 1'b1;
            we_r <= 1'b1;
            if (m_wb_ack_i) begin
              offset_r <= 9'(offset_r + 1);
              cyc_r <= 1'b0;
              stb_r <= 1'b0;
              we_r <= 1'b0;
              st_r <= 0;
            end
          end
        end
      end else begin
        first_r <= 1'b0;
        cyc_r <= 1'b0;
        stb_r <= 1'b0;
        we_r <= 1'b0;
        st_r <= 0;
        offset_r <= 0;
      end
    end
  end
  assign m_wb_adr_o = 32'(adr + (32'($unsigned(offset_r)) << 2));
  assign m_wb_we_o = we_r;
  assign m_wb_dat_o = dat_r;
  assign m_wb_cyc_o = cyc_r;
  assign m_wb_stb_o = stb_r;
  assign m_wb_cti_o = 0;
  assign m_wb_bte_o = 0;
  assign full = rx_full_w;
  assign empty = rx_empty_w;

endmodule

