// Multi-outstanding S2MM (Stream to Memory-Mapped) write engine.
//
// Pops data from FIFO and writes to memory via AXI4.
// Issues AW+W for one burst, then immediately starts next AW+W
// while previous B response is still outstanding.
// Up to NUM_OUTSTANDING B responses can be in-flight.
//
// Architecture:
//   Idle → Active (AW+W issue + B collection) → Drain → Done
//
//   AW/W path: sequential per-burst (AW then W beats), but next
//              burst starts immediately after current finishes W.
//   B path:    collected asynchronously; inflight count tracks.
//
// FSM-based (no threads) — baseline for thread comparison.
module FsmS2mmMulti #(
  parameter int NUM_OUTSTANDING = 4,
  parameter int ID_W = 2
) (
  input logic clk,
  input logic rst,
  input logic start,
  input logic [16-1:0] total_xfers,
  input logic [32-1:0] base_addr,
  input logic [8-1:0] burst_len,
  output logic done,
  output logic halted,
  output logic idle_out,
  output logic aw_valid,
  input logic aw_ready,
  output logic [32-1:0] aw_addr,
  output logic [ID_W-1:0] aw_id,
  output logic [8-1:0] aw_len,
  output logic [3-1:0] aw_size,
  output logic [2-1:0] aw_burst,
  output logic w_valid,
  input logic w_ready,
  output logic [32-1:0] w_data,
  output logic [4-1:0] w_strb,
  output logic w_last,
  input logic b_valid,
  output logic b_ready,
  input logic [ID_W-1:0] b_id,
  input logic pop_valid,
  output logic pop_ready,
  input logic [32-1:0] pop_data
);

  typedef enum logic [2:0] {
    IDLE = 3'd0,
    SENDAW = 3'd1,
    SENDW = 3'd2,
    DRAIN = 3'd3,
    DONE = 3'd4
  } FsmS2mmMulti_state_t;
  
  FsmS2mmMulti_state_t state_r, state_next;
  
  logic [16-1:0] total_xfers_r;
  logic [32-1:0] base_addr_r;
  logic [8-1:0] burst_len_r;
  logic [16-1:0] aw_issued_r;
  logic [32-1:0] next_addr_r;
  logic [ID_W-1:0] next_id_r;
  logic [8-1:0] w_beat_ctr_r;
  logic w_sending_r;
  logic [16-1:0] b_received_r;
  logic [16-1:0] inflight_r;
  logic w_last_r;
  
  logic aw_fire;
  assign aw_fire = aw_valid && aw_ready;
  logic w_fire;
  assign w_fire = w_valid && w_ready;
  logic b_fire;
  assign b_fire = b_valid && b_ready;
  logic can_issue_aw;
  assign can_issue_aw = !w_sending_r && aw_issued_r < total_xfers_r && inflight_r < 16'(NUM_OUTSTANDING);
  logic all_b_done;
  assign all_b_done = b_received_r == total_xfers_r && aw_issued_r == total_xfers_r;
  logic w_is_last;
  assign w_is_last = w_last_r;
  
  always_ff @(posedge clk) begin
    if (rst) begin
      state_r <= IDLE;
      total_xfers_r <= 0;
      base_addr_r <= 0;
      burst_len_r <= 0;
      aw_issued_r <= 0;
      next_addr_r <= 0;
      next_id_r <= 0;
      w_beat_ctr_r <= 0;
      w_sending_r <= 0;
      b_received_r <= 0;
      inflight_r <= 0;
      w_last_r <= 0;
    end else begin
      state_r <= state_next;
      case (state_r)
        IDLE: begin
          // Control
          // Status
          // AXI4 write channels
          // FIFO pop interface
          // Latched control
          // AW tracking
          // W tracking (within current burst)
          // B tracking
          // Derived signals
          if (start) begin
            total_xfers_r <= total_xfers;
            base_addr_r <= base_addr;
            burst_len_r <= burst_len;
            next_addr_r <= base_addr;
            next_id_r <= 0;
            aw_issued_r <= 0;
            b_received_r <= 0;
            inflight_r <= 0;
            w_beat_ctr_r <= 8'(burst_len_r - 1);
            w_sending_r <= 1'b0;
            w_last_r <= 1'b0;
          end
        end
        SENDAW: begin
          // Issue AW
          // Always accept B responses
          if (aw_fire) begin
            next_addr_r <= next_addr_r + (32'($unsigned(burst_len_r)) << 2);
            next_id_r <= ID_W'(next_id_r + 1);
            aw_issued_r <= aw_issued_r + 1;
            inflight_r <= inflight_r + 1;
            w_beat_ctr_r <= 0;
            w_sending_r <= 1'b1;
            w_last_r <= burst_len_r == 1;
          end
          if (b_fire) begin
            b_received_r <= b_received_r + 1;
            inflight_r <= inflight_r - 1;
          end
          // Simultaneous AW + B: net inflight +1 -1 = 0 change handled by priority
          if (aw_fire && b_fire) begin
            inflight_r <= inflight_r;
          end
        end
        SENDW: begin
          // Drive W channel from FIFO
          // Always accept B responses
          if (w_fire) begin
            w_beat_ctr_r <= w_beat_ctr_r + 1;
            w_last_r <= 8'(w_beat_ctr_r + 1) == burst_len_r;
          end
          if (b_fire) begin
            b_received_r <= b_received_r + 1;
            inflight_r <= inflight_r - 1;
          end
        end
        DRAIN: begin
          // Last W beat registered — transition on next cycle when w_last_r is set
          if (b_fire) begin
            b_received_r <= b_received_r + 1;
            inflight_r <= inflight_r - 1;
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
        if (start) state_next = SENDAW;
      end
      SENDAW: begin
        if (aw_fire) state_next = SENDW;
      end
      SENDW: begin
        if (w_last_r && aw_issued_r < total_xfers_r && inflight_r < 16'(NUM_OUTSTANDING)) state_next = SENDAW;
        else if (w_last_r && (aw_issued_r == total_xfers_r || inflight_r >= 16'(NUM_OUTSTANDING))) state_next = DRAIN;
      end
      DRAIN: begin
        if (aw_issued_r < total_xfers_r && inflight_r < 16'(NUM_OUTSTANDING) && (!b_fire || inflight_r > 1)) state_next = SENDAW;
        else if (all_b_done) state_next = DONE;
      end
      DONE: begin
        state_next = IDLE;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    done = 1'b0;
    halted = 1'b1;
    idle_out = 1'b0;
    aw_valid = 1'b0;
    aw_addr = 0;
    aw_id = 0;
    aw_len = 0;
    aw_size = 3'd2;
    aw_burst = 2'd1;
    w_valid = 1'b0;
    w_data = 0;
    w_strb = 4'd15;
    w_last = 1'b0;
    b_ready = 1'b0;
    pop_ready = 1'b0;
    case (state_r)
      IDLE: begin
        idle_out = 1'b1;
        halted = 1'b0;
      end
      SENDAW: begin
        halted = 1'b0;
        aw_valid = 1'b1;
        aw_addr = next_addr_r;
        aw_id = ID_W'(next_id_r);
        aw_len = 8'(burst_len_r - 1);
        aw_size = 3'd2;
        aw_burst = 2'd1;
        b_ready = 1'b1;
      end
      SENDW: begin
        halted = 1'b0;
        w_valid = pop_valid;
        w_data = pop_data;
        w_strb = 4'd15;
        w_last = w_last_r;
        pop_ready = w_ready;
        b_ready = 1'b1;
      end
      DRAIN: begin
        halted = 1'b0;
        b_ready = 1'b1;
      end
      DONE: begin
        // If more AW to issue and inflight dropped, go back to SendAW
        done = 1'b1;
        halted = 1'b0;
      end
      default: ;
    endcase
  end

endmodule

