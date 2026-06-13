//! ---
//! tags: [compression, lz4, decompressor, streaming, ram, thread]
//! refs:
//!   - "LZ4 Block Format Specification v1.9.3"
//!   - "CAST LZ4SNP-D IP Core (reference architecture)"
//! ---
//!
//! LZ4 block decompressor — streaming byte-wide implementation.
//!
//! Accepts a raw LZ4 block (no frame header) as a byte stream and emits
//! the decompressed output. Handles token / literal / offset / match
//! sequences including one level of extended lengths.
//! History window: 256 bytes (offsets 1–255 supported).
//!
//! Interface: AXI4-Stream-compatible handshake on both sides.
//!   in_valid / in_ready / in_data / in_last  — compressed input
//!   out_valid / out_ready / out_data / out_last — decompressed output
//!
//! Thread semantics: the Decompress thread processes ONE LZ4 sequence per
//! invocation, then restarts. `wait until not saw_last_r` at the top
//! gates out after the last sequence (saw_last_r set when in_last fires
//! during the literal copy phase). Reset clears all state for a new block.
/// 256-byte circular history buffer — simple-dual-port BRAM.
/// LZ4 block decompressor.
module _Lz4Decomp_threads #(
  localparam [4:0] _t0_S0_wait_until = 0,
  localparam [4:0] _t0_S1_wait_until = 1,
  localparam [4:0] _t0_S2_action = 2,
  localparam [4:0] _t0_S3_action = 3,
  localparam [4:0] _t0_S4_dispatch = 4,
  localparam [4:0] _t0_S5_wait_until = 5,
  localparam [4:0] _t0_S6_action = 6,
  localparam [4:0] _t0_S7_action = 7,
  localparam [4:0] _t0_S8_dispatch = 8,
  localparam [4:0] _t0_S9_action = 9,
  localparam [4:0] _t0_S10_wait_until = 10,
  localparam [4:0] _t0_S11_action = 11,
  localparam [4:0] _t0_S12_wait_until = 12,
  localparam [4:0] _t0_S13_dispatch = 13,
  localparam [4:0] _t0_S14_dispatch = 14,
  localparam [4:0] _t0_S15_wait_until = 15,
  localparam [4:0] _t0_S16_action = 16,
  localparam [4:0] _t0_S17_wait_until = 17,
  localparam [4:0] _t0_S18_action = 18,
  localparam [4:0] _t0_S19_action = 19,
  localparam [4:0] _t0_S20_dispatch = 20,
  localparam [4:0] _t0_S21_wait_until = 21,
  localparam [4:0] _t0_S22_action = 22,
  localparam [4:0] _t0_S23_action = 23,
  localparam [4:0] _t0_S24_action = 24,
  localparam [4:0] _t0_S25_action = 25,
  localparam [4:0] _t0_S26_wait_until = 26,
  localparam [4:0] _t0_S27_dispatch = 27
) (
  input logic clk,
  input logic rst,
  input logic [7:0] h_rd_data_w,
  input logic [7:0] in_data,
  input logic in_last,
  input logic in_valid,
  input logic out_ready,
  output logic [7:0] h_rd_addr_w,
  output logic h_rd_en_w,
  output logic [7:0] h_wr_addr_w,
  output logic [7:0] h_wr_data_w,
  output logic h_wr_en_w,
  output logic h_wr_wen_w,
  output logic in_ready,
  output logic [7:0] out_data,
  output logic out_last,
  output logic out_valid,
  output logic byte_last_r,
  output logic [7:0] byte_r,
  output logic [7:0] ext_r,
  output logic [15:0] lit_len_r,
  output logic [15:0] mat_len_r,
  output logic [15:0] offset_r,
  output logic [7:0] rd_ptr_r,
  output logic saw_last_r,
  output logic [3:0] token_hi_r,
  output logic [3:0] token_lo_r,
  output logic [7:0] wr_ptr_r
);

  logic [4:0] _t0_state = 0;
  logic [31:0] _t0_cnt = 0;
  logic [15:0] _t0_loop_cnt_0 = 0;
  logic [15:0] _t0_loop_cnt_1 = 0;
  always_comb begin
    h_rd_addr_w = 0;
    h_rd_en_w = 0;
    h_wr_addr_w = 0;
    h_wr_data_w = 0;
    h_wr_en_w = 0;
    h_wr_wen_w = 0;
    in_ready = 0;
    out_data = 0;
    out_last = 0;
    out_valid = 0;
    // Compressed input
    // Decompressed output
    // ── History RAM wires ─────────────────────────────────────────────────────
    // ── Working registers ─────────────────────────────────────────────────────
    // ── LZ4 decompressor thread ───────────────────────────────────────────────
    // One LZ4 sequence per thread execution; implicit restart loops back.
    // `saw_last_r` latches on `in_last` and gates the restart condition.
    in_ready = 1'b0;
    out_valid = 1'b0;
    out_data = 8'd0;
    out_last = 1'b0;
    h_rd_en_w = 1'b0;
    h_rd_addr_w = 8'd0;
    h_wr_en_w = 1'b0;
    h_wr_wen_w = 1'b0;
    h_wr_addr_w = 8'd0;
    h_wr_data_w = 8'd0;
    if (_t0_state == _t0_S1_wait_until) begin
      // Entry gate: stall here once the block is fully decoded.
      // ── Phase A: consume token byte ─────────────────────────────────────────
      in_ready = 1;
    end
    if (_t0_state == _t0_S5_wait_until) begin
      // ── Phase B: initial literal length from high nibble ────────────────────
      // ── Phase C: one extension byte for literal length (if nibble == 15) ────
      // Handles lit_len up to 15 + 255 = 270 bytes per sequence.
      in_ready = 1;
    end
    if (_t0_state == _t0_S10_wait_until) begin
      // ── Phase D: copy lit_len literal bytes ─────────────────────────────────
      // Consume input byte
      in_ready = 1;
    end
    if (_t0_state == _t0_S12_wait_until) begin
      // Drive output until consumer accepts
      out_valid = 1;
      out_data = byte_r;
      out_last = byte_last_r;
    end
    if (_t0_state == _t0_S13_dispatch) begin
      // Write literal to history ring buffer
      h_wr_en_w = 1;
      h_wr_wen_w = 1;
      h_wr_addr_w = wr_ptr_r;
      h_wr_data_w = byte_r;
    end
    if (_t0_state == _t0_S15_wait_until) begin
      // ── Phase E–H: match section (absent in the last sequence) ──────────────
      // Phase E: match offset low byte
      in_ready = 1;
    end
    if (_t0_state == _t0_S17_wait_until) begin
      // Phase F: match offset high byte
      in_ready = 1;
    end
    if (_t0_state == _t0_S21_wait_until) begin
      // Phase G: initial match length = low nibble + 4
      // Phase G2: one extension byte for match length (if nibble == 15)
      in_ready = 1;
    end
    if (_t0_state == _t0_S25_action) begin
      // Phase H: copy mat_len bytes from history
      // rd_ptr_r = wr_ptr_r − offset (wrapping, 256-byte window)
      // Issue latency-1 RAM read
      h_rd_en_w = 1;
      h_rd_addr_w = rd_ptr_r;
    end
    if (_t0_state == _t0_S26_wait_until) begin
      // Data available: drive output
      out_valid = 1;
      out_data = h_rd_data_w;
    end
    if (_t0_state == _t0_S27_dispatch) begin
      // Write match byte to history (h_rd_data_w stable: rd.en=0 here)
      h_wr_en_w = 1;
      h_wr_wen_w = 1;
      h_wr_addr_w = wr_ptr_r;
      h_wr_data_w = h_rd_data_w;
    end
    // Thread restarts from `wait until not saw_last_r`.
  end
  always_ff @(posedge clk) begin
    if (rst) begin
      _t0_state <= 0;
      byte_last_r <= 1'b0;
      byte_r <= 0;
      ext_r <= 0;
      lit_len_r <= 0;
      mat_len_r <= 0;
      offset_r <= 0;
      rd_ptr_r <= 0;
      saw_last_r <= 1'b0;
      token_hi_r <= 0;
      token_lo_r <= 0;
      wr_ptr_r <= 0;
    end else begin
      if (_t0_state == _t0_S0_wait_until) begin
        if (!saw_last_r) begin
          _t0_state <= _t0_S1_wait_until;
        end
      end
      if (_t0_state == _t0_S1_wait_until) begin
        if (in_valid) begin
          token_hi_r <= in_data[7:4];
          token_lo_r <= in_data[3:0];
          saw_last_r <= saw_last_r || in_last;
          _t0_state <= _t0_S3_action;
        end
      end
      if (_t0_state == _t0_S3_action) begin
        lit_len_r <= 16'($unsigned(token_hi_r));
        _t0_state <= _t0_S4_dispatch;
      end
      if (_t0_state == _t0_S4_dispatch) begin
        if (token_hi_r == 4'd15) begin
          _t0_state <= _t0_S5_wait_until;
        end
        if (!(token_hi_r == 4'd15)) begin
          _t0_state <= _t0_S8_dispatch;
        end
      end
      if (_t0_state == _t0_S5_wait_until) begin
        if (in_valid) begin
          ext_r <= in_data;
          saw_last_r <= saw_last_r || in_last;
          _t0_state <= _t0_S7_action;
        end
      end
      if (_t0_state == _t0_S7_action) begin
        lit_len_r <= 16'(lit_len_r + 16'($unsigned(ext_r)));
        if (1'b1) begin
          _t0_state <= _t0_S8_dispatch;
        end
      end
      if (_t0_state == _t0_S8_dispatch) begin
        if (lit_len_r > 16'd0) begin
          _t0_state <= _t0_S9_action;
        end
        if (!(lit_len_r > 16'd0)) begin
          _t0_state <= _t0_S14_dispatch;
        end
      end
      if (_t0_state == _t0_S9_action) begin
        _t0_state <= _t0_S10_wait_until;
      end
      if (_t0_state == _t0_S10_wait_until) begin
        if (in_valid) begin
          byte_r <= in_data;
          byte_last_r <= in_last;
          saw_last_r <= saw_last_r || in_last;
          _t0_state <= _t0_S12_wait_until;
        end
      end
      if (_t0_state == _t0_S12_wait_until) begin
        if (out_ready) begin
          _t0_state <= _t0_S13_dispatch;
        end
      end
      if (_t0_state == _t0_S13_dispatch) begin
        wr_ptr_r <= 8'(wr_ptr_r + 8'd1);
        if (_t0_loop_cnt_0 < 16'(lit_len_r - 1)) begin
          _t0_state <= _t0_S10_wait_until;
        end
        if (_t0_loop_cnt_0 >= 16'(lit_len_r - 1)) begin
          _t0_state <= _t0_S14_dispatch;
        end
      end
      if (_t0_state == _t0_S14_dispatch) begin
        if (!saw_last_r) begin
          _t0_state <= _t0_S15_wait_until;
        end
        if (!!saw_last_r) begin
          _t0_state <= _t0_S0_wait_until;
        end
      end
      if (_t0_state == _t0_S15_wait_until) begin
        if (in_valid) begin
          offset_r <= 16'($unsigned(in_data));
          saw_last_r <= saw_last_r || in_last;
          _t0_state <= _t0_S17_wait_until;
        end
      end
      if (_t0_state == _t0_S17_wait_until) begin
        if (in_valid) begin
          offset_r <= {in_data, offset_r[7:0]};
          saw_last_r <= saw_last_r || in_last;
          _t0_state <= _t0_S19_action;
        end
      end
      if (_t0_state == _t0_S19_action) begin
        mat_len_r <= 16'(16'($unsigned(token_lo_r)) + 16'd4);
        _t0_state <= _t0_S20_dispatch;
      end
      if (_t0_state == _t0_S20_dispatch) begin
        if (token_lo_r == 4'd15) begin
          _t0_state <= _t0_S21_wait_until;
        end
        if (!(token_lo_r == 4'd15)) begin
          _t0_state <= _t0_S24_action;
        end
      end
      if (_t0_state == _t0_S21_wait_until) begin
        if (in_valid) begin
          ext_r <= in_data;
          _t0_state <= _t0_S23_action;
        end
      end
      if (_t0_state == _t0_S23_action) begin
        mat_len_r <= 16'(mat_len_r + 16'($unsigned(ext_r)));
        if (1'b1) begin
          _t0_state <= _t0_S24_action;
        end
      end
      if (_t0_state == _t0_S24_action) begin
        rd_ptr_r <= 8'(wr_ptr_r - 8'(offset_r));
        _t0_state <= _t0_S25_action;
      end
      if (_t0_state == _t0_S25_action) begin
        _t0_state <= _t0_S26_wait_until;
      end
      if (_t0_state == _t0_S26_wait_until) begin
        if (out_ready) begin
          _t0_state <= _t0_S27_dispatch;
        end
      end
      if (_t0_state == _t0_S27_dispatch) begin
        wr_ptr_r <= 8'(wr_ptr_r + 8'd1);
        rd_ptr_r <= 8'(rd_ptr_r + 8'd1);
        if (_t0_loop_cnt_1 < 16'(mat_len_r - 1)) begin
          _t0_state <= _t0_S25_action;
        end
        if (_t0_loop_cnt_1 >= 16'(mat_len_r - 1)) begin
          _t0_state <= _t0_S0_wait_until;
        end
      end
    end
  end
  always_ff @(posedge clk) begin
    if (_t0_state == _t0_S9_action) begin
      _t0_loop_cnt_0 <= 0;
    end
    if (_t0_state == _t0_S13_dispatch) begin
      _t0_loop_cnt_0 <= 16'(_t0_loop_cnt_0 + 16'd1);
    end
    if (_t0_state == _t0_S24_action) begin
      _t0_loop_cnt_1 <= 0;
    end
    if (_t0_state == _t0_S27_dispatch) begin
      _t0_loop_cnt_1 <= 16'(_t0_loop_cnt_1 + 16'd1);
    end
  end

endmodule

// domain SysDomain
//   freq_mhz: 200

module HistBuf #(
  parameter int DEPTH = 256,
  parameter int DATA_WIDTH = 8
) (
  input logic clk,
  input logic [7:0] rd_port_addr,
  input logic rd_port_en,
  output logic [7:0] rd_port_rdata,
  input logic [7:0] wr_port_addr,
  input logic wr_port_en,
  input logic wr_port_wen,
  input logic [7:0] wr_port_wdata
);

  logic [DATA_WIDTH-1:0] mem [0:DEPTH-1];
  logic [DATA_WIDTH-1:0] rd_port_rdata_r;
  
  always_ff @(posedge clk) begin
    if (wr_port_en)
      mem[wr_port_addr] <= wr_port_wdata;
    if (rd_port_en)
      rd_port_rdata_r <= mem[rd_port_addr];
  end
  assign rd_port_rdata = rd_port_rdata_r;
  
  initial begin
    for (int i = 0; i < DEPTH; i++) mem[i] = '0;
  end

endmodule

module Lz4Decomp (
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

  logic h_rd_en_w;
  logic [7:0] h_rd_addr_w;
  logic [7:0] h_rd_data_w;
  logic h_wr_en_w;
  logic h_wr_wen_w;
  logic [7:0] h_wr_addr_w;
  logic [7:0] h_wr_data_w;
  logic [3:0] token_hi_r;
  logic [3:0] token_lo_r;
  logic [15:0] lit_len_r;
  logic [15:0] mat_len_r;
  logic [15:0] offset_r;
  logic [7:0] wr_ptr_r;
  logic [7:0] rd_ptr_r;
  logic [7:0] byte_r;
  logic byte_last_r;
  logic [7:0] ext_r;
  logic saw_last_r;
  HistBuf hist (
    .clk(clk),
    .rd_port_en(h_rd_en_w),
    .rd_port_addr(h_rd_addr_w),
    .rd_port_rdata(h_rd_data_w),
    .wr_port_en(h_wr_en_w),
    .wr_port_wen(h_wr_wen_w),
    .wr_port_addr(h_wr_addr_w),
    .wr_port_wdata(h_wr_data_w)
  );
  _Lz4Decomp_threads _threads (
    .clk(clk),
    .rst(rst),
    .h_rd_data_w(h_rd_data_w),
    .in_data(in_data),
    .in_last(in_last),
    .in_valid(in_valid),
    .out_ready(out_ready),
    .h_rd_addr_w(h_rd_addr_w),
    .h_rd_en_w(h_rd_en_w),
    .h_wr_addr_w(h_wr_addr_w),
    .h_wr_data_w(h_wr_data_w),
    .h_wr_en_w(h_wr_en_w),
    .h_wr_wen_w(h_wr_wen_w),
    .in_ready(in_ready),
    .out_data(out_data),
    .out_last(out_last),
    .out_valid(out_valid),
    .byte_last_r(byte_last_r),
    .byte_r(byte_r),
    .ext_r(ext_r),
    .lit_len_r(lit_len_r),
    .mat_len_r(mat_len_r),
    .offset_r(offset_r),
    .rd_ptr_r(rd_ptr_r),
    .saw_last_r(saw_last_r),
    .token_hi_r(token_hi_r),
    .token_lo_r(token_lo_r),
    .wr_ptr_r(wr_ptr_r)
  );

endmodule

