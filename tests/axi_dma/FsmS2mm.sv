// S2MM channel FSM — pops data from FIFO, writes to memory via AXI4.
// States: Idle → WaitRecv → SendAW → SendW → WaitB → Done
module FsmS2mm (
  input logic clk,
  input logic rst,
  input logic start,
  input logic [32-1:0] dst_addr,
  input logic [8-1:0] num_beats,
  input logic [8-1:0] recv_count,
  output logic done,
  output logic halted,
  output logic idle_out,
  input logic pop_valid,
  output logic pop_ready,
  input logic [32-1:0] pop_data,
  output logic aw_valid,
  input logic aw_ready,
  output logic [32-1:0] aw_addr,
  output logic [8-1:0] aw_len,
  output logic [3-1:0] aw_size,
  output logic [2-1:0] aw_burst,
  output logic w_valid,
  input logic w_ready,
  output logic [32-1:0] w_data,
  output logic [4-1:0] w_strb,
  output logic w_last,
  input logic b_valid,
  output logic b_ready
);

  typedef enum logic [2:0] {
    IDLE = 3'd0,
    WAITRECV = 3'd1,
    SENDAW = 3'd2,
    SENDW = 3'd3,
    WAITB = 3'd4,
    DONE = 3'd5
  } FsmS2mm_state_t;
  
  FsmS2mm_state_t state_r, state_next;
  
  logic [32-1:0] dst_addr_r;
  logic [8-1:0] num_beats_r;
  logic [8-1:0] beat_ctr_r;
  
  always_ff @(posedge clk) begin
    if (rst) begin
      state_r <= IDLE;
      beat_ctr_r <= 0;
    end else begin
      state_r <= state_next;
      case (state_r)
        IDLE: begin
          // Control interface (from register block)
          // Status outputs
          // FIFO pop interface
          // AXI4 Write Address channel
          // AXI4 Write Data channel
          // AXI4 Write Response channel
          // Internal registers
          if (start) begin
            dst_addr_r <= dst_addr;
            num_beats_r <= num_beats;
            beat_ctr_r <= 0;
          end
        end
        SENDW: begin
          if (w_ready & pop_valid) begin
            beat_ctr_r <= 8'(beat_ctr_r + 1);
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
        if (start) state_next = WAITRECV;
      end
      WAITRECV: begin
        if (recv_count >= num_beats_r) state_next = SENDAW;
      end
      SENDAW: begin
        if (aw_ready) state_next = SENDW;
      end
      SENDW: begin
        if (w_ready & pop_valid & w_last) state_next = WAITB;
      end
      WAITB: begin
        if (b_valid) state_next = DONE;
      end
      DONE: begin
        state_next = IDLE;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    done = 1'b0;
    halted = 1'b0;
    idle_out = 1'b0;
    pop_ready = 1'b0;
    aw_valid = 1'b0;
    aw_addr = 0;
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
        halted = 1'b1;
        idle_out = 1'b1;
      end
      WAITRECV: begin
      end
      SENDAW: begin
        aw_valid = 1'b1;
        aw_addr = dst_addr_r;
        aw_len = 8'(num_beats_r - 1);
        aw_size = 2;
        aw_burst = 1;
      end
      SENDW: begin
        w_valid = pop_valid;
        w_data = pop_data;
        w_strb = 'hF;
        w_last = beat_ctr_r == 8'(num_beats_r - 1);
        pop_ready = w_ready;
      end
      WAITB: begin
        b_ready = 1'b1;
      end
      DONE: begin
        done = 1'b1;
      end
      default: ;
    endcase
  end

endmodule

