// MM2S channel FSM — reads from memory via AXI4, pushes data into FIFO.
// States: Idle → SendAR → WaitR → Done
module FsmMm2s (
  input logic clk,
  input logic rst,
  input logic start,
  input logic [32-1:0] src_addr,
  input logic [8-1:0] num_beats,
  output logic done,
  output logic halted,
  output logic idle_out,
  output logic ar_valid,
  input logic ar_ready,
  output logic [32-1:0] ar_addr,
  output logic [8-1:0] ar_len,
  output logic [3-1:0] ar_size,
  output logic [2-1:0] ar_burst,
  input logic r_valid,
  output logic r_ready,
  input logic [32-1:0] r_data,
  input logic r_last,
  output logic push_valid,
  input logic push_ready,
  output logic [32-1:0] push_data
);

  typedef enum logic [1:0] {
    IDLE = 2'd0,
    SENDAR = 2'd1,
    WAITR = 2'd2,
    DONE = 2'd3
  } FsmMm2s_state_t;
  
  FsmMm2s_state_t state_r, state_next;
  
  logic [32-1:0] src_addr_r;
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
          // AXI4 Read Address channel
          // AXI4 Read Data channel
          // FIFO push interface
          // Internal registers
          if (start) begin
            src_addr_r <= src_addr;
            num_beats_r <= num_beats;
            beat_ctr_r <= 0;
          end
        end
        WAITR: begin
          if (r_valid & push_ready) begin
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
        if (start) state_next = SENDAR;
      end
      SENDAR: begin
        if (ar_ready) state_next = WAITR;
      end
      WAITR: begin
        if (r_valid & r_last & push_ready) state_next = DONE;
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
    ar_valid = 1'b0;
    ar_addr = 0;
    ar_len = 0;
    ar_size = 0;
    ar_burst = 0;
    r_ready = 1'b0;
    push_valid = 1'b0;
    push_data = 0;
    case (state_r)
      IDLE: begin
        halted = 1'b1;
        idle_out = 1'b1;
      end
      SENDAR: begin
        ar_valid = 1'b1;
        ar_addr = src_addr_r;
        ar_len = 8'(num_beats_r - 1);
        ar_size = 2;
        ar_burst = 1;
      end
      WAITR: begin
        r_ready = push_ready;
        push_valid = r_valid;
        push_data = r_data;
      end
      DONE: begin
        done = 1'b1;
      end
      default: ;
    endcase
  end

endmodule

