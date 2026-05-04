// SD RX FIFO — dual-clock, 4-bit nibble input → 32-bit word output
// Matches RealBench reference: ram + manual pointers, unsynchronized
// full/empty (combinational compare). Nibble accumulator packs 8×4-bit
// inputs into 32-bit words before writing to RAM.
//
// Spec: sd_rx_fifo.md — DEPTH=8, ADR_SIZE=4
// domain WrDomain
//   freq_mhz: 100

// domain RdDomain
//   freq_mhz: 50

module sd_rx_fifo #(
  parameter int RX_DEPTH = 8
) (
  input logic wclk,
  input logic rclk,
  input logic rst,
  input logic [3:0] d,
  input logic wr,
  output logic [31:0] q,
  input logic rd,
  output logic full,
  output logic empty,
  output logic [1:0] mem_empt
);

  // ── Nibble accumulator ───────────────────────────────────────────────
  logic [7:0] we;
  // one-hot rotating, bit 0 first
  logic [31:0] tmp;
  // accumulated 32-bit word
  logic ft;
  // first full word flag
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
  // Write to RAM when nibble accumulator completes a word
  logic ram_we;
  assign ram_we = wr & we[0:0] & ft;
  always_ff @(posedge wclk or posedge rst) begin
    if (rst) begin
      adr_i <= 0;
      ft <= 1'b0;
      ram_0 <= 0;
      ram_1 <= 0;
      ram_2 <= 0;
      ram_3 <= 0;
      ram_4 <= 0;
      ram_5 <= 0;
      ram_6 <= 0;
      ram_7 <= 0;
      tmp <= 0;
      we <= 1;
    end else begin
      if (wr) begin
        // Rotate we left
        we <= {we[6:0], we[7:7]};
        // BIG_ENDIAN: first nibble (we[0]) → MSB tmp[31:28]
        if (we[0:0]) begin
          tmp[31:28] <= d;
        end
        if (we[1:1]) begin
          tmp[27:24] <= d;
        end
        if (we[2:2]) begin
          tmp[23:20] <= d;
        end
        if (we[3:3]) begin
          tmp[19:16] <= d;
        end
        if (we[4:4]) begin
          tmp[15:12] <= d;
        end
        if (we[5:5]) begin
          tmp[11:8] <= d;
        end
        if (we[6:6]) begin
          tmp[7:4] <= d;
        end
        if (we[7:7]) begin
          tmp[3:0] <= d;
          ft <= 1'b1;
        end
      end
      // Write pointer
      if (ram_we) begin
        if (adr_i[2:0] == 3'(RX_DEPTH - 1)) begin
          adr_i[2:0] <= 0;
          adr_i[3:3] <= ~adr_i[3:3];
        end else begin
          adr_i <= 4'(adr_i + 1);
        end
        // Write tmp to RAM at old address
        if (adr_i[2:0] == 0) begin
          ram_0 <= tmp;
        end else if (adr_i[2:0] == 1) begin
          ram_1 <= tmp;
        end else if (adr_i[2:0] == 2) begin
          ram_2 <= tmp;
        end else if (adr_i[2:0] == 3) begin
          ram_3 <= tmp;
        end else if (adr_i[2:0] == 4) begin
          ram_4 <= tmp;
        end else if (adr_i[2:0] == 5) begin
          ram_5 <= tmp;
        end else if (adr_i[2:0] == 6) begin
          ram_6 <= tmp;
        end else begin
          ram_7 <= tmp;
        end
      end
    end
  end
  // Read pointer
  always_ff @(posedge rclk or posedge rst) begin
    if (rst) begin
      adr_o <= 0;
    end else begin
      if (~empty & rd) begin
        if (adr_o[2:0] == 3'(RX_DEPTH - 1)) begin
          adr_o[2:0] <= 0;
          adr_o[3:3] <= ~adr_o[3:3];
        end else begin
          adr_o <= 4'(adr_o + 1);
        end
      end
    end
  end
  // ── Read output mux ──────────────────────────────────────────────────
  // mem_empt = occupancy = adr_i - adr_o (lower 2 bits)
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
    mem_empt = level[1:0];
  end

endmodule

// SD FIFO RX Filler
// Reads data from RX FIFO and writes to system memory via Wishbone master.
// Internally instantiates sd_rx_fifo. Port interface matches OpenCores SDC reference.
module sd_fifo_rx_filler (
  input logic clk,
  input logic rst,
  output logic [31:0] m_wb_adr_o,
  output logic m_wb_we_o,
  output logic [31:0] m_wb_dat_o,
  output logic m_wb_cyc_o,
  output logic m_wb_stb_o,
  input logic m_wb_ack_i,
  output logic [2:0] m_wb_cti_o,
  output logic [1:0] m_wb_bte_o,
  input logic en,
  input logic [31:0] adr,
  input logic sd_clk,
  input logic [3:0] dat_i,
  input logic wr,
  output logic full,
  output logic empty
);

  // Wishbone master
  // Data master control
  // Data serial signals (directly to RX FIFO write side)
  // Internal RX FIFO
  logic [31:0] rx_dat_out;
  logic rx_full_w;
  logic rx_empty_w;
  logic [1:0] rx_mem_w;
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
  logic [1:0] st_r;
  logic [8:0] offset_r;
  logic cyc_r;
  logic stb_r;
  logic we_r;
  logic [31:0] dat_r;
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

