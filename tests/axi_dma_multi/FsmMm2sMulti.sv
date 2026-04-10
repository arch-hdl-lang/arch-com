// Multi-outstanding MM2S (Memory-Mapped to Stream) read engine.
//
// Issues up to NUM_OUTSTANDING AXI4 read bursts concurrently.
// Each burst is tagged with a unique AXI ID.  R responses are
// collected in-order (FIFO push), with r_id used to track
// per-transaction beat counts.
//
// Architecture:
//   Idle → Active (concurrent AR issue + R collection) → Done
//
//   In Active state:
//     AR channel: issues bursts while inflight < NUM_OUTSTANDING
//                 and xfers_issued < total_xfers
//     R channel:  always accepts beats, pushes to FIFO,
//                 frees ID slot on r_last
//
// FSM-based (no threads) — baseline for thread comparison.
module FsmMm2sMulti #(
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
  output logic ar_valid,
  input logic ar_ready,
  output logic [32-1:0] ar_addr,
  output logic [ID_W-1:0] ar_id,
  output logic [8-1:0] ar_len,
  output logic [3-1:0] ar_size,
  output logic [2-1:0] ar_burst,
  input logic r_valid,
  output logic r_ready,
  input logic [32-1:0] r_data,
  input logic [ID_W-1:0] r_id,
  input logic r_last,
  output logic push_valid,
  input logic push_ready,
  output logic [32-1:0] push_data
);

  typedef enum logic [1:0] {
    IDLE = 2'd0,
    ACTIVE = 2'd1,
    DONE = 2'd2
  } FsmMm2sMulti_state_t;
  
  FsmMm2sMulti_state_t state_r, state_next;
  
  logic [16-1:0] total_xfers_r;
  logic [32-1:0] base_addr_r;
  logic [8-1:0] burst_len_r;
  logic [16-1:0] xfers_issued_r;
  logic [16-1:0] xfers_complete_r;
  logic [32-1:0] next_addr_r;
  logic [ID_W-1:0] next_id_r;
  logic [16-1:0] inflight_r;
  
  logic can_issue;
  assign can_issue = inflight_r < 16'(NUM_OUTSTANDING) && xfers_issued_r < total_xfers_r;
  logic all_done;
  assign all_done = xfers_complete_r == total_xfers_r && xfers_issued_r == total_xfers_r;
  logic ar_fire;
  assign ar_fire = ar_valid && ar_ready;
  logic r_fire;
  assign r_fire = r_valid && r_ready && push_ready;
  
  always_ff @(posedge clk) begin
    if (rst) begin
      state_r <= IDLE;
      total_xfers_r <= 0;
      base_addr_r <= 0;
      burst_len_r <= 0;
      xfers_issued_r <= 0;
      xfers_complete_r <= 0;
      next_addr_r <= 0;
      next_id_r <= 0;
      inflight_r <= 0;
    end else begin
      state_r <= state_next;
      case (state_r)
        IDLE: begin
          // Control interface
          // Status
          // AXI4 read channels (directly wired, not using bus construct)
          // FIFO push interface
          // Latched control registers
          // Transaction tracking
          // Derived signals
          if (start) begin
            total_xfers_r <= total_xfers;
            base_addr_r <= base_addr;
            burst_len_r <= burst_len;
            next_addr_r <= base_addr;
            next_id_r <= 0;
            xfers_issued_r <= 0;
            xfers_complete_r <= 0;
            inflight_r <= 0;
          end
        end
        ACTIVE: begin
          // AR channel: issue next burst if possible
          // R channel: always accept and push to FIFO
          // AR handshake: advance to next transfer
          if (ar_fire) begin
            next_addr_r <= next_addr_r + (32'($unsigned(burst_len_r)) << 2);
            next_id_r <= ID_W'(next_id_r + 1);
            xfers_issued_r <= xfers_issued_r + 1;
            inflight_r <= inflight_r + 1;
          end
          // R last beat: free ID slot
          if (r_fire && r_last) begin
            xfers_complete_r <= xfers_complete_r + 1;
            inflight_r <= inflight_r - 1;
          end
          // Handle simultaneous AR fire and R last (net inflight unchanged)
          if (ar_fire && r_fire && r_last) begin
            inflight_r <= inflight_r;
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
        if (all_done) state_next = DONE;
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
    ar_valid = 1'b0;
    ar_addr = 0;
    ar_id = 0;
    ar_len = 0;
    ar_size = 3'd2;
    ar_burst = 2'd1;
    r_ready = 1'b0;
    push_valid = 1'b0;
    push_data = 0;
    case (state_r)
      IDLE: begin
        idle_out = 1'b1;
        halted = 1'b0;
      end
      ACTIVE: begin
        halted = 1'b0;
        if (can_issue) begin
          ar_valid = 1'b1;
          ar_addr = next_addr_r;
          ar_id = ID_W'(next_id_r);
          ar_len = 8'(burst_len_r - 1);
          ar_size = 3'd2;
          ar_burst = 2'd1;
        end
        r_ready = push_ready;
        push_valid = r_valid;
        push_data = r_data;
      end
      DONE: begin
        done = 1'b1;
        halted = 1'b0;
      end
      default: ;
    endcase
  end

endmodule

