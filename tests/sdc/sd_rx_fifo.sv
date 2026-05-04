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

