// =============================================================================
// BufMgr — 256-Queue Shared Buffer Manager
//
// 16K entries x 128-bit data pool, 256 dynamically-sharing queues.
// Singly-linked list per queue; all pointers + data in SRAM.
// Head/tail pointers in flops.  Simultaneous enqueue + dequeue every cycle.
//
// Sub-components:
//   DataMem      — 16K x 128b  simple_dual  sync_out (2-cycle read)
//   NextPtrMem   — 16K x 14b   simple_dual  sync_out (2-cycle read)
//   FreeListMem  — 16K x 14b   simple_dual  sync_out (2-cycle read)
//
// Enqueue pipeline: EQ0 → EQ1 → EQ2  (3 cycles)
// Dequeue pipeline: DQ0 → DQ1 → DQ2  (3 cycles)
// =============================================================================
// domain SysDomain
//   freq_mhz: 500

// ── Data SRAM: 16K x 128b ──────────────────────────────────────────────────
module DataMem #(
  parameter int DEPTH = 16384,
  parameter int DATA_WIDTH = 128
) (
  input logic clk,
  input logic rd_port_en,
  input logic [14-1:0] rd_port_addr,
  output logic [DATA_WIDTH-1:0] rd_port_data,
  input logic wr_port_en,
  input logic [14-1:0] wr_port_addr,
  input logic [DATA_WIDTH-1:0] wr_port_data
);

  logic [DATA_WIDTH-1:0] mem [0:DEPTH-1];
  logic [DATA_WIDTH-1:0] rd_port_data_r;
  
  always_ff @(posedge clk) begin
    if (wr_port_en)
      mem[wr_port_addr] <= wr_port_data;
    if (rd_port_en)
      rd_port_data_r <= mem[rd_port_addr];
  end
  logic [DATA_WIDTH-1:0] rd_port_data_r2;
  always_ff @(posedge clk) rd_port_data_r2 <= rd_port_data_r;
  assign rd_port_data = rd_port_data_r2;

endmodule

// ── Next-Pointer SRAM: 16K x 14b ──────────────────────────────────────────
module NextPtrMem #(
  parameter int DEPTH = 16384,
  parameter int DATA_WIDTH = 14
) (
  input logic clk,
  input logic rd_port_en,
  input logic [14-1:0] rd_port_addr,
  output logic [DATA_WIDTH-1:0] rd_port_data,
  input logic wr_port_en,
  input logic [14-1:0] wr_port_addr,
  input logic [DATA_WIDTH-1:0] wr_port_data
);

  logic [DATA_WIDTH-1:0] mem [0:DEPTH-1];
  logic [DATA_WIDTH-1:0] rd_port_data_r;
  
  always_ff @(posedge clk) begin
    if (wr_port_en)
      mem[wr_port_addr] <= wr_port_data;
    if (rd_port_en)
      rd_port_data_r <= mem[rd_port_addr];
  end
  logic [DATA_WIDTH-1:0] rd_port_data_r2;
  always_ff @(posedge clk) rd_port_data_r2 <= rd_port_data_r;
  assign rd_port_data = rd_port_data_r2;

endmodule

// ── Free-List SRAM: 16K x 14b (circular FIFO) ─────────────────────────────
module FreeListMem #(
  parameter int DEPTH = 16384,
  parameter int DATA_WIDTH = 14
) (
  input logic clk,
  input logic rd_port_en,
  input logic [14-1:0] rd_port_addr,
  output logic [DATA_WIDTH-1:0] rd_port_data,
  input logic wr_port_en,
  input logic [14-1:0] wr_port_addr,
  input logic [DATA_WIDTH-1:0] wr_port_data
);

  logic [DATA_WIDTH-1:0] mem [0:DEPTH-1];
  logic [DATA_WIDTH-1:0] rd_port_data_r;
  
  always_ff @(posedge clk) begin
    if (wr_port_en)
      mem[wr_port_addr] <= wr_port_data;
    if (rd_port_en)
      rd_port_data_r <= mem[rd_port_addr];
  end
  logic [DATA_WIDTH-1:0] rd_port_data_r2;
  always_ff @(posedge clk) rd_port_data_r2 <= rd_port_data_r;
  assign rd_port_data = rd_port_data_r2;

endmodule

// ── Top-Level Buffer Manager ────────────────────────────────────────────────
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
  input logic [QN_WIDTH-1:0] enqueue_queue_number,
  input logic [DATA_WIDTH-1:0] enqueue_data,
  input logic dequeue_valid,
  input logic [QN_WIDTH-1:0] dequeue_queue_number,
  output logic dequeue_resp_valid,
  output logic [DATA_WIDTH-1:0] dequeue_data,
  output logic [15-1:0] free_count_out,
  output logic init_done
);

  // ── Enqueue interface ──
  // ── Dequeue interface ──
  // ── Status ──
  // ══════════════════════════════════════════════════════════════════════════
  // Internal state — flop arrays
  // ══════════════════════════════════════════════════════════════════════════
  // Head/tail pointer arrays (256 x 14b each)
  logic [PTR_WIDTH-1:0] [0:QUEUE_COUNT-1] head_arr = 0;
  logic [PTR_WIDTH-1:0] [0:QUEUE_COUNT-1] tail_arr = 0;
  logic [15-1:0] [0:QUEUE_COUNT-1] count_arr = 0;
  // Free-list circular FIFO pointers
  logic [PTR_WIDTH-1:0] free_rd_ptr = 0;
  logic [PTR_WIDTH-1:0] free_wr_ptr = 0;
  logic [15-1:0] free_count = 0;
  logic setup_done = 0;
  logic [PTR_WIDTH-1:0] setup_ctr = 0;
  // ── Enqueue pipeline registers ──
  // EQ0 → EQ1
  logic eq1_valid = 0;
  logic [QN_WIDTH-1:0] eq1_qn = 0;
  logic [DATA_WIDTH-1:0] eq1_data = 0;
  logic [PTR_WIDTH-1:0] eq1_old_tail = 0;
  logic eq1_was_empty = 0;
  // EQ1 → EQ2
  logic eq2_valid = 0;
  logic [QN_WIDTH-1:0] eq2_qn = 0;
  logic [DATA_WIDTH-1:0] eq2_data = 0;
  logic [PTR_WIDTH-1:0] eq2_old_tail = 0;
  logic eq2_was_empty = 0;
  // ── Dequeue pipeline registers ──
  // DQ0 → DQ1
  logic dq1_valid = 0;
  logic [QN_WIDTH-1:0] dq1_qn = 0;
  logic [PTR_WIDTH-1:0] dq1_old_head = 0;
  // DQ1 → DQ2
  logic dq2_valid = 0;
  logic [QN_WIDTH-1:0] dq2_qn = 0;
  logic [PTR_WIDTH-1:0] dq2_old_head = 0;
  // ══════════════════════════════════════════════════════════════════════════
  // Let bindings for bypass/forwarding (combinational)
  // ══════════════════════════════════════════════════════════════════════════
  // Bypassed tail for enqueue pipeline
  logic [PTR_WIDTH-1:0] eq0_tail_bypassed;
  assign eq0_tail_bypassed = ((eq2_valid && (eq2_qn == enqueue_queue_number))) ? (free_slot_rd_data) : (tail_arr[enqueue_queue_number]);
  // Bypassed count for enqueue pipeline
  logic [15-1:0] eq0_count_raw;
  assign eq0_count_raw = count_arr[enqueue_queue_number];
  logic [15-1:0] eq0_count_adj_eq2;
  assign eq0_count_adj_eq2 = ((eq2_valid && (eq2_qn == enqueue_queue_number))) ? (15'((eq0_count_raw + 14'd1))) : (eq0_count_raw);
  logic [15-1:0] eq0_count_adj_eq1;
  assign eq0_count_adj_eq1 = ((eq1_valid && (eq1_qn == enqueue_queue_number))) ? (15'((eq0_count_adj_eq2 + 14'd1))) : (eq0_count_adj_eq2);
  logic eq0_was_empty;
  assign eq0_was_empty = (eq0_count_adj_eq1 == 15'd0);
  // Bypassed head for dequeue pipeline
  // Note: dq2_next_ptr comes from NextPtrMem read — available combinationally at DQ2
  // For DQ0, we forward from DQ2 if same queue
  logic [PTR_WIDTH-1:0] dq0_head_bypassed;
  assign dq0_head_bypassed = ((dq2_valid && (dq2_qn == dequeue_queue_number))) ? (next_ptr_rd_data) : (head_arr[dequeue_queue_number]);
  // Bypassed count for dequeue pipeline
  logic [15-1:0] dq0_count_raw;
  assign dq0_count_raw = count_arr[dequeue_queue_number];
  logic [15-1:0] dq0_count_adj_dq2;
  assign dq0_count_adj_dq2 = ((dq2_valid && (dq2_qn == dequeue_queue_number))) ? (15'((dq0_count_raw - 14'd1))) : (dq0_count_raw);
  // ══════════════════════════════════════════════════════════════════════════
  // SRAM read-data wires (declared for inst connections)
  // ══════════════════════════════════════════════════════════════════════════
  // Free-list SRAM read result (the allocated free slot)
  logic [14-1:0] free_slot_rd_data = 0;
  // Data SRAM read result
  logic [128-1:0] data_rd_data = 0;
  // Next-pointer SRAM read result
  logic [14-1:0] next_ptr_rd_data = 0;
  // ══════════════════════════════════════════════════════════════════════════
  // RAM Instances
  // ══════════════════════════════════════════════════════════════════════════
  // ── Data SRAM ──
  DataMem dmem (
    .clk(clk),
    .wr_port_en(eq2_valid),
    .wr_port_addr(free_slot_rd_data),
    .wr_port_data(eq2_data),
    .rd_port_en((dequeue_valid && setup_done)),
    .rd_port_addr(dq0_head_bypassed),
    .rd_port_data(data_rd_data)
  );
  // Write port: enqueue pipeline EQ2 writes payload
  // Read port: dequeue pipeline DQ0 issues read
  // ── Next-Pointer SRAM ──
  NextPtrMem nptr (
    .clk(clk),
    .wr_port_en((eq2_valid && (!eq2_was_empty))),
    .wr_port_addr(eq2_old_tail),
    .wr_port_data(free_slot_rd_data),
    .rd_port_en((dequeue_valid && setup_done)),
    .rd_port_addr(dq0_head_bypassed),
    .rd_port_data(next_ptr_rd_data)
  );
  // Write port: enqueue pipeline EQ2 links old_tail → free_slot
  // Read port: dequeue pipeline DQ0 reads next[head] for new head
  // ── Free-List SRAM ──
  FreeListMem flist (
    .clk(clk),
    .rd_port_en(((enqueue_valid && setup_done) || (!setup_done))),
    .rd_port_addr((setup_done) ? (free_rd_ptr) : (setup_ctr)),
    .wr_port_en(((dq2_valid && setup_done) || (!setup_done))),
    .wr_port_addr((setup_done) ? (free_wr_ptr) : (setup_ctr)),
    .wr_port_data((setup_done) ? (dq2_old_head) : (setup_ctr)),
    .rd_port_data(free_slot_rd_data)
  );
  // Read port: enqueue pipeline EQ0 allocates a slot
  // Write port: dequeue pipeline DQ2 returns freed slot (or init writes)
  // ══════════════════════════════════════════════════════════════════════════
  // Main clocked logic
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
      eq1_data <= 0;
      eq1_old_tail <= 0;
      eq1_qn <= 0;
      eq1_valid <= 0;
      eq1_was_empty <= 0;
      eq2_data <= 0;
      eq2_old_tail <= 0;
      eq2_qn <= 0;
      eq2_valid <= 0;
      eq2_was_empty <= 0;
      free_count <= 0;
      free_rd_ptr <= 0;
      free_wr_ptr <= 0;
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
          free_rd_ptr <= 14'd0;
          free_wr_ptr <= 14'd0;
        end
      end
      if ((enqueue_valid && setup_done)) begin
        eq1_valid <= 1'd1;
        eq1_qn <= enqueue_queue_number;
        eq1_data <= enqueue_data;
        eq1_old_tail <= eq0_tail_bypassed;
        eq1_was_empty <= eq0_was_empty;
        free_rd_ptr <= 14'((free_rd_ptr + 14'd1));
        free_count <= 15'((free_count - 15'd1));
      end else begin
        eq1_valid <= 1'd0;
      end
      eq2_valid <= eq1_valid;
      eq2_qn <= eq1_qn;
      eq2_data <= eq1_data;
      eq2_old_tail <= eq1_old_tail;
      eq2_was_empty <= eq1_was_empty;
      if (eq2_valid) begin
        tail_arr[eq2_qn] <= free_slot_rd_data;
        count_arr[eq2_qn] <= 15'((count_arr[eq2_qn] + 15'd1));
        if (eq2_was_empty) begin
          head_arr[eq2_qn] <= free_slot_rd_data;
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
        free_wr_ptr <= 14'((free_wr_ptr + 14'd1));
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
  // ── Reset / Init FSM ──────────────────────────────────────────────────
  // Fill free list: write FreeListMem[i] = i for i in 0..16383
  // ── Enqueue Pipeline ──────────────────────────────────────────────────
  // EQ0: capture inputs, issue free-list read
  // EQ1 → EQ2: propagate pipeline regs, free_slot not yet available
  // free_slot_rd_data will be loaded from SRAM read result in comb block
  // EQ2: commit — SRAM writes happen via inst connections above
  // Update flop arrays
  // ── Dequeue Pipeline ──────────────────────────────────────────────────
  // DQ0: capture inputs, issue SRAM reads
  // DQ1 → DQ2: propagate
  // DQ2: commit — SRAM writes happen via inst connections above
  // ── Simultaneous enqueue + dequeue count fix ──
  // If EQ2 and DQ2 both commit to the same queue in the same cycle,
  // the count updates above would double-apply. Fix: net zero change.
  // Similarly fix free_count if both alloc and free happen same cycle
  // ══════════════════════════════════════════════════════════════════════════
  // Combinational outputs
  // ══════════════════════════════════════════════════════════════════════════
  assign dequeue_resp_valid = dq2_valid;
  assign dequeue_data = data_rd_data;
  assign free_count_out = free_count;
  assign init_done = setup_done;

endmodule

// Dequeue output
// Status
