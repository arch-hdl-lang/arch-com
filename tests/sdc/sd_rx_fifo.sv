// SD RX FIFO (Dual-clock, 4-bit input -> 32-bit output, depth 8)
// Nibble packing wrapper around ARCH fifo construct with OVERFLOW mode.
// BIG_ENDIAN: first nibble goes to MSB (tmp[31:28]).
// domain WrDomain
//   freq_mhz: 100

// domain RdDomain
//   freq_mhz: 50

module RxFifoCore #(
  parameter int  DEPTH      = 8,
  parameter int  OVERFLOW   = 1,
  parameter int  DATA_WIDTH = 32
) (
  input logic wr_clk,
  input logic rd_clk,
  input logic rst,
  input logic push_valid,
  output logic push_ready,
  input logic [DATA_WIDTH-1:0] push_data,
  output logic pop_valid,
  input logic pop_ready,
  output logic [DATA_WIDTH-1:0] pop_data
);

  localparam int PTR_W = $clog2(DEPTH) + 1;
  
  // Gray-code helper functions
  function automatic logic [PTR_W-1:0] bin2gray(input logic [PTR_W-1:0] b);
    return b ^ (b >> 1);
  endfunction
  function automatic logic [PTR_W-1:0] gray2bin(input logic [PTR_W-1:0] g);
    logic [PTR_W-1:0] b;
    b[PTR_W-1] = g[PTR_W-1];
    for (int i = PTR_W-2; i >= 0; i--) b[i] = b[i+1] ^ g[i];
    return b;
  endfunction
  
  logic [DATA_WIDTH-1:0] mem [0:DEPTH-1];
  logic [PTR_W-1:0] wr_ptr_bin, rd_ptr_bin;
  logic [PTR_W-1:0] wr_ptr_gray, rd_ptr_gray;
  // Two-stage synchronizers
  logic [PTR_W-1:0] wr_ptr_gray_s1, wr_ptr_gray_sync; // in rd domain
  logic [PTR_W-1:0] rd_ptr_gray_s1, rd_ptr_gray_sync; // in wr domain
  
  assign wr_ptr_gray = bin2gray(wr_ptr_bin);
  assign rd_ptr_gray = bin2gray(rd_ptr_bin);
  
  // Sync wr_ptr into rd domain (rd_clk)
  always_ff @(posedge rd_clk or posedge rst) begin
    if (rst) begin wr_ptr_gray_s1 <= '0; wr_ptr_gray_sync <= '0; end
    else begin wr_ptr_gray_s1 <= wr_ptr_gray; wr_ptr_gray_sync <= wr_ptr_gray_s1; end
  end
  // Sync rd_ptr into wr domain (wr_clk)
  always_ff @(posedge wr_clk or posedge rst) begin
    if (rst) begin rd_ptr_gray_s1 <= '0; rd_ptr_gray_sync <= '0; end
    else begin rd_ptr_gray_s1 <= rd_ptr_gray; rd_ptr_gray_sync <= rd_ptr_gray_s1; end
  end
  
  // Write domain: full detection using synced rd_ptr
  logic full_r;
  logic [PTR_W-1:0] rd_ptr_bin_wr;
  assign rd_ptr_bin_wr = gray2bin(rd_ptr_gray_sync);
  assign full_r  = (wr_ptr_bin[PTR_W-1] != rd_ptr_bin_wr[PTR_W-1]) &&
                   (wr_ptr_bin[PTR_W-2:0] == rd_ptr_bin_wr[PTR_W-2:0]);
  assign push_ready = (OVERFLOW != 0) ? 1'b1 : !full_r;
  always_ff @(posedge wr_clk or posedge rst) begin
    if (rst) wr_ptr_bin <= '0;
    else if (push_valid && push_ready) begin
      mem[wr_ptr_bin[PTR_W-2:0]] <= push_data;
      wr_ptr_bin <= wr_ptr_bin + 1;
    end
  end
  
  // Read domain: empty detection using synced wr_ptr
  logic empty_r;
  logic [PTR_W-1:0] wr_ptr_bin_rd;
  assign wr_ptr_bin_rd = gray2bin(wr_ptr_gray_sync);
  assign empty_r = (rd_ptr_bin == wr_ptr_bin_rd);
  assign pop_valid = !empty_r;
  assign pop_data  = mem[rd_ptr_bin[PTR_W-2:0]];
  always_ff @(posedge rd_clk or posedge rst) begin
    if (rst) rd_ptr_bin <= '0;
    else if (pop_valid && pop_ready) rd_ptr_bin <= rd_ptr_bin + 1;
  end

endmodule

module sd_rx_fifo (
  input logic wclk,
  input logic rclk,
  input logic rst,
  input logic [4-1:0] d,
  input logic wr,
  output logic [32-1:0] q,
  input logic rd,
  output logic full,
  output logic empty,
  output logic [2-1:0] mem_empt
);

  // Nibble accumulator state
  logic [8-1:0] we_r;
  // one-hot rotating, bit 0 first
  logic [32-1:0] tmp;
  logic ft;
  logic push_ready_w;
  logic pop_valid_w;
  // Word complete: we wraps back to bit 0 AND first full word was accumulated
  logic word_complete;
  assign word_complete = wr & we_r[0:0] & ft;
  always_ff @(posedge wclk or posedge rst) begin
    if (rst) begin
      ft <= 1'b0;
      tmp <= 0;
      we_r <= 1;
    end else begin
      if (wr) begin
        // Rotate we left
        we_r <= {we_r[6:0], we_r[7:7]};
        // BIG_ENDIAN: first nibble (we[0]) → MSB tmp[31:28]
        if (we_r[0:0]) begin
          tmp[31:28] <= d;
        end
        if (we_r[1:1]) begin
          tmp[27:24] <= d;
        end
        if (we_r[2:2]) begin
          tmp[23:20] <= d;
        end
        if (we_r[3:3]) begin
          tmp[19:16] <= d;
        end
        if (we_r[4:4]) begin
          tmp[15:12] <= d;
        end
        if (we_r[5:5]) begin
          tmp[11:8] <= d;
        end
        if (we_r[6:6]) begin
          tmp[7:4] <= d;
        end
        if (we_r[7:7]) begin
          tmp[3:0] <= d;
          ft <= 1'b1;
        end
      end
    end
  end
  // Core FIFO with OVERFLOW: never back-pressures, overwrites oldest when full
  RxFifoCore fifo_core (
    .wr_clk(wclk),
    .rd_clk(rclk),
    .rst(rst),
    .push_valid(word_complete),
    .push_ready(push_ready_w),
    .push_data(tmp),
    .pop_valid(pop_valid_w),
    .pop_ready(rd),
    .pop_data(q)
  );
  assign full = ~push_ready_w;
  assign empty = ~pop_valid_w;
  assign mem_empt = 0;

endmodule

