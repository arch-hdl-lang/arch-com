// =============================================================================
// BufMgr — 256-Queue Shared Buffer Manager
//
// 16K entries x 128-bit data pool, 256 dynamically-sharing queues.
// Singly-linked list per queue; all pointers + data in SRAM.
// Head/tail pointers in flops.  Simultaneous enqueue + dequeue every cycle.
//
// Sub-components (separate files):
//   DataMem       — 16K x 128b  simple_dual  sync_out (2-cycle read)
//   NextPtrMem    — 16K x 14b   simple_dual  sync_out (2-cycle read)
//   FreeListBank  — 8K x 14b    simple_dual  sync_out (2-cycle read) x 2
//
// Free-list uses 2-bank interleaving to hide 2-cycle read latency:
//   Bank0 holds even-index entries, Bank1 holds odd-index entries.
//   Alternating prefetch reads → 1 free slot per cycle once primed.
//   Results stored in a 4-entry flop FIFO for immediate access at EQ0.
//
// Enqueue pipeline: EQ0 → EQ1 → EQ2  (3 stages)
// Dequeue pipeline: DQ0 → DQ1 → DQ2  (3 stages)
// =============================================================================
module BufMgr #(
  parameter int DEPTH = 16384,
  parameter int QUEUE_COUNT = 256,
  parameter int DATA_WIDTH = 128,
  parameter int PTR_WIDTH = 14,
  parameter int QN_WIDTH = 8
) (
  input logic clk,
  input logic rst,
  input logic enqueue_valid,
  input logic [8-1:0] enqueue_queue_number,
  input logic [128-1:0] enqueue_data,
  input logic dequeue_valid,
  input logic [8-1:0] dequeue_queue_number,
  output logic dequeue_resp_valid,
  output logic [128-1:0] dequeue_data,
  output logic [15-1:0] free_count_out,
  output logic init_done
);

  // ── Flop arrays (256 entries) ──
  logic [14-1:0] [0:256-1] head_arr = 0;
  logic [14-1:0] [0:256-1] tail_arr = 0;
  logic [15-1:0] [0:256-1] count_arr = 0;
  // ── Init FSM ──
  logic setup_done = 0;
  logic [14-1:0] setup_ctr = 0;
  // ── Free-list state ──
  logic [15-1:0] free_count = 0;
  logic [14-1:0] fl_wr_ptr = 0;
  // Prefetch pipeline: tracks reads in-flight through sync_out latency
  logic [14-1:0] fl_rd_ptr = 0;
  logic fl_pipe_d1 = 0;
  logic fl_pipe_d2 = 0;
  logic fl_pipe_bank_d1 = 0;
  logic fl_pipe_bank_d2 = 0;
  // 4-entry flop FIFO for prefetched free slots (circular buffer)
  logic [14-1:0] fl_buf0 = 0;
  logic [14-1:0] fl_buf1 = 0;
  logic [14-1:0] fl_buf2 = 0;
  logic [14-1:0] fl_buf3 = 0;
  logic [3-1:0] fl_buf_count = 0;
  logic [2-1:0] fl_buf_wr = 0;
  logic [2-1:0] fl_buf_rd = 0;
  // ── Enqueue pipeline registers ──
  logic eq1_valid = 0;
  logic [8-1:0] eq1_qn = 0;
  logic [128-1:0] eq1_data = 0;
  logic [14-1:0] eq1_old_tail = 0;
  logic eq1_was_empty = 0;
  logic [14-1:0] eq1_alloc_slot = 0;
  logic eq2_valid = 0;
  logic [8-1:0] eq2_qn = 0;
  logic [128-1:0] eq2_data = 0;
  logic [14-1:0] eq2_old_tail = 0;
  logic eq2_was_empty = 0;
  logic [14-1:0] eq2_alloc_slot = 0;
  // ── Dequeue pipeline registers ──
  logic dq1_valid = 0;
  logic [8-1:0] dq1_qn = 0;
  logic [14-1:0] dq1_old_head = 0;
  logic dq2_valid = 0;
  logic [8-1:0] dq2_qn = 0;
  logic [14-1:0] dq2_old_head = 0;
  // ── SRAM output wires (no reset — driven by SRAM outputs) ──
  logic [14-1:0] fbank0_rd_data = 0;
  logic [14-1:0] fbank1_rd_data = 0;
  logic [128-1:0] data_rd_data = 0;
  logic [14-1:0] next_ptr_rd_data = 0;
  // ── Combinational: prefetch buffer output (4-entry mux) ──
  logic [14-1:0] alloc_slot;
  assign alloc_slot = ((fl_buf_rd == 2'd0)) ? (fl_buf0) : (((fl_buf_rd == 2'd1)) ? (fl_buf1) : (((fl_buf_rd == 2'd2)) ? (fl_buf2) : (fl_buf3)));
  // ── Combinational: bypass logic ──
  logic [14-1:0] eq0_tail_bypassed;
  assign eq0_tail_bypassed = ((eq1_valid && (eq1_qn == enqueue_queue_number))) ? (eq1_alloc_slot) : (((eq2_valid && (eq2_qn == enqueue_queue_number))) ? (eq2_alloc_slot) : (tail_arr[enqueue_queue_number]));
  logic [15-1:0] eq0_count_raw;
  assign eq0_count_raw = count_arr[enqueue_queue_number];
  logic [15-1:0] eq0_count_adj_eq2;
  assign eq0_count_adj_eq2 = ((eq2_valid && (eq2_qn == enqueue_queue_number))) ? (15'((eq0_count_raw + 14'd1))) : (eq0_count_raw);
  logic [15-1:0] eq0_count_adj_eq1;
  assign eq0_count_adj_eq1 = ((eq1_valid && (eq1_qn == enqueue_queue_number))) ? (15'((eq0_count_adj_eq2 + 14'd1))) : (eq0_count_adj_eq2);
  logic eq0_was_empty;
  assign eq0_was_empty = (eq0_count_adj_eq1 == 15'd0);
  logic [14-1:0] dq0_head_bypassed;
  assign dq0_head_bypassed = ((dq2_valid && (dq2_qn == dequeue_queue_number))) ? (next_ptr_rd_data) : (head_arr[dequeue_queue_number]);
  // ── Combinational: free-list bank address/select ──
  logic fl_rd_bank;
  assign fl_rd_bank = 1'(fl_rd_ptr);
  logic [13-1:0] fl_rd_addr;
  assign fl_rd_addr = 13'((fl_rd_ptr >> 1));
  logic fl_wr_bank;
  assign fl_wr_bank = 1'(fl_wr_ptr);
  logic [13-1:0] fl_wr_addr;
  assign fl_wr_addr = 13'((fl_wr_ptr >> 1));
  logic setup_bank;
  assign setup_bank = 1'(setup_ctr);
  logic [13-1:0] setup_addr;
  assign setup_addr = 13'((setup_ctr >> 1));
  // Prefetch control: issue read if buffer + in-flight < 4
  logic [3-1:0] fl_inflight;
  assign fl_inflight = 3'((3'(fl_pipe_d1) + 3'(fl_pipe_d2)));
  logic [3-1:0] fl_pending;
  assign fl_pending = 3'((fl_buf_count + fl_inflight));
  logic fl_do_prefetch;
  assign fl_do_prefetch = (setup_done && (fl_pending != 3'd4));
  // Arriving data from whichever bank was read 2 cycles ago
  logic [14-1:0] fl_arriving_slot;
  assign fl_arriving_slot = (fl_pipe_bank_d2) ? (fbank1_rd_data) : (fbank0_rd_data);
  // ── RAM Instances ──
  DataMem dmem (
    .clk(clk),
    .wr_port_en(eq2_valid),
    .wr_port_addr(eq2_alloc_slot),
    .wr_port_data(eq2_data),
    .rd_port_en((dequeue_valid && setup_done)),
    .rd_port_addr(dq0_head_bypassed),
    .rd_port_data(data_rd_data)
  );
  NextPtrMem nptr (
    .clk(clk),
    .wr_port_en((eq2_valid && (!eq2_was_empty))),
    .wr_port_addr(eq2_old_tail),
    .wr_port_data(eq2_alloc_slot),
    .rd_port_en((dequeue_valid && setup_done)),
    .rd_port_addr(dq0_head_bypassed),
    .rd_port_data(next_ptr_rd_data)
  );
  FreeListBank fbank0 (
    .clk(clk),
    .rd_port_en((fl_do_prefetch && (!fl_rd_bank))),
    .rd_port_addr((setup_done) ? (fl_rd_addr) : (setup_addr)),
    .wr_port_en((((!setup_done) && (!setup_bank)) || ((setup_done && dq2_valid) && (!fl_wr_bank)))),
    .wr_port_addr((setup_done) ? (fl_wr_addr) : (setup_addr)),
    .wr_port_data((setup_done) ? (dq2_old_head) : (setup_ctr)),
    .rd_port_data(fbank0_rd_data)
  );
  FreeListBank fbank1 (
    .clk(clk),
    .rd_port_en((fl_do_prefetch && fl_rd_bank)),
    .rd_port_addr((setup_done) ? (fl_rd_addr) : (setup_addr)),
    .wr_port_en((((!setup_done) && setup_bank) || ((setup_done && dq2_valid) && fl_wr_bank))),
    .wr_port_addr((setup_done) ? (fl_wr_addr) : (setup_addr)),
    .wr_port_data((setup_done) ? (dq2_old_head) : (setup_ctr)),
    .rd_port_data(fbank1_rd_data)
  );
  // ══════════════════════════════════════════════════════════════════════════
  // Clocked logic
  // ══════════════════════════════════════════════════════════════════════════
  always_ff @(posedge clk) begin
    if (rst) begin
      count_arr <= 0;
      dq1_old_head <= 0;
      dq1_qn <= 0;
      dq1_valid <= 0;
      dq2_old_head <= 0;
      dq2_qn <= 0;
      dq2_valid <= 0;
      eq1_alloc_slot <= 0;
      eq1_data <= 0;
      eq1_old_tail <= 0;
      eq1_qn <= 0;
      eq1_valid <= 0;
      eq1_was_empty <= 0;
      eq2_alloc_slot <= 0;
      eq2_data <= 0;
      eq2_old_tail <= 0;
      eq2_qn <= 0;
      eq2_valid <= 0;
      eq2_was_empty <= 0;
      fl_buf0 <= 0;
      fl_buf1 <= 0;
      fl_buf2 <= 0;
      fl_buf3 <= 0;
      fl_buf_count <= 0;
      fl_buf_rd <= 0;
      fl_buf_wr <= 0;
      fl_pipe_bank_d1 <= 0;
      fl_pipe_bank_d2 <= 0;
      fl_pipe_d1 <= 0;
      fl_pipe_d2 <= 0;
      fl_rd_ptr <= 0;
      fl_wr_ptr <= 0;
      free_count <= 0;
      head_arr <= 0;
      setup_ctr <= 0;
      setup_done <= 0;
      tail_arr <= 0;
    end else begin
      if ((!setup_done)) begin
        setup_ctr <= 14'((setup_ctr + 14'd1));
        if ((setup_ctr == 14'd16383)) begin
          setup_done <= 1'd1;
          free_count <= 15'd16384;
          fl_rd_ptr <= 14'd0;
          fl_wr_ptr <= 14'd0;
        end
      end
      fl_pipe_d2 <= fl_pipe_d1;
      fl_pipe_bank_d2 <= fl_pipe_bank_d1;
      if (fl_do_prefetch) begin
        fl_pipe_d1 <= 1'd1;
        fl_pipe_bank_d1 <= fl_rd_bank;
        fl_rd_ptr <= 14'((fl_rd_ptr + 14'd1));
      end else begin
        fl_pipe_d1 <= 1'd0;
      end
      if (fl_pipe_d2) begin
        if ((fl_buf_wr == 2'd0)) begin
          fl_buf0 <= fl_arriving_slot;
        end
        if ((fl_buf_wr == 2'd1)) begin
          fl_buf1 <= fl_arriving_slot;
        end
        if ((fl_buf_wr == 2'd2)) begin
          fl_buf2 <= fl_arriving_slot;
        end
        if ((fl_buf_wr == 2'd3)) begin
          fl_buf3 <= fl_arriving_slot;
        end
        fl_buf_wr <= 2'((fl_buf_wr + 2'd1));
        fl_buf_count <= 3'((fl_buf_count + 3'd1));
      end
      if ((enqueue_valid && setup_done)) begin
        eq1_valid <= 1'd1;
        eq1_qn <= enqueue_queue_number;
        eq1_data <= enqueue_data;
        eq1_old_tail <= eq0_tail_bypassed;
        eq1_was_empty <= eq0_was_empty;
        eq1_alloc_slot <= alloc_slot;
        fl_buf_rd <= 2'((fl_buf_rd + 2'd1));
        fl_buf_count <= 3'((fl_buf_count - 3'd1));
        free_count <= 15'((free_count - 15'd1));
      end else begin
        eq1_valid <= 1'd0;
      end
      if (((fl_pipe_d2 && enqueue_valid) && setup_done)) begin
        fl_buf_count <= fl_buf_count;
      end
      eq2_valid <= eq1_valid;
      eq2_qn <= eq1_qn;
      eq2_data <= eq1_data;
      eq2_old_tail <= eq1_old_tail;
      eq2_was_empty <= eq1_was_empty;
      eq2_alloc_slot <= eq1_alloc_slot;
      if (eq2_valid) begin
        tail_arr[eq2_qn] <= eq2_alloc_slot;
        count_arr[eq2_qn] <= 15'((count_arr[eq2_qn] + 15'd1));
        if (eq2_was_empty) begin
          head_arr[eq2_qn] <= eq2_alloc_slot;
        end
      end
      if ((dequeue_valid && setup_done)) begin
        dq1_valid <= 1'd1;
        dq1_qn <= dequeue_queue_number;
        dq1_old_head <= dq0_head_bypassed;
      end else begin
        dq1_valid <= 1'd0;
      end
      dq2_valid <= dq1_valid;
      dq2_qn <= dq1_qn;
      dq2_old_head <= dq1_old_head;
      if (dq2_valid) begin
        head_arr[dq2_qn] <= next_ptr_rd_data;
        count_arr[dq2_qn] <= 15'((count_arr[dq2_qn] - 15'd1));
        fl_wr_ptr <= 14'((fl_wr_ptr + 14'd1));
        free_count <= 15'((free_count + 15'd1));
      end
      if (((eq2_valid && dq2_valid) && (eq2_qn == dq2_qn))) begin
        count_arr[eq2_qn] <= count_arr[eq2_qn];
      end
      if (((enqueue_valid && setup_done) && dq2_valid)) begin
        free_count <= free_count;
      end
    end
  end
  // ── Init FSM: fill both free-list banks ──
  // ── Prefetch pipeline shift ──
  // Issue prefetch read
  // Capture arriving prefetch result into FIFO
  // ── Enqueue Pipeline ──
  // EQ0: consume from prefetch buffer, capture inputs
  // Simultaneous prefetch arrive + enqueue consume: net zero on fl_buf_count
  // EQ1 → EQ2
  // EQ2 commit: update flop arrays
  // ── Dequeue Pipeline ──
  // DQ0: issue SRAM reads
  // DQ1 → DQ2
  // DQ2 commit: return freed slot to free-list bank, update flop arrays
  // ── Simultaneous EQ2 + DQ2 same-queue count fix ──
  // Simultaneous alloc + free: net zero on free_count
  assign dequeue_resp_valid = dq2_valid;
  assign dequeue_data = data_rd_data;
  assign free_count_out = free_count;
  assign init_done = (setup_done && (fl_buf_count >= 3'd2));

endmodule

