// BufMgr_Sm — Small variant for arch sim testing
// 16 entries x 32-bit, 4 queues. Same architecture as BufMgr.
module BufMgrSm #(
  parameter int DEPTH = 16,
  parameter int QUEUE_COUNT = 4,
  parameter int DATA_WIDTH = 32,
  parameter int PTR_WIDTH = 4,
  parameter int QN_WIDTH = 2
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
  output logic [5-1:0] free_count_out,
  output logic init_done
);

  logic [4-1:0] head_arr [0:4-1] = '{default: 0};
  logic [4-1:0] tail_arr [0:4-1] = '{default: 0};
  logic [5-1:0] count_arr [0:4-1] = '{default: 0};
  logic [PTR_WIDTH-1:0] free_rd_ptr = 0;
  logic [PTR_WIDTH-1:0] free_wr_ptr = 0;
  logic [5-1:0] free_count = 0;
  logic setup_done = 0;
  logic [PTR_WIDTH-1:0] setup_ctr = 0;
  // Enqueue pipeline regs
  logic eq1_valid = 0;
  logic [QN_WIDTH-1:0] eq1_qn = 0;
  logic [DATA_WIDTH-1:0] eq1_data = 0;
  logic [PTR_WIDTH-1:0] eq1_old_tail = 0;
  logic eq1_was_empty = 0;
  logic eq2_valid;
  always_ff @(posedge clk) begin
    if (rst) begin
      eq2_valid <= '0;
    end else begin
      eq2_valid <= eq1_valid;
    end
  end
  logic [QN_WIDTH-1:0] eq2_qn = 0;
  logic [DATA_WIDTH-1:0] eq2_data = 0;
  logic [PTR_WIDTH-1:0] eq2_old_tail = 0;
  logic eq2_was_empty = 0;
  logic [PTR_WIDTH-1:0] eq2_free_slot = 0;
  // Dequeue pipeline regs
  logic dq1_valid = 0;
  logic [QN_WIDTH-1:0] dq1_qn = 0;
  logic [PTR_WIDTH-1:0] dq1_old_head = 0;
  logic dq2_valid = 0;
  logic [QN_WIDTH-1:0] dq2_qn = 0;
  logic [PTR_WIDTH-1:0] dq2_old_head = 0;
  // SRAM output wires
  logic [4-1:0] free_slot_rd_data = 0;
  logic [32-1:0] data_rd_data = 0;
  logic [4-1:0] next_ptr_rd_data = 0;
  // Bypass logic
  logic [PTR_WIDTH-1:0] eq0_tail_bypassed;
  assign eq0_tail_bypassed = ((eq1_valid && (eq1_qn == enqueue_queue_number))) ? (free_slot_rd_data) : (((eq2_valid && (eq2_qn == enqueue_queue_number))) ? (eq2_free_slot) : (tail_arr[enqueue_queue_number]));
  logic [5-1:0] eq0_count_raw;
  assign eq0_count_raw = count_arr[enqueue_queue_number];
  logic [5-1:0] eq0_count_adj_eq2;
  assign eq0_count_adj_eq2 = ((eq2_valid && (eq2_qn == enqueue_queue_number))) ? (5'((eq0_count_raw + 4'd1))) : (eq0_count_raw);
  logic [5-1:0] eq0_count_adj_eq1;
  assign eq0_count_adj_eq1 = ((eq1_valid && (eq1_qn == enqueue_queue_number))) ? (5'((eq0_count_adj_eq2 + 4'd1))) : (eq0_count_adj_eq2);
  logic eq0_was_empty;
  assign eq0_was_empty = (eq0_count_adj_eq1 == 5'd0);
  logic [PTR_WIDTH-1:0] dq0_head_bypassed;
  assign dq0_head_bypassed = ((dq2_valid && (dq2_qn == dequeue_queue_number))) ? (next_ptr_rd_data) : (head_arr[dequeue_queue_number]);
  logic [5-1:0] dq0_count_raw;
  assign dq0_count_raw = count_arr[dequeue_queue_number];
  logic [5-1:0] dq0_count_adj_dq2;
  assign dq0_count_adj_dq2 = ((dq2_valid && (dq2_qn == dequeue_queue_number))) ? (5'((dq0_count_raw - 4'd1))) : (dq0_count_raw);
  // RAM instances
  DataMemSm dmem (
    .clk(clk),
    .wr_port_en(eq2_valid),
    .wr_port_addr(eq2_free_slot),
    .wr_port_data(eq2_data),
    .rd_port_en((dequeue_valid && setup_done)),
    .rd_port_addr(dq0_head_bypassed),
    .rd_port_data(data_rd_data)
  );
  NextPtrMemSm nptr (
    .clk(clk),
    .wr_port_en((eq2_valid && (!eq2_was_empty))),
    .wr_port_addr(eq2_old_tail),
    .wr_port_data(eq2_free_slot),
    .rd_port_en((dequeue_valid && setup_done)),
    .rd_port_addr(dq0_head_bypassed),
    .rd_port_data(next_ptr_rd_data)
  );
  FreeListMemSm flist (
    .clk(clk),
    .rd_port_en(((enqueue_valid && setup_done) || (!setup_done))),
    .rd_port_addr((setup_done) ? (free_rd_ptr) : (setup_ctr)),
    .wr_port_en(((dq2_valid && setup_done) || (!setup_done))),
    .wr_port_addr((setup_done) ? (free_wr_ptr) : (setup_ctr)),
    .wr_port_data((setup_done) ? (dq2_old_head) : (setup_ctr)),
    .rd_port_data(free_slot_rd_data)
  );
  always_ff @(posedge clk) begin
    if (rst) begin
      count_arr <= '{default: 0};
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
      eq2_free_slot <= 0;
      eq2_old_tail <= 0;
      eq2_qn <= 0;
      eq2_was_empty <= 0;
      free_count <= 0;
      free_rd_ptr <= 0;
      free_wr_ptr <= 0;
      head_arr <= '{default: 0};
      setup_ctr <= 0;
      setup_done <= 0;
      tail_arr <= '{default: 0};
    end else begin
      if ((!setup_done)) begin
        setup_ctr <= 4'((setup_ctr + 4'd1));
        if ((setup_ctr == 4'd15)) begin
          setup_done <= 1'd1;
          free_count <= 5'd16;
          free_rd_ptr <= 4'd0;
          free_wr_ptr <= 4'd0;
        end
      end
      if ((enqueue_valid && setup_done)) begin
        eq1_valid <= 1'd1;
        eq1_qn <= enqueue_queue_number;
        eq1_data <= enqueue_data;
        eq1_old_tail <= eq0_tail_bypassed;
        eq1_was_empty <= eq0_was_empty;
        free_rd_ptr <= 4'((free_rd_ptr + 4'd1));
        free_count <= 5'((free_count - 5'd1));
      end else begin
        eq1_valid <= 1'd0;
      end
      eq2_qn <= eq1_qn;
      eq2_data <= eq1_data;
      eq2_old_tail <= eq1_old_tail;
      eq2_was_empty <= eq1_was_empty;
      eq2_free_slot <= free_slot_rd_data;
      if (eq2_valid) begin
        tail_arr[eq2_qn] <= eq2_free_slot;
        count_arr[eq2_qn] <= 5'((count_arr[eq2_qn] + 5'd1));
        if (eq2_was_empty) begin
          head_arr[eq2_qn] <= eq2_free_slot;
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
        count_arr[dq2_qn] <= 5'((count_arr[dq2_qn] - 5'd1));
        free_wr_ptr <= 4'((free_wr_ptr + 4'd1));
        free_count <= 5'((free_count + 5'd1));
      end
      if (((eq2_valid && dq2_valid) && (eq2_qn == dq2_qn))) begin
        count_arr[eq2_qn] <= count_arr[eq2_qn];
      end
      if (((enqueue_valid && setup_done) && dq2_valid)) begin
        free_count <= free_count;
      end
    end
  end
  assign dequeue_resp_valid = dq2_valid;
  assign dequeue_data = data_rd_data;
  assign free_count_out = free_count;
  assign init_done = setup_done;

endmodule

