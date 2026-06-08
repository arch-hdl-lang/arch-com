// LZ4 block-format decompressor (byte-stream FSM).
//
// Accepts a raw LZ4 block byte-stream via AXI-S (in_valid/in_ready/
// in_data/in_last) and emits the decoded byte stream via AXI-S
// (out_valid/out_ready/out_data/out_last).  Caller must strip the
// 4-byte frame header before feeding this block.
//
// LZ4 sequence layout per token byte:
//   [7:4] = literal-length base  (if 0xF, read extension bytes until < 0xFF)
//   [3:0] = match-length-base-4  (if 0xF, read extension bytes until < 0xFF)
//   then: literal bytes (count = lit_len)
//   then: 2-byte LE match offset  (omitted for the LAST sequence)
//   then: match copy (count = match_len, minimum 4)
//
// History ring buffer is 65536 bytes (power-of-2 for free wrap-around).
// The design uses a phase register instead of nested do-while loops to
// comply with the ARCH thread constraint that do-until bodies must not
// contain wait-until or for-loop statements.
// Simple-dual-port, latency-0 (async read) history ring buffer.
// Decoder phase tags.  Each phase handles exactly one handshake event
// per thread iteration; the implicit thread loop drives the state machine.
// consume token byte, parse nibbles
// consume literal-length extension bytes (nibble was 0xF)
// consume + forward one literal byte
// consume match-offset low byte
// consume match-offset high byte; compute match base address
// consume match-length extension bytes (nibble was 0xF)
// set history read address (one-cycle address-setup bubble)
// emit one decoded match byte (back-pressure via do-until)

// Moved to top to satisfy forward-reference requirement in Verilator.
typedef enum logic [2:0] {
  P_TOKEN = 3'd0,
  P_EXT_LIT = 3'd1,
  P_COPY_LIT = 3'd2,
  P_OFF_LO = 3'd3,
  P_OFF_HI = 3'd4,
  P_EXT_MATCH = 3'd5,
  P_MATCH_RD = 3'd6,
  P_MATCH_OUT = 3'd7
} LZ4Phase;

module _Lz4BlockDecomp_threads #(
  localparam [4:0] _t0_S0_dispatch = 0,
  localparam [4:0] _t0_S1_wait_until = 1,
  localparam [4:0] _t0_S2_action = 2,
  localparam [4:0] _t0_S3_dispatch = 3,
  localparam [4:0] _t0_S4_wait_until = 4,
  localparam [4:0] _t0_S5_action = 5,
  localparam [4:0] _t0_S6_dispatch = 6,
  localparam [4:0] _t0_S7_wait_until = 7,
  localparam [4:0] _t0_S8_action = 8,
  localparam [4:0] _t0_S9_dispatch = 9,
  localparam [4:0] _t0_S10_wait_until = 10,
  localparam [4:0] _t0_S11_action = 11,
  localparam [4:0] _t0_S12_dispatch = 12,
  localparam [4:0] _t0_S13_wait_until = 13,
  localparam [4:0] _t0_S14_action = 14,
  localparam [4:0] _t0_S15_dispatch = 15,
  localparam [4:0] _t0_S16_wait_until = 16,
  localparam [4:0] _t0_S17_action = 17,
  localparam [4:0] _t0_S18_dispatch = 18,
  localparam [4:0] _t0_S19_action = 19,
  localparam [4:0] _t0_S20_dispatch = 20,
  localparam [4:0] _t0_S21_action = 21
) (
  input logic clk,
  input logic rst,
  input logic [7:0] hist_rd_data_w,
  input logic [7:0] in_data,
  input logic in_last,
  input logic in_valid,
  input logic out_ready,
  output logic [15:0] hist_wr_addr_w,
  output logic [7:0] hist_wr_data_w,
  output logic hist_wr_en_w,
  output logic in_ready,
  output logic [7:0] out_data,
  output logic out_last,
  output logic out_valid,
  output logic [15:0] hist_rd_addr_r,
  output logic [15:0] lit_ctr_r,
  output logic [15:0] lit_len_r,
  output logic [15:0] match_base_r,
  output logic [15:0] match_ctr_r,
  output logic [15:0] match_len_r,
  output logic [15:0] match_off_r,
  output LZ4Phase phase_r,
  output logic saw_last_r,
  output logic [15:0] wr_ptr_r
);

  logic [4:0] _t0_state = 0;
  always_comb begin
    hist_wr_addr_w = 0;
    hist_wr_data_w = 0;
    hist_wr_en_w = 0;
    in_ready = 0;
    out_data = 0;
    out_last = 0;
    out_valid = 0;
    // Decoder state registers
    // History buffer wires — single-driver via thread default comb.
    in_ready = 1'b0;
    out_valid = 1'b0;
    out_data = 8'd0;
    out_last = 1'b0;
    hist_wr_en_w = 1'b0;
    hist_wr_addr_w = 16'd0;
    hist_wr_data_w = 8'd0;
    if (_t0_state == _t0_S2_action) begin
      // ── P_TOKEN ──────────────────────────────────────────────────────────
      // Consume one token byte.  Parse high nibble → lit_len base,
      // low nibble → match_len base.  Advance phase based on nibble values.
      in_ready = 1'b1;
    end
    if (_t0_state == _t0_S5_action) begin
      // in_data[7:4]==0 and in_last: last sequence with 0 literals and no
      // match → stay P_TOKEN, decompressor idles until next block.
      // ── P_EXT_LIT ────────────────────────────────────────────────────────
      // Each iteration consumes one extension byte, adding it to lit_len_r.
      // Stay until byte < 0xFF (LZ4 convention).
      in_ready = 1'b1;
    end
    if (_t0_state == _t0_S8_action) begin
      // ── P_COPY_LIT ───────────────────────────────────────────────────────
      // Pass one literal byte to output and write it to the history buffer.
      // Phase transitions: last literal + EOS → P_TOKEN;
      //                    last literal + !EOS → P_OFF_LO;
      //                    not last literal    → stay P_COPY_LIT.
      in_ready = 1'b1;
      out_valid = 1'b1;
      out_data = in_data;
      out_last = 16'(lit_ctr_r + 16'd1) >= lit_len_r && (saw_last_r || in_last);
      hist_wr_en_w = 1'b1;
      hist_wr_addr_w = wr_ptr_r;
      hist_wr_data_w = in_data;
    end
    if (_t0_state == _t0_S11_action) begin
      // ── P_OFF_LO ─────────────────────────────────────────────────────────
      in_ready = 1'b1;
    end
    if (_t0_state == _t0_S14_action) begin
      // ── P_OFF_HI ─────────────────────────────────────────────────────────
      // Complete the 16-bit little-endian match offset.  Pre-compute the
      // match base address (wr_ptr − full_offset) so P_MATCH_RD can use
      // match_base_r directly without a further pipeline bubble.
      in_ready = 1'b1;
    end
    if (_t0_state == _t0_S17_action) begin
      // ── P_EXT_MATCH ──────────────────────────────────────────────────────
      in_ready = 1'b1;
    end
    if (_t0_state == _t0_S21_action) begin
      // ── P_MATCH_RD ───────────────────────────────────────────────────────
      // One-cycle address-setup bubble: register hist_rd_addr_r so the
      // latency-0 async RAM read is valid in the following P_MATCH_OUT cycle.
      // ── P_MATCH_OUT ──────────────────────────────────────────────────────
      // Emit one decoded match byte (from history) and append it to history.
      // Back-pressure: hold output valid until out_ready via do-until.
      out_valid = 1'b1;
      out_data = hist_rd_data_w;
      hist_wr_en_w = 1'b1;
      hist_wr_addr_w = wr_ptr_r;
      hist_wr_data_w = hist_rd_data_w;
    end
  end
  always_ff @(posedge clk) begin
    if (rst) begin
      _t0_state <= 0;
      hist_rd_addr_r <= 0;
      lit_ctr_r <= 0;
      lit_len_r <= 0;
      match_base_r <= 0;
      match_ctr_r <= 0;
      match_len_r <= 4;
      match_off_r <= 0;
      phase_r <= P_TOKEN;
      saw_last_r <= 1'b0;
      wr_ptr_r <= 0;
    end else begin
      if (_t0_state == _t0_S0_dispatch) begin
        if (phase_r == P_TOKEN) begin
          _t0_state <= _t0_S1_wait_until;
        end
        if (!(phase_r == P_TOKEN)) begin
          _t0_state <= _t0_S3_dispatch;
        end
      end
      if (_t0_state == _t0_S1_wait_until) begin
        if (in_valid) begin
          _t0_state <= _t0_S2_action;
        end
      end
      if (_t0_state == _t0_S2_action) begin
        lit_len_r <= 16'($unsigned(in_data[7:4]));
        match_len_r <= 16'(16'($unsigned(in_data[3:0])) + 16'd4);
        lit_ctr_r <= 16'd0;
        match_ctr_r <= 16'd0;
        saw_last_r <= in_last;
        if (in_data[7:4] == 4'd15) begin
          phase_r <= P_EXT_LIT;
        end else if (in_data[7:4] != 4'd0) begin
          phase_r <= P_COPY_LIT;
        end else if (!in_last) begin
          phase_r <= P_OFF_LO;
        end
        if (1'b1) begin
          _t0_state <= _t0_S0_dispatch;
        end
      end
      if (_t0_state == _t0_S3_dispatch) begin
        if (phase_r == P_EXT_LIT) begin
          _t0_state <= _t0_S4_wait_until;
        end
        if (!(phase_r == P_EXT_LIT)) begin
          _t0_state <= _t0_S6_dispatch;
        end
      end
      if (_t0_state == _t0_S4_wait_until) begin
        if (in_valid) begin
          _t0_state <= _t0_S5_action;
        end
      end
      if (_t0_state == _t0_S5_action) begin
        lit_len_r <= 16'(lit_len_r + 16'($unsigned(in_data)));
        saw_last_r <= saw_last_r || in_last;
        if (in_data < 8'd255) begin
          phase_r <= P_COPY_LIT;
        end
        if (1'b1) begin
          _t0_state <= _t0_S0_dispatch;
        end
      end
      if (_t0_state == _t0_S6_dispatch) begin
        if (phase_r == P_COPY_LIT) begin
          _t0_state <= _t0_S7_wait_until;
        end
        if (!(phase_r == P_COPY_LIT)) begin
          _t0_state <= _t0_S9_dispatch;
        end
      end
      if (_t0_state == _t0_S7_wait_until) begin
        if (in_valid && out_ready) begin
          _t0_state <= _t0_S8_action;
        end
      end
      if (_t0_state == _t0_S8_action) begin
        wr_ptr_r <= 16'(wr_ptr_r + 16'd1);
        lit_ctr_r <= 16'(lit_ctr_r + 16'd1);
        saw_last_r <= saw_last_r || in_last;
        if (16'(lit_ctr_r + 16'd1) >= lit_len_r) begin
          if (saw_last_r || in_last) begin
            phase_r <= P_TOKEN;
          end else begin
            phase_r <= P_OFF_LO;
          end
        end
        if (1'b1) begin
          _t0_state <= _t0_S0_dispatch;
        end
      end
      if (_t0_state == _t0_S9_dispatch) begin
        if (phase_r == P_OFF_LO) begin
          _t0_state <= _t0_S10_wait_until;
        end
        if (!(phase_r == P_OFF_LO)) begin
          _t0_state <= _t0_S12_dispatch;
        end
      end
      if (_t0_state == _t0_S10_wait_until) begin
        if (in_valid) begin
          _t0_state <= _t0_S11_action;
        end
      end
      if (_t0_state == _t0_S11_action) begin
        match_off_r <= 16'($unsigned(in_data));
        phase_r <= P_OFF_HI;
        if (1'b1) begin
          _t0_state <= _t0_S0_dispatch;
        end
      end
      if (_t0_state == _t0_S12_dispatch) begin
        if (phase_r == P_OFF_HI) begin
          _t0_state <= _t0_S13_wait_until;
        end
        if (!(phase_r == P_OFF_HI)) begin
          _t0_state <= _t0_S15_dispatch;
        end
      end
      if (_t0_state == _t0_S13_wait_until) begin
        if (in_valid) begin
          _t0_state <= _t0_S14_action;
        end
      end
      if (_t0_state == _t0_S14_action) begin
        match_off_r <= match_off_r | 16'($unsigned(in_data)) << 8;
        match_base_r <= (16 > $bits(match_off_r | 16'($unsigned(in_data)) << 8) ? 16 : $bits(match_off_r | 16'($unsigned(in_data)) << 8))'(wr_ptr_r - (match_off_r | 16'($unsigned(in_data)) << 8));
        match_ctr_r <= 16'd0;
        if (match_len_r == 16'd19) begin
          phase_r <= P_EXT_MATCH;
        end else begin
          phase_r <= P_MATCH_RD;
        end
        if (1'b1) begin
          _t0_state <= _t0_S0_dispatch;
        end
      end
      if (_t0_state == _t0_S15_dispatch) begin
        if (phase_r == P_EXT_MATCH) begin
          _t0_state <= _t0_S16_wait_until;
        end
        if (!(phase_r == P_EXT_MATCH)) begin
          _t0_state <= _t0_S18_dispatch;
        end
      end
      if (_t0_state == _t0_S16_wait_until) begin
        if (in_valid) begin
          _t0_state <= _t0_S17_action;
        end
      end
      if (_t0_state == _t0_S17_action) begin
        match_len_r <= 16'(match_len_r + 16'($unsigned(in_data)));
        if (in_data < 8'd255) begin
          phase_r <= P_MATCH_RD;
        end
        if (1'b1) begin
          _t0_state <= _t0_S0_dispatch;
        end
      end
      if (_t0_state == _t0_S18_dispatch) begin
        if (phase_r == P_MATCH_RD) begin
          _t0_state <= _t0_S19_action;
        end
        if (!(phase_r == P_MATCH_RD)) begin
          _t0_state <= _t0_S20_dispatch;
        end
      end
      if (_t0_state == _t0_S19_action) begin
        hist_rd_addr_r <= 16'(match_base_r + match_ctr_r);
        phase_r <= P_MATCH_OUT;
        if (1'b1) begin
          _t0_state <= _t0_S0_dispatch;
        end
      end
      if (_t0_state == _t0_S20_dispatch) begin
        if (phase_r == P_MATCH_OUT) begin
          _t0_state <= _t0_S21_action;
        end
        if (!(phase_r == P_MATCH_OUT)) begin
          _t0_state <= _t0_S0_dispatch;
        end
      end
      if (_t0_state == _t0_S21_action) begin
        if (out_ready) begin
          wr_ptr_r <= 16'(wr_ptr_r + 16'd1);
        end
        if (out_ready) begin
          match_ctr_r <= 16'(match_ctr_r + 16'd1);
        end
        if (out_ready) begin
          if (16'(match_ctr_r + 16'd1) >= match_len_r) begin
            phase_r <= P_TOKEN;
          end else begin
            phase_r <= P_MATCH_RD;
          end
        end
        if (out_ready) begin
          _t0_state <= _t0_S0_dispatch;
        end
      end
    end
  end

endmodule

// domain SysDomain
//   freq_mhz: 100

module HistBuf #(
  parameter int DEPTH = 256,
  parameter int DATA_WIDTH = 8
) (
  input logic clk,
  input logic wr_port_en,
  input logic [15:0] wr_port_addr,
  input logic [7:0] wr_port_data,
  input logic [15:0] rd_port_addr,
  output logic [7:0] rd_port_data
);

  logic [DATA_WIDTH-1:0] mem [0:DEPTH-1];
  
  assign rd_port_data = mem[rd_port_addr];
  
  always_ff @(posedge clk) begin
    if (wr_port_en)
      mem[wr_port_addr] <= wr_port_data;
  end

endmodule

// typedef moved to top of file.

module Lz4BlockDecomp (
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

  LZ4Phase phase_r;
  logic [15:0] lit_len_r;
  logic [15:0] lit_ctr_r;
  logic [15:0] match_len_r;
  logic [15:0] match_ctr_r;
  logic [15:0] match_off_r;
  logic [15:0] match_base_r;
  logic saw_last_r;
  logic [15:0] wr_ptr_r;
  logic [15:0] hist_rd_addr_r;
  logic hist_wr_en_w;
  logic [15:0] hist_wr_addr_w;
  logic [7:0] hist_wr_data_w;
  logic [7:0] hist_rd_data_w;
  HistBuf hist (
    .clk(clk),
    .wr_port_en(hist_wr_en_w),
    .wr_port_addr(hist_wr_addr_w),
    .wr_port_data(hist_wr_data_w),
    .rd_port_addr(hist_rd_addr_r),
    .rd_port_data(hist_rd_data_w)
  );
  _Lz4BlockDecomp_threads _threads (
    .clk(clk),
    .rst(rst),
    .hist_rd_data_w(hist_rd_data_w),
    .in_data(in_data),
    .in_last(in_last),
    .in_valid(in_valid),
    .out_ready(out_ready),
    .hist_wr_addr_w(hist_wr_addr_w),
    .hist_wr_data_w(hist_wr_data_w),
    .hist_wr_en_w(hist_wr_en_w),
    .in_ready(in_ready),
    .out_data(out_data),
    .out_last(out_last),
    .out_valid(out_valid),
    .hist_rd_addr_r(hist_rd_addr_r),
    .lit_ctr_r(lit_ctr_r),
    .lit_len_r(lit_len_r),
    .match_base_r(match_base_r),
    .match_ctr_r(match_ctr_r),
    .match_len_r(match_len_r),
    .match_off_r(match_off_r),
    .phase_r(phase_r),
    .saw_last_r(saw_last_r),
    .wr_ptr_r(wr_ptr_r)
  );

endmodule

