// leading_zero_cnt: trailing zero counter (REVERSE=1) / leading zero counter (REVERSE=0)
// Finds the index of the first set bit: from LSB (REVERSE=1) or from MSB (REVERSE=0).
module leading_zero_cnt #(
  parameter DATA_WIDTH = 32,
  parameter REVERSE    = 0,
  parameter OUT_WIDTH  = $clog2(DATA_WIDTH)
) (
  input  wire [DATA_WIDTH-1:0] data,
  output reg  [OUT_WIDTH-1:0]  leading_zeros,
  output wire                  all_zeros
);
  integer k;
  integer idx;
  integer found;
  always @(*) begin : lzc_scan
    idx   = 0;
    found = 0;
    if (REVERSE) begin
      for (k = 0; k < DATA_WIDTH; k = k + 1) begin
        if (!found && data[k]) begin
          idx   = k;
          found = 1;
        end
      end
    end else begin
      for (k = DATA_WIDTH-1; k >= 0; k = k - 1) begin
        if (!found && data[k]) begin
          idx   = k;
          found = 1;
        end
      end
    end
    leading_zeros = idx[OUT_WIDTH-1:0];
  end

  assign all_zeros = (data == {DATA_WIDTH{1'b0}});

endmodule


// cache_mshr: Miss Status Handling Registers
// Tracks multiple outstanding cache misses with a linked-list structure.
// Supports: allocate, finalize, fill (addr lookup), dequeue (linked-list traversal).
module cache_mshr #(
  parameter INSTANCE_ID        = "mo_mshr",
  parameter MSHR_SIZE          = 32,
  parameter CS_LINE_ADDR_WIDTH = 10,
  parameter WORD_SEL_WIDTH     = 4,
  parameter WORD_SIZE          = 4,
  // Derived parameters
  parameter MSHR_ADDR_WIDTH    = $clog2(MSHR_SIZE),
  parameter TAG_WIDTH          = 32 - (CS_LINE_ADDR_WIDTH + $clog2(WORD_SIZE) + WORD_SEL_WIDTH),
  parameter CS_WORD_WIDTH      = WORD_SIZE * 8,
  parameter DATA_WIDTH         = WORD_SEL_WIDTH + WORD_SIZE + CS_WORD_WIDTH + TAG_WIDTH
) (
  input  wire                          clk,
  input  wire                          reset,

  // Memory fill interface
  input  wire                          fill_valid,
  input  wire [MSHR_ADDR_WIDTH-1:0]    fill_id,
  output wire [CS_LINE_ADDR_WIDTH-1:0] fill_addr,

  // Dequeue interface
  output reg                           dequeue_valid,
  output reg  [CS_LINE_ADDR_WIDTH-1:0] dequeue_addr,
  output reg                           dequeue_rw,
  output reg  [DATA_WIDTH-1:0]         dequeue_data,
  output reg  [MSHR_ADDR_WIDTH-1:0]    dequeue_id,
  input  wire                          dequeue_ready,

  // Allocate interface
  input  wire                          allocate_valid,
  input  wire [CS_LINE_ADDR_WIDTH-1:0] allocate_addr,
  input  wire                          allocate_rw,
  input  wire [DATA_WIDTH-1:0]         allocate_data,
  output reg  [MSHR_ADDR_WIDTH-1:0]    allocate_id,
  output reg                           allocate_pending,
  output reg  [MSHR_ADDR_WIDTH-1:0]    allocate_previd,
  output wire                          allocate_ready,

  // Finalize interface
  input  wire                          finalize_valid,
  input  wire [MSHR_ADDR_WIDTH-1:0]    finalize_id
);

  // -----------------------------------------------------------------------
  // Entry metadata registers
  // -----------------------------------------------------------------------
  reg                          entry_valid    [0:MSHR_SIZE-1];
  reg [CS_LINE_ADDR_WIDTH-1:0] entry_addr     [0:MSHR_SIZE-1];
  reg                          entry_write    [0:MSHR_SIZE-1];
  reg                          entry_has_next [0:MSHR_SIZE-1];
  reg [MSHR_ADDR_WIDTH-1:0]    entry_next_idx [0:MSHR_SIZE-1];

  // SP RAM for request data (write latency 1, read combinational)
  reg [DATA_WIDTH-1:0] ram [0:MSHR_SIZE-1];

  // -----------------------------------------------------------------------
  // Build valid_inv for LZC1: find first free slot
  // -----------------------------------------------------------------------
  wire [MSHR_SIZE-1:0] valid_inv;
  genvar gi;
  generate
    for (gi = 0; gi < MSHR_SIZE; gi = gi + 1) begin : g_valid_inv
      assign valid_inv[gi] = ~entry_valid[gi];
    end
  endgenerate

  // -----------------------------------------------------------------------
  // LZC #1: find first free slot index
  // -----------------------------------------------------------------------
  wire [MSHR_ADDR_WIDTH-1:0] alloc_idx;
  wire                        full_flag;

  leading_zero_cnt #(
    .DATA_WIDTH (MSHR_SIZE),
    .REVERSE    (1),
    .OUT_WIDTH  (MSHR_ADDR_WIDTH)
  ) u_alloc_lzc (
    .data          (valid_inv),
    .leading_zeros (alloc_idx),
    .all_zeros     (full_flag)
  );

  assign allocate_ready = ~full_flag;

  // -----------------------------------------------------------------------
  // Build match_no_next: valid entries with same addr and no next pointer,
  // only when an allocate is firing
  // -----------------------------------------------------------------------
  wire allocate_fire = allocate_valid & ~full_flag;

  reg [MSHR_SIZE-1:0] match_no_next;
  integer mi;
  always @(*) begin : build_match_no_next
    for (mi = 0; mi < MSHR_SIZE; mi = mi + 1) begin
      match_no_next[mi] = entry_valid[mi]
                          & (entry_addr[mi] == allocate_addr)
                          & ~entry_has_next[mi]
                          & allocate_fire;
    end
  end

  // -----------------------------------------------------------------------
  // LZC #2: find tail of existing chain for same cache line address
  // -----------------------------------------------------------------------
  wire [MSHR_ADDR_WIDTH-1:0] prev_idx;
  wire                        prev_no_hit;

  leading_zero_cnt #(
    .DATA_WIDTH (MSHR_SIZE),
    .REVERSE    (1),
    .OUT_WIDTH  (MSHR_ADDR_WIDTH)
  ) u_prev_lzc (
    .data          (match_no_next),
    .leading_zeros (prev_idx),
    .all_zeros     (prev_no_hit)
  );

  // fill_addr: combinational read from entry_addr at fill_id
  assign fill_addr = entry_addr[fill_id];

  // -----------------------------------------------------------------------
  // Dequeue cursor
  // -----------------------------------------------------------------------
  reg                       dq_active;
  reg [MSHR_ADDR_WIDTH-1:0] dq_cur_idx;

  // -----------------------------------------------------------------------
  // Sequential state
  // -----------------------------------------------------------------------
  integer ri;
  always @(posedge clk) begin : ff_state
    if (reset) begin
      allocate_id       <= {MSHR_ADDR_WIDTH{1'b0}};
      allocate_pending  <= 1'b0;
      allocate_previd   <= {MSHR_ADDR_WIDTH{1'b0}};
      dequeue_valid     <= 1'b0;
      dequeue_addr      <= {CS_LINE_ADDR_WIDTH{1'b0}};
      dequeue_rw        <= 1'b0;
      dequeue_data      <= {DATA_WIDTH{1'b0}};
      dequeue_id        <= {MSHR_ADDR_WIDTH{1'b0}};
      dq_active         <= 1'b0;
      dq_cur_idx        <= {MSHR_ADDR_WIDTH{1'b0}};
      for (ri = 0; ri < MSHR_SIZE; ri = ri + 1) begin
        entry_valid[ri]    <= 1'b0;
        entry_addr[ri]     <= {CS_LINE_ADDR_WIDTH{1'b0}};
        entry_write[ri]    <= 1'b0;
        entry_has_next[ri] <= 1'b0;
        entry_next_idx[ri] <= {MSHR_ADDR_WIDTH{1'b0}};
      end
    end else begin

      // -----------------------------------------------------------------
      // Register allocation outputs (1-cycle latency)
      // -----------------------------------------------------------------
      allocate_id      <= alloc_idx;
      allocate_pending <= ~prev_no_hit;
      allocate_previd  <= prev_idx;

      // -----------------------------------------------------------------
      // Finalize: release entry
      // -----------------------------------------------------------------
      if (finalize_valid) begin
        entry_valid[finalize_id]    <= 1'b0;
        entry_has_next[finalize_id] <= 1'b0;
      end

      // -----------------------------------------------------------------
      // Allocate: claim first free slot.
      // -----------------------------------------------------------------
      if (allocate_fire) begin
        entry_valid[alloc_idx]    <= 1'b1;
        entry_addr[alloc_idx]     <= allocate_addr;
        entry_write[alloc_idx]    <= allocate_rw;
        entry_has_next[alloc_idx] <= 1'b0;
        entry_next_idx[alloc_idx] <= {MSHR_ADDR_WIDTH{1'b0}};
        ram[alloc_idx]            <= allocate_data;

        // Link the tail of the existing chain to the new entry
        if (~prev_no_hit) begin
          entry_has_next[prev_idx] <= 1'b1;
          entry_next_idx[prev_idx] <= alloc_idx;
        end
      end

      // -----------------------------------------------------------------
      // Fill + Dequeue
      // On fill_valid: capture fill_id and start dequeuing.
      // Each cycle when dq_active && dequeue_ready: walk linked list.
      // -----------------------------------------------------------------
      if (fill_valid) begin
        // Start dequeue: output the first (fill_id) entry
        dq_active     <= 1'b1;
        dq_cur_idx    <= fill_id;
        dequeue_valid <= 1'b1;
        dequeue_id    <= fill_id;
        dequeue_addr  <= entry_addr[fill_id];
        dequeue_rw    <= entry_write[fill_id];
        dequeue_data  <= ram[fill_id];
      end else if (dq_active && dequeue_ready) begin
        if (entry_has_next[dq_cur_idx]) begin
          // Advance: output next entry
          dq_cur_idx    <= entry_next_idx[dq_cur_idx];
          dequeue_valid <= 1'b1;
          dequeue_id    <= entry_next_idx[dq_cur_idx];
          dequeue_addr  <= entry_addr[entry_next_idx[dq_cur_idx]];
          dequeue_rw    <= entry_write[entry_next_idx[dq_cur_idx]];
          dequeue_data  <= ram[entry_next_idx[dq_cur_idx]];
        end else begin
          // No more linked entries
          dq_active     <= 1'b0;
          dequeue_valid <= 1'b0;
        end
      end else if (~dq_active) begin
        dequeue_valid <= 1'b0;
      end

    end
  end

endmodule
