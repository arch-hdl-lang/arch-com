// AXI4 read burst FSM: fetches one cache line (8 x 64-bit beats, INCR).
// fill_word is a registered Vec output — valid after fill_done is pulsed
// and retained until the next fill starts.
module FsmAxi4Fill (
  input logic clk,
  input logic rst,
  input logic fill_start,
  input logic [64-1:0] fill_addr,
  output logic fill_done,
  output logic [64-1:0] fill_word [8-1:0],
  output logic ar_valid,
  input logic ar_ready,
  output logic [64-1:0] ar_addr,
  output logic [4-1:0] ar_id,
  output logic [8-1:0] ar_len,
  output logic [3-1:0] ar_size,
  output logic [2-1:0] ar_burst,
  input logic r_valid,
  output logic r_ready,
  input logic [64-1:0] r_data,
  input logic [4-1:0] r_id,
  input logic [2-1:0] r_resp,
  input logic r_last
);

  typedef enum logic [1:0] {
    IDLE = 2'd0,
    SENDAR = 2'd1,
    WAITR = 2'd2,
    DONE = 2'd3
  } FsmAxi4Fill_state_t;
  
  FsmAxi4Fill_state_t state_r, state_next;
  
  logic [64-1:0] fill_addr_r;
  logic [4-1:0] beat_ctr_r;
  
  always_ff @(posedge clk) begin
    if (rst) begin
      state_r <= IDLE;
      fill_addr_r <= 0;
      beat_ctr_r <= 0;
    end else begin
      state_r <= state_next;
      case (state_r)
        IDLE: begin
          // Handshake with cache controller
          // Filled line words — registered Vec, retain value after Done
          // AXI4 read address channel
          // AXI4 read data channel
          if (fill_start) begin
            fill_addr_r <= fill_addr;
            beat_ctr_r <= 0;
          end
        end
        WAITR: begin
          if (r_valid) begin
            fill_word[3'(beat_ctr_r)] <= r_data;
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
        if (fill_start) state_next = SENDAR;
      end
      SENDAR: begin
        if (ar_ready) state_next = WAITR;
      end
      WAITR: begin
        if (r_valid & r_last) state_next = DONE;
      end
      DONE: begin
        state_next = IDLE;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    fill_done = 1'b0;
    ar_valid = 1'b0;
    ar_addr = 0;
    ar_id = 0;
    ar_len = 0;
    ar_size = 0;
    ar_burst = 0;
    r_ready = 1'b0;
    case (state_r)
      IDLE: begin
      end
      SENDAR: begin
        ar_valid = 1'b1;
        ar_addr = fill_addr_r & ~64'($unsigned(63));
        ar_id = 0;
        ar_len = 7;
        ar_size = 3;
        ar_burst = 1;
      end
      WAITR: begin
        r_ready = 1'b1;
      end
      DONE: begin
        fill_done = 1'b1;
      end
      default: ;
    endcase
  end

endmodule

