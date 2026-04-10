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

  typedef enum logic [1:0] {
    IDLE = 2'd0,
    ACTIVE = 2'd1,
    DRAIN = 2'd2,
    DONE = 2'd3
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
          // AW+W combined state: issue AW first, then W beats, no dead cycles
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
        ACTIVE: begin
          // Combined AW+W state: issues AW, then sends W beats, loops back for next burst.
          // AW is issued on the FIRST cycle (when w_sending_r=false) or OVERLAPPED with
          // the last W beat of the previous burst (w_sending_r transitions false→true).
          // AW channel: issue when not sending W beats, OR overlap with last W beat
          // W channel: drive from FIFO when sending
          // W beat accepted
          if (w_fire) begin
            w_beat_ctr_r <= w_beat_ctr_r + 1;
          end
          // Last W beat + AW simultaneously: seamless burst transition (zero gap)
          if (w_fire && w_beat_ctr_r == 8'(burst_len_r - 1) && aw_fire) begin
            // AW for next burst accepted on same cycle as last W beat
            next_addr_r <= next_addr_r + (32'($unsigned(burst_len_r)) << 2);
            next_id_r <= ID_W'(next_id_r + 1);
            aw_issued_r <= aw_issued_r + 1;
            inflight_r <= inflight_r + 1;
            w_beat_ctr_r <= 0;
            w_sending_r <= 1'b1;
            w_last_r <= burst_len_r == 1;
          end else if (w_fire && w_beat_ctr_r == 8'(burst_len_r - 1)) begin
            // Last W beat, no AW overlap
            w_sending_r <= 1'b0;
          end else if (aw_fire) begin
            // AW accepted (first burst or after gap)
            next_addr_r <= next_addr_r + (32'($unsigned(burst_len_r)) << 2);
            next_id_r <= ID_W'(next_id_r + 1);
            aw_issued_r <= aw_issued_r + 1;
            inflight_r <= inflight_r + 1;
            w_beat_ctr_r <= 0;
            w_sending_r <= 1'b1;
            w_last_r <= burst_len_r == 1;
          end
          // B response
          if (b_fire) begin
            b_received_r <= b_received_r + 1;
            inflight_r <= inflight_r - 1;
          end
          // Simultaneous AW + B: net zero inflight change
          if (aw_fire && b_fire) begin
            inflight_r <= inflight_r;
          end
        end
        DRAIN: begin
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
        if (start) state_next = ACTIVE;
      end
      ACTIVE: begin
        if (aw_issued_r == total_xfers_r && !w_sending_r) state_next = DRAIN;
      end
      DRAIN: begin
        if (all_b_done) state_next = DONE;
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
      ACTIVE: begin
        halted = 1'b0;
        b_ready = 1'b1;
        if (can_issue_aw && (!w_sending_r || w_beat_ctr_r == 8'(burst_len_r - 1))) begin
          aw_valid = 1'b1;
          aw_addr = next_addr_r;
          aw_id = ID_W'(next_id_r);
          aw_len = 8'(burst_len_r - 1);
          aw_size = 3'd2;
          aw_burst = 2'd1;
        end
        if (w_sending_r) begin
          w_valid = pop_valid;
          w_data = pop_data;
          w_strb = 4'd15;
          w_last = w_beat_ctr_r == 8'(burst_len_r - 1);
          pop_ready = w_ready;
        end
      end
      DRAIN: begin
        halted = 1'b0;
        b_ready = 1'b1;
      end
      DONE: begin
        done = 1'b1;
        halted = 1'b0;
      end
      default: ;
    endcase
  end

endmodule

