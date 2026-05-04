// SD TX FIFO — dual-clock, 32-bit in, 32-bit out
// Matches RealBench reference: ram + manual pointers, unsynchronized
// full/empty (combinational compare). Straightforward dual-clock FIFO.
//
// Spec: sd_tx_fifo.md — DEPTH=8, ADR_SIZE=4
// domain WrDomain
//   freq_mhz: 100

// domain RdDomain
//   freq_mhz: 50

module sd_tx_fifo #(
  parameter int TX_DEPTH = 8
) (
  input logic wclk,
  input logic rclk,
  input logic rst,
  input logic [31:0] d,
  input logic wr,
  output logic [31:0] q,
  input logic rd,
  output logic full,
  output logic empty,
  output logic [5:0] mem_empt
);

  // ── RAM storage ──────────────────────────────────────────────────────
  logic [31:0] ram_0;
  logic [31:0] ram_1;
  logic [31:0] ram_2;
  logic [31:0] ram_3;
  logic [31:0] ram_4;
  logic [31:0] ram_5;
  logic [31:0] ram_6;
  logic [31:0] ram_7;
  // ── Pointers: [MSB]=wrap, [2:0]=address ─────────────────────────────
  logic [3:0] adr_i;
  // write pointer (wclk domain)
  logic [3:0] adr_o;
  // read pointer  (rclk domain)
  // Write side
  always_ff @(posedge wclk or posedge rst) begin
    if (rst) begin
      adr_i <= 0;
      ram_0 <= 0;
      ram_1 <= 0;
      ram_2 <= 0;
      ram_3 <= 0;
      ram_4 <= 0;
      ram_5 <= 0;
      ram_6 <= 0;
      ram_7 <= 0;
    end else begin
      if (wr & ~full) begin
        // Write data to RAM at current address
        if (adr_i[2:0] == 0) begin
          ram_0 <= d;
        end else if (adr_i[2:0] == 1) begin
          ram_1 <= d;
        end else if (adr_i[2:0] == 2) begin
          ram_2 <= d;
        end else if (adr_i[2:0] == 3) begin
          ram_3 <= d;
        end else if (adr_i[2:0] == 4) begin
          ram_4 <= d;
        end else if (adr_i[2:0] == 5) begin
          ram_5 <= d;
        end else if (adr_i[2:0] == 6) begin
          ram_6 <= d;
        end else begin
          ram_7 <= d;
        end
        // Increment write pointer
        if (adr_i[2:0] == 3'(TX_DEPTH - 1)) begin
          adr_i[2:0] <= 0;
          adr_i[3:3] <= ~adr_i[3:3];
        end else begin
          adr_i <= 4'(adr_i + 1);
        end
      end
    end
  end
  // Read side
  always_ff @(posedge rclk or posedge rst) begin
    if (rst) begin
      adr_o <= 0;
    end else begin
      if (~empty & rd) begin
        if (adr_o[2:0] == 3'(TX_DEPTH - 1)) begin
          adr_o[2:0] <= 0;
          adr_o[3:3] <= ~adr_o[3:3];
        end else begin
          adr_o <= 4'(adr_o + 1);
        end
      end
    end
  end
  // ── Combinational read + status ──────────────────────────────────────
  // mem_empt = occupancy = adr_i - adr_o
  logic [5:0] level;
  assign level = 5'($unsigned(adr_i)) - 5'($unsigned(adr_o));
  always_comb begin
    if (adr_o[2:0] == 0) begin
      q = ram_0;
    end else if (adr_o[2:0] == 1) begin
      q = ram_1;
    end else if (adr_o[2:0] == 2) begin
      q = ram_2;
    end else if (adr_o[2:0] == 3) begin
      q = ram_3;
    end else if (adr_o[2:0] == 4) begin
      q = ram_4;
    end else if (adr_o[2:0] == 5) begin
      q = ram_5;
    end else if (adr_o[2:0] == 6) begin
      q = ram_6;
    end else begin
      q = ram_7;
    end
    // Full/empty: combinational pointer compare (matches reference)
    full = (adr_i[2:0] == adr_o[2:0]) & (adr_i[3:3] ^ adr_o[3:3]);
    empty = adr_i == adr_o;
    // mem_empt from pre-computed level
    mem_empt = level[5:0];
  end

endmodule

// SD FIFO TX Filler
// DMA: reads data from system memory via Wishbone master, fills TX FIFO.
module sd_fifo_tx_filler (
  input logic clk,
  input logic rst,
  output logic [31:0] m_wb_adr_o,
  output logic m_wb_we_o,
  input logic [31:0] m_wb_dat_i,
  output logic m_wb_cyc_o,
  output logic m_wb_stb_o,
  input logic m_wb_ack_i,
  output logic [2:0] m_wb_cti_o,
  output logic [1:0] m_wb_bte_o,
  input logic en,
  input logic [31:0] adr,
  input logic sd_clk,
  output logic [31:0] dat_o,
  input logic rd,
  output logic empty,
  output logic fe
);

  logic [31:0] offset_r;
  logic [31:0] din_r;
  logic wr_tx_r;
  logic delay_r;
  logic ackd_r;
  logic cyc_r;
  logic stb_r;
  logic fe_w;
  logic empty_w;
  logic [5:0] mem_empt_w;
  sd_tx_fifo u_fifo (
    .wclk(clk),
    .rclk(sd_clk),
    .rst(rst),
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

