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

