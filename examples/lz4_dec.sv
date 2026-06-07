//! ---
//! tags: [fsm, ram, streaming, compression, lz4, cast-lz4snp-d]
//! ---
//!
//! LZ4 block decompressor — models the CAST LZ4SNP-D IP.
//!
//! Accepts a raw LZ4 block as a byte-streaming input (valid/ready/data/last)
//! and produces the decompressed byte stream as output. The `in_last` flag
//! must be asserted on the final compressed byte of the block (the last
//! literal byte of the last sequence — in LZ4 block format the last sequence
//! always ends with literals and has no match/offset tail).
//!
//! Key design choices
//! ------------------
//! * History buffer: 64 KiB simple-dual-port RAM, latency 0 (async read).
//!   The read data is captured by a `reg … reset none` on each rising edge,
//!   which gives an effective 1-cycle read latency in the FSM (the FSM spends
//!   one "issue" cycle before the "data" cycle).
//! * Match copy: two-phase per byte (issue read → use data), so match
//!   throughput is 0.5 B/cycle. Literal copy is 1 B/cycle.
//! * No implicit latches: every combinational output has an explicit default
//!   before the if/elsif chain.
//!
//! References
//! ----------
//! LZ4 block format: https://github.com/lz4/lz4/blob/dev/doc/lz4_Block_format.md
//! CAST LZ4SNP-D:    https://www.cast-inc.com/compression/lossless-data-compression/lz4snp-d
// domain SysDomain
//   freq_mhz: 200

// ── History ring buffer ────────────────────────────────────────────────────────
/// 64 KiB simple-dual-port history buffer.  Async read (latency 0) so the
/// read data is combinatorially available on the same cycle the address is
/// presented; the parent module captures it in a `reg reset none` to get a
/// clean 1-cycle read latency without any additional pipelining registers.
module HistoryBuf #(
  parameter int DEPTH = 256,
  parameter int DATA_WIDTH = 8
) (
  input logic clk,
  input logic wr_port_en,
  input logic [15:0] wr_port_addr,
  input logic [7:0] wr_port_data,
  input logic rd_port_en,
  input logic [15:0] rd_port_addr,
  output logic [7:0] rd_port_data
);

  logic [DATA_WIDTH-1:0] mem [0:DEPTH-1];
  
  assign rd_port_data = mem[rd_port_addr];
  
  always_ff @(posedge clk) begin
    if (wr_port_en)
      mem[wr_port_addr] <= wr_port_data;
  end
  
  initial begin
    for (int i = 0; i < DEPTH; i++) mem[i] = '0;
  end

endmodule

// ── State encoding ─────────────────────────────────────────────────────────────
/// FSM states for the LZ4 sequence parser.
typedef enum logic [2:0] {
  TOKEN = 3'd0,
  EXTLIT = 3'd1,
  COPYLIT = 3'd2,
  OFFLO = 3'd3,
  OFFHI = 3'd4,
  EXTMATCH = 3'd5,
  COPYMATCH = 3'd6,
  IDLE = 3'd7
} Lz4State;

// consume token byte; decode lit-len and match-len nibbles
// accumulate extended literal-length bytes (value == 255 → continue)
// stream literal bytes: input → output and history write
// consume low byte of match offset
// consume high byte; initialise match_len_r
// accumulate extended match-length bytes
// copy from history → output and history write (2-phase per byte)
// one-cycle gap between back-to-back blocks; auto-advances to Token
// ── Top-level decompressor module ─────────────────────────────────────────────
/// Streaming LZ4 block decompressor.
///
/// Input handshake:  `in_valid` / `in_ready` / `in_data` / `in_last`
/// Output handshake: `out_valid` / `out_ready` / `out_data` / `out_last`
///
/// `in_last` must be asserted on the last byte of the compressed input block.
/// `out_last` pulses on the last decompressed byte of the block.
module Lz4Dec (
  input logic clk,
  input logic rst,
  input logic in_valid,
  output logic in_ready,
  input logic [7:0] in_data,
  input logic in_last,
  output logic out_valid,
  input logic out_ready,
  output logic [7:0] out_data,
  output logic out_last
);

  // Compressed input stream
  // Decompressed output stream
  // ── FSM state ────────────────────────────────────────────────────────────────
  Lz4State state_r;
  logic copy_phase_r;
  // ── Datapath registers ───────────────────────────────────────────────────────
  logic [15:0] lit_len_r;
  // remaining literals to copy
  logic [3:0] match_code_r;
  // token lower nibble (match len code)
  logic [15:0] match_len_r;
  // remaining match bytes to copy
  logic [15:0] offset_r;
  // match offset (distance back in history)
  logic [7:0] offset_lo_r;
  // temp: low byte of offset
  logic [15:0] wr_ptr_r;
  // write cursor in history ring
  // ── History RAM interface wires (driven by comb block) ───────────────────────
  logic hist_wr_en_w;
  logic [15:0] hist_wr_addr_w;
  logic [7:0] hist_wr_data_w;
  logic hist_rd_en_w;
  logic [15:0] hist_rd_addr_w;
  // Async read result captured every rising edge; latency-0 RAM output is
  // combinatorial so this register gives the effective 1-cycle read latency.
  logic [7:0] hist_rd_data_r = 0;
  // ── History RAM instance ─────────────────────────────────────────────────────
  HistoryBuf hist (
    .clk(clk),
    .wr_port_en(hist_wr_en_w),
    .wr_port_addr(hist_wr_addr_w),
    .wr_port_data(hist_wr_data_w),
    .rd_port_en(hist_rd_en_w),
    .rd_port_addr(hist_rd_addr_w),
    .rd_port_data(hist_rd_data_r)
  );
  // ── Combinational block: outputs and RAM control ─────────────────────────────
  //
  // Pattern: assign safe defaults unconditionally, then override per state.
  // Every signal is written on every execution path so the latch check passes.
  always_comb begin
    // Safe defaults (all other states fall through to these)
    in_ready = 1'b0;
    out_valid = 1'b0;
    out_data = 0;
    out_last = 1'b0;
    hist_wr_en_w = 1'b0;
    hist_wr_addr_w = 0;
    hist_wr_data_w = 0;
    hist_rd_en_w = 1'b0;
    hist_rd_addr_w = 0;
    if (state_r == TOKEN || state_r == EXTLIT || state_r == OFFLO || state_r == OFFHI || state_r == EXTMATCH) begin
      // These states consume one input byte per cycle; no output.
      in_ready = 1'b1;
    end else if (state_r == COPYLIT) begin
      // Literals: consume and produce simultaneously; backpressure from
      // downstream stalls both sides.
      in_ready = out_ready;
      out_valid = in_valid;
      out_data = in_data;
      out_last = in_valid && out_ready && lit_len_r == 1 && in_last;
      hist_wr_en_w = in_valid && out_ready;
      hist_wr_addr_w = wr_ptr_r;
      hist_wr_data_w = in_data;
    end else if (state_r == COPYMATCH) begin
      // Match copy (2-phase):
      //   Phase 0 (copy_phase_r = false): issue the async read; no output yet.
      //   Phase 1 (copy_phase_r = true):  drive output from captured read data;
      //                                   also write that byte back to history.
      // Keep rd_en/rd_addr active in both phases so hist_rd_data_r stays valid
      // during any backpressure stall in phase 1.
      hist_rd_en_w = 1'b1;
      hist_rd_addr_w = 16'(wr_ptr_r - offset_r);
      if (copy_phase_r) begin
        out_valid = 1'b1;
        out_data = hist_rd_data_r;
        hist_wr_en_w = out_ready;
        hist_wr_addr_w = wr_ptr_r;
        hist_wr_data_w = hist_rd_data_r;
      end
    end
  end
  // ── Sequential block: state transitions and register updates ─────────────────
  always_ff @(posedge clk) begin
    if (rst) begin
      copy_phase_r <= 1'b0;
      lit_len_r <= 0;
      match_code_r <= 0;
      match_len_r <= 0;
      offset_lo_r <= 0;
      offset_r <= 0;
      state_r <= TOKEN;
      wr_ptr_r <= 0;
    end else begin
      if (state_r == TOKEN) begin
        if (in_valid) begin
          match_code_r <= in_data[3:0];
          if (in_data[7:4] == 15) begin
            // Extended literal length: start accumulation at 15
            lit_len_r <= 15;
            state_r <= EXTLIT;
          end else if (in_data[7:4] > 0) begin
            // Literal count fits in the nibble
            lit_len_r <= 16'($unsigned(in_data[7:4]));
            state_r <= COPYLIT;
          end else if (in_last) begin
            // Token with 0 literals is the terminal sequence; block is done
            state_r <= IDLE;
          end else begin
            // No literals; go straight to match offset
            state_r <= OFFLO;
          end
        end
      end else if (state_r == EXTLIT) begin
        if (in_valid) begin
          lit_len_r <= 16'(lit_len_r + 16'($unsigned(in_data)));
          if (in_data != 255) begin
            state_r <= COPYLIT;
          end
        end
      end else if (state_r == COPYLIT) begin
        if (in_valid && out_ready) begin
          lit_len_r <= (16 > 1 ? 16 : 1)'(lit_len_r - 1);
          wr_ptr_r <= (16 > 1 ? 16 : 1)'(wr_ptr_r + 1);
          if (lit_len_r == 1) begin
            if (in_last) begin
              state_r <= IDLE;
            end else begin
              state_r <= OFFLO;
            end
          end
        end
      end else if (state_r == OFFLO) begin
        if (in_valid) begin
          offset_lo_r <= in_data;
          state_r <= OFFHI;
        end
      end else if (state_r == OFFHI) begin
        if (in_valid) begin
          offset_r <= 16'($unsigned(in_data)) << 8 | 16'($unsigned(offset_lo_r));
          match_len_r <= (16 > 3 ? 16 : 3)'(16'($unsigned(match_code_r)) + 4);
          if (match_code_r == 15) begin
            state_r <= EXTMATCH;
          end else begin
            copy_phase_r <= 1'b0;
            state_r <= COPYMATCH;
          end
        end
      end else if (state_r == EXTMATCH) begin
        if (in_valid) begin
          match_len_r <= 16'(match_len_r + 16'($unsigned(in_data)));
          if (in_data != 255) begin
            copy_phase_r <= 1'b0;
            state_r <= COPYMATCH;
          end
        end
      end else if (state_r == COPYMATCH) begin
        if (!copy_phase_r) begin
          // Phase 0: read issued; advance to phase 1 next cycle
          copy_phase_r <= 1'b1;
        end else if (out_ready) begin
          // Phase 1: data consumed by downstream
          match_len_r <= (16 > 1 ? 16 : 1)'(match_len_r - 1);
          wr_ptr_r <= (16 > 1 ? 16 : 1)'(wr_ptr_r + 1);
          copy_phase_r <= 1'b0;
          if (match_len_r == 1) begin
            state_r <= TOKEN;
          end
        end
      end else if (state_r == IDLE) begin
        // One-cycle inter-block gap; auto-advance to accept next block
        state_r <= TOKEN;
      end
    end
  end

endmodule

