// Simple FSM with datapath registers to test reg/seq in FSM
// domain SysDomain
//   freq_mhz: 100

module FsmCounter (
  input logic clk,
  input logic rst,
  input logic go,
  input logic [8-1:0] target,
  output logic done,
  output logic [8-1:0] count
);

  typedef enum logic [1:0] {
    IDLE = 2'd0,
    COUNTING = 2'd1,
    DONE = 2'd2
  } FsmCounter_state_t;
  
  FsmCounter_state_t state_r, state_next;
  
  logic [8-1:0] cnt_r;
  logic [8-1:0] tgt_r;
  
  always_ff @(posedge clk) begin
    if (rst) begin
      state_r <= IDLE;
      cnt_r <= 0;
      tgt_r <= 0;
    end else begin
      state_r <= state_next;
      case (state_r)
        IDLE: begin
          if (go) begin
            cnt_r <= 0;
            tgt_r <= target;
          end
        end
        COUNTING: begin
          cnt_r <= 8'(cnt_r + 1);
        end
        default: ;
      endcase
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    case (state_r)
      IDLE: begin
        if (go) state_next = COUNTING;
      end
      COUNTING: begin
        if (cnt_r == tgt_r) state_next = DONE;
      end
      DONE: begin
        state_next = IDLE;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    done = 1'b0; // default
    count = 0; // default
    case (state_r)
      IDLE: begin
        done = 1'b0;
        count = cnt_r;
      end
      COUNTING: begin
        done = 1'b0;
        count = cnt_r;
      end
      DONE: begin
        done = 1'b1;
        count = cnt_r;
      end
      default: ;
    endcase
  end

endmodule

// no fallthrough needed — compiler defaults to stay in Idle
// implicit stay in Counting
