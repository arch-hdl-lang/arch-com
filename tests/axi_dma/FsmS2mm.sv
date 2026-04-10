// S2MM channel FSM — pops data from FIFO, writes to memory via AXI4.
// States: Idle → WaitRecv → SendAW → SendW → WaitB → Done
// Timing: w_last_r is a lookahead register — set one cycle early so the
// critical path reads a FF output instead of computing beat_ctr == num_beats-1
// combinationally through a subtractor and into the FSM next-state mux.
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
  output logic axi_wr_aw_valid,
  input logic axi_wr_aw_ready,
  output logic [32-1:0] axi_wr_aw_addr,
  output logic [1-1:0] axi_wr_aw_id,
  output logic [8-1:0] axi_wr_aw_len,
  output logic [3-1:0] axi_wr_aw_size,
  output logic [2-1:0] axi_wr_aw_burst,
  output logic axi_wr_w_valid,
  input logic axi_wr_w_ready,
  output logic [32-1:0] axi_wr_w_data,
  output logic [4-1:0] axi_wr_w_strb,
  output logic axi_wr_w_last,
  input logic axi_wr_b_valid,
  output logic axi_wr_b_ready,
  input logic [1-1:0] axi_wr_b_id,
  input logic [2-1:0] axi_wr_b_resp
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
  logic w_last_r;
  
  always_ff @(posedge clk) begin
    if (rst) begin
      state_r <= IDLE;
      beat_ctr_r <= 0;
      w_last_r <= 1'b0;
    end else begin
      state_r <= state_next;
      case (state_r)
        IDLE: begin
          // Control interface (from register block)
          // Status outputs
          // FIFO pop interface
          // AXI4 Write Master
          // Internal registers
          // Lookahead: w_last_r is true when the CURRENT beat is the last one.
          // Precomputed one cycle early so w_last is a FF output on the critical path,
          // not a combinational subtractor+comparator chain.
          if (start) begin
            dst_addr_r <= dst_addr;
            num_beats_r <= num_beats;
            beat_ctr_r <= 0;
          end
        end
        SENDAW: begin
          if (axi_wr_aw_ready) begin
            // Preload lookahead: beat 0 is last iff num_beats_r == 1
            w_last_r <= num_beats_r == 1;
          end
        end
        SENDW: begin
          if (axi_wr_w_ready & pop_valid) begin
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
        if (axi_wr_aw_ready) state_next = SENDW;
      end
      SENDW: begin
        if (axi_wr_w_ready & pop_valid & beat_ctr_r == 8'(num_beats_r - 1)) state_next = WAITB;
      end
      WAITB: begin
        if (axi_wr_b_valid) state_next = DONE;
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
    axi_wr_aw_valid = 1'b0;
    axi_wr_aw_addr = 0;
    axi_wr_aw_len = 0;
    axi_wr_aw_size = 0;
    axi_wr_aw_burst = 0;
    axi_wr_aw_id = 0;
    axi_wr_w_valid = 1'b0;
    axi_wr_w_data = 0;
    axi_wr_w_strb = 0;
    axi_wr_w_last = 1'b0;
    axi_wr_b_ready = 1'b0;
    case (state_r)
      IDLE: begin
        halted = 1'b1;
        idle_out = 1'b1;
      end
      WAITRECV: begin
      end
      SENDAW: begin
        axi_wr_aw_valid = 1'b1;
        axi_wr_aw_addr = dst_addr_r;
        axi_wr_aw_len = 8'(num_beats_r - 1);
        axi_wr_aw_size = 2;
        axi_wr_aw_burst = 1;
      end
      SENDW: begin
        axi_wr_w_valid = pop_valid;
        axi_wr_w_data = pop_data;
        axi_wr_w_strb = 'hF;
        axi_wr_w_last = beat_ctr_r == 8'(num_beats_r - 1);
        pop_ready = axi_wr_w_ready;
      end
      WAITB: begin
        axi_wr_b_ready = 1'b1;
      end
      DONE: begin
        done = 1'b1;
      end
      default: ;
    endcase
  end

endmodule

