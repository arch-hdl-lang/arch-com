// Basic singly-linked task queue
// Exercises: alloc, free, insert_tail, delete_head, read_data, write_data
// with full/empty/length status outputs.
module TaskQueue #(
  parameter int  DEPTH = 8,
  parameter type DATA  = logic [32-1:0]
) (
  input  logic clk,
  input  logic rst,
  input  logic alloc_req_valid,
  output logic alloc_req_ready,
  output logic alloc_resp_valid,
  output logic [3-1:0] alloc_resp_handle,
  input  logic free_req_valid,
  output logic free_req_ready,
  input  logic [3-1:0] free_req_handle,
  input  logic insert_tail_req_valid,
  output logic insert_tail_req_ready,
  input  DATA insert_tail_req_data,
  output logic insert_tail_resp_valid,
  output logic [3-1:0] insert_tail_resp_handle,
  input  logic delete_head_req_valid,
  output logic delete_head_req_ready,
  output logic delete_head_resp_valid,
  output DATA delete_head_resp_data,
  input  logic read_data_req_valid,
  input  logic [3-1:0] read_data_req_handle,
  output logic read_data_resp_valid,
  output DATA read_data_resp_data,
  input  logic write_data_req_valid,
  input  logic [3-1:0] write_data_req_handle,
  input  DATA write_data_req_data,
  output logic write_data_resp_valid,
  output logic empty,
  output logic full,
  output logic [4-1:0] length
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
  
  // alloc controller registers
  logic _ctrl_alloc_resp_v;
  logic [3-1:0] _ctrl_alloc_resp_handle;
  
  // free controller registers
  
  // insert_tail controller registers
  logic _ctrl_insert_tail_busy;
  logic _ctrl_insert_tail_resp_v;
  logic [3-1:0] _ctrl_insert_tail_resp_handle;
  logic _ctrl_insert_tail_was_empty;
  
  // delete_head controller registers
  logic _ctrl_delete_head_busy;
  logic _ctrl_delete_head_resp_v;
  DATA _ctrl_delete_head_resp_data;
  logic [HANDLE_W-1:0] _ctrl_delete_head_slot;
  
  // read_data controller registers
  logic _ctrl_read_data_resp_v;
  DATA _ctrl_read_data_resp_data;
  
  // write_data controller registers
  logic _ctrl_write_data_resp_v;
  
  // Status outputs
  assign empty  = (_fl_cnt == CNT_W'(DEPTH));
  assign full   = (_fl_cnt == '0);
  assign length = CNT_W'(DEPTH) - _fl_cnt;
  
  // req_ready: combinational
  assign alloc_req_ready = !(_fl_cnt == '0);
  assign alloc_resp_valid = _ctrl_alloc_resp_v;
  assign alloc_resp_handle = _ctrl_alloc_resp_handle;
  assign free_req_ready = !(_fl_cnt == CNT_W'(DEPTH));
  assign insert_tail_req_ready = !_ctrl_insert_tail_busy && !(_fl_cnt == '0);
  assign insert_tail_resp_valid = _ctrl_insert_tail_resp_v;
  assign insert_tail_resp_handle = _ctrl_insert_tail_resp_handle;
  assign delete_head_req_ready = !_ctrl_delete_head_busy && !(_fl_cnt == CNT_W'(DEPTH));
  assign delete_head_resp_valid = _ctrl_delete_head_resp_v;
  assign delete_head_resp_data = _ctrl_delete_head_resp_data;
  assign read_data_resp_valid = _ctrl_read_data_resp_v;
  assign read_data_resp_data = _ctrl_read_data_resp_data;
  assign write_data_resp_valid = _ctrl_write_data_resp_v;
  
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
      _ctrl_alloc_resp_v <= 1'b0;
      _ctrl_insert_tail_busy <= 1'b0;
      _ctrl_insert_tail_resp_v <= 1'b0;
      _ctrl_delete_head_busy <= 1'b0;
      _ctrl_delete_head_resp_v <= 1'b0;
      _ctrl_read_data_resp_v <= 1'b0;
      _ctrl_write_data_resp_v <= 1'b0;
    end else begin
      _ctrl_alloc_resp_v <= 1'b0;
      _ctrl_insert_tail_resp_v <= 1'b0;
      _ctrl_delete_head_resp_v <= 1'b0;
      _ctrl_read_data_resp_v <= 1'b0;
      _ctrl_write_data_resp_v <= 1'b0;
      
      // ── alloc ─────────────────────────────────────────
      if (alloc_req_valid && !(_fl_cnt == '0)) begin
        _fl_rdp <= _fl_rdp + 1'b1;
        _fl_cnt <= _fl_cnt - 1'b1;
        _ctrl_alloc_resp_v <= 1'b1;
        _ctrl_alloc_resp_handle <= _fl_mem[_fl_rdp[HANDLE_W-1:0]];
      end
      
      // ── free ─────────────────────────────────────────
      if (free_req_valid) begin
        _fl_mem[_fl_wrp[HANDLE_W-1:0]] <= free_req_handle;
        _fl_wrp <= _fl_wrp + 1'b1;
        _fl_cnt <= _fl_cnt + 1'b1;
      end
      
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
      
      // ── read_data ─────────────────────────────────────────
      if (read_data_req_valid) begin
        _ctrl_read_data_resp_data <= _data_mem[read_data_req_handle];
        _ctrl_read_data_resp_v <= 1'b1;
      end
      
      // ── write_data ─────────────────────────────────────────
      if (write_data_req_valid) begin
        _data_mem[write_data_req_handle] <= write_data_req_data;
        _ctrl_write_data_resp_v <= 1'b1;
      end
      
    end
  end
  
endmodule

