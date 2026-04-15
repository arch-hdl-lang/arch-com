// hw_task_queue: Hardware task queue using the linklist construct.
// A FIFO-ordered ready queue for hardware task scheduling.
//
// Tasks are submitted with a 32-bit payload and dispatched in FIFO order.
// insert_tail: enqueue a task — 2-cycle latency, resp_valid signals done
// delete_head: dequeue oldest task — 2-cycle latency, resp_valid + resp_data
module hw_task_queue #(
  parameter int  DEPTH = 8,
  parameter type DATA  = logic [31:0]
) (
  input  logic clk,
  input  logic rst,
  input  logic insert_tail_req_valid,
  output logic insert_tail_req_ready,
  input  logic [31:0] insert_tail_req_data,
  output logic insert_tail_resp_valid,
  output logic [2:0] insert_tail_resp_handle,
  input  logic delete_head_req_valid,
  output logic delete_head_req_ready,
  output logic delete_head_resp_valid,
  output logic [31:0] delete_head_resp_data,
  output logic empty,
  output logic full,
  output logic [3:0] length
);

  localparam int HANDLE_W = $clog2(DEPTH);
  localparam int CNT_W    = $clog2(DEPTH + 1);
  
  // Free list — circular FIFO of available slot indices
  logic [HANDLE_W-1:0] _fl_mem  [0:DEPTH-1];
  logic [CNT_W-1:0]    _fl_rdp;
  logic [CNT_W-1:0]    _fl_wrp;
  logic [CNT_W-1:0]    _fl_cnt;
  
  // Payload and link RAMs
  DATA                 _data_mem [0:DEPTH-1];
  logic [HANDLE_W-1:0] _next_mem [0:DEPTH-1];
  
  // Head / tail registers
  logic [HANDLE_W-1:0] _head_r;
  logic [HANDLE_W-1:0] _tail_r;
  
  // insert_tail controller registers
  logic _ctrl_insert_tail_busy;
  logic _ctrl_insert_tail_resp_v;
  logic [2:0] _ctrl_insert_tail_resp_handle;
  logic _ctrl_insert_tail_was_empty;
  
  // delete_head controller registers
  logic _ctrl_delete_head_busy;
  logic _ctrl_delete_head_resp_v;
  logic [31:0] _ctrl_delete_head_resp_data;
  logic [HANDLE_W-1:0] _ctrl_delete_head_slot;
  
  // Status outputs
  assign empty  = (_fl_cnt == CNT_W'(DEPTH));
  assign full   = (_fl_cnt == '0);
  assign length = CNT_W'(DEPTH) - _fl_cnt;
  
  // req_ready: combinational
  assign insert_tail_req_ready = !_ctrl_insert_tail_busy && !(_fl_cnt == '0);
  assign insert_tail_resp_valid = _ctrl_insert_tail_resp_v;
  assign insert_tail_resp_handle = _ctrl_insert_tail_resp_handle;
  assign delete_head_req_ready = !_ctrl_delete_head_busy && !(_fl_cnt == CNT_W'(DEPTH));
  assign delete_head_resp_valid = _ctrl_delete_head_resp_v;
  assign delete_head_resp_data = _ctrl_delete_head_resp_data;
  
  integer _ll_i;
  always_ff @(posedge clk) begin
    if (rst) begin
      for (_ll_i = 0; _ll_i < DEPTH; _ll_i++)
        _fl_mem[_ll_i] <= HANDLE_W'(_ll_i);
      _fl_rdp <= '0;
      _fl_wrp <= '0;
      _fl_cnt <= CNT_W'(DEPTH);
      _head_r <= '0;
      _tail_r <= '0;
      _ctrl_insert_tail_busy <= 1'b0;
      _ctrl_insert_tail_resp_v <= 1'b0;
      _ctrl_delete_head_busy <= 1'b0;
      _ctrl_delete_head_resp_v <= 1'b0;
    end else begin
      _ctrl_insert_tail_resp_v <= 1'b0;
      _ctrl_delete_head_resp_v <= 1'b0;
      
      // ── insert_tail ─────────────────────────────────────────
      if (!_ctrl_insert_tail_busy && insert_tail_req_valid && !(_fl_cnt == '0)) begin
        _ctrl_insert_tail_resp_handle <= _fl_mem[_fl_rdp[HANDLE_W-1:0]];
        _data_mem[_fl_mem[_fl_rdp[HANDLE_W-1:0]]] <= insert_tail_req_data;
        _fl_rdp <= _fl_rdp + 1'b1;
        _fl_cnt <= _fl_cnt - 1'b1;
        _ctrl_insert_tail_was_empty <= (_fl_cnt == CNT_W'(DEPTH));
        _ctrl_insert_tail_busy <= 1'b1;
      end else if (_ctrl_insert_tail_busy) begin
        if (!_ctrl_insert_tail_was_empty) _next_mem[_tail_r] <= _ctrl_insert_tail_resp_handle;
        _tail_r <= _ctrl_insert_tail_resp_handle;
        if (_ctrl_insert_tail_was_empty) _head_r <= _ctrl_insert_tail_resp_handle;
        _ctrl_insert_tail_resp_v <= 1'b1;
        _ctrl_insert_tail_busy <= 1'b0;
      end
      
      // ── delete_head ─────────────────────────────────────────
      if (!_ctrl_delete_head_busy && delete_head_req_valid && !(_fl_cnt == CNT_W'(DEPTH))) begin
        _ctrl_delete_head_resp_data <= _data_mem[_head_r];
        _ctrl_delete_head_slot      <= _head_r;
        _ctrl_delete_head_busy <= 1'b1;
      end else if (_ctrl_delete_head_busy) begin
        _fl_mem[_fl_wrp[HANDLE_W-1:0]] <= _ctrl_delete_head_slot;
        _fl_wrp <= _fl_wrp + 1'b1;
        _fl_cnt <= _fl_cnt + 1'b1;
        _head_r <= _next_mem[_ctrl_delete_head_slot];
        _ctrl_delete_head_resp_v <= 1'b1;
        _ctrl_delete_head_busy <= 1'b0;
      end
      
    end
  end
  
endmodule

