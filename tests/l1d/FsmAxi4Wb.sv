// AXI4 write burst FSM: evicts one dirty cache line (8 x 64-bit beats, INCR).
// wb_word must be driven valid from wb_start until wb_done.
// Uses AXI4 write ID=1 to distinguish from fills (ID=0).
module FsmAxi4Wb (
  input logic clk,
  input logic rst,
  input logic wb_start,
  input logic [64-1:0] wb_addr,
  output logic wb_done,
  input logic [64-1:0] wb_word [8-1:0],
  output logic aw_valid,
  input logic aw_ready,
  output logic [64-1:0] aw_addr,
  output logic [4-1:0] aw_id,
  output logic [8-1:0] aw_len,
  output logic [3-1:0] aw_size,
  output logic [2-1:0] aw_burst,
  output logic w_valid,
  input logic w_ready,
  output logic [64-1:0] w_data,
  output logic [8-1:0] w_strb,
  output logic w_last,
  input logic b_valid,
  output logic b_ready,
  input logic [4-1:0] b_id,
  input logic [2-1:0] b_resp
);

  typedef enum logic [1:0] {
    IDLE = 2'd0,
    SENDAW = 2'd1,
    SENDW = 2'd2,
    WAITB = 2'd3
  } FsmAxi4Wb_state_t;
  
  FsmAxi4Wb_state_t state_r, state_next;
  
  logic [64-1:0] wb_addr_r;
  logic [4-1:0] beat_ctr_r;
  
  always_ff @(posedge clk) begin
    if (rst) begin
      state_r <= IDLE;
      wb_addr_r <= 0;
      beat_ctr_r <= 0;
    end else begin
      state_r <= state_next;
      case (state_r)
        IDLE: begin
          // Handshake with cache controller
          // Dirty line words to write (held valid by controller during entire WB)
          // AXI4 write address channel
          // AXI4 write data channel
          // AXI4 write response channel
          if (wb_start) begin
            wb_addr_r <= wb_addr;
            beat_ctr_r <= 0;
          end
        end
        SENDW: begin
          if (w_ready) begin
            beat_ctr_r <= 4'(beat_ctr_r + 1);
          end
        end
        default: ;
      endcase
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    case (state_r)
      IDLE: begin
        if (wb_start) state_next = SENDAW;
      end
      SENDAW: begin
        if (aw_ready) state_next = SENDW;
      end
      SENDW: begin
        if (w_ready & beat_ctr_r == 7) state_next = WAITB;
      end
      WAITB: begin
        if (b_valid) state_next = IDLE;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    wb_done = 1'b0;
    aw_valid = 1'b0;
    aw_addr = 0;
    aw_id = 0;
    aw_len = 0;
    aw_size = 0;
    aw_burst = 0;
    w_valid = 1'b0;
    w_data = 0;
    w_strb = 0;
    w_last = 1'b0;
    b_ready = 1'b0;
    case (state_r)
      IDLE: begin
      end
      SENDAW: begin
        aw_valid = 1'b1;
        aw_addr = wb_addr_r & ~64'($unsigned(63));
        aw_id = 1;
        aw_len = 7;
        aw_size = 3;
        aw_burst = 1;
      end
      SENDW: begin
        w_valid = 1'b1;
        w_strb = 255;
        w_last = beat_ctr_r == 7;
        w_data = wb_word[3'(beat_ctr_r)];
      end
      WAITB: begin
        b_ready = 1'b1;
        wb_done = b_valid;
      end
      default: ;
    endcase
  end

endmodule

