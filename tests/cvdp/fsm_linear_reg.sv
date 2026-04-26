// 3-state linear-regression accumulator: idle until `start`, compute for
// one cycle, then assert `done`.
module fsm_linear_reg #(
  parameter int DATA_WIDTH = 16
) (
  input logic clk,
  input logic reset,
  input logic start,
  input logic signed [DATA_WIDTH-1:0] x_in,
  input logic signed [DATA_WIDTH-1:0] w_in,
  input logic signed [DATA_WIDTH-1:0] b_in,
  output logic signed [DATA_WIDTH * 2-1:0] result1,
  output logic signed [DATA_WIDTH + 1-1:0] result2,
  output logic done
);

  typedef enum logic [1:0] {
    IDLE = 2'd0,
    COMPUTE = 2'd1,
    DONE = 2'd2
  } fsm_linear_reg_state_t;
  
  fsm_linear_reg_state_t state_r, state_next;
  
  logic signed [DATA_WIDTH * 2-1:0] buf_result1;
  logic signed [DATA_WIDTH + 1-1:0] buf_result2;
  logic buf_done;
  
  logic signed [DATA_WIDTH-1:0] x_shifted;
  assign x_shifted = x_in >>> 2;
  logic signed [DATA_WIDTH * 2-1:0] w_ext;
  assign w_ext = {{(DATA_WIDTH * 2-$bits(w_in)){w_in[$bits(w_in)-1]}}, w_in};
  logic signed [DATA_WIDTH * 2-1:0] x_ext;
  assign x_ext = {{(DATA_WIDTH * 2-$bits(x_in)){x_in[$bits(x_in)-1]}}, x_in};
  logic signed [DATA_WIDTH * 2-1:0] product;
  assign product = (DATA_WIDTH * 2)'(w_ext * x_ext);
  logic signed [DATA_WIDTH + 1-1:0] b_ext;
  assign b_ext = {{(DATA_WIDTH + 1-$bits(b_in)){b_in[$bits(b_in)-1]}}, b_in};
  logic signed [DATA_WIDTH + 1-1:0] xs_ext;
  assign xs_ext = {{(DATA_WIDTH + 1-$bits(x_shifted)){x_shifted[$bits(x_shifted)-1]}}, x_shifted};
  
  always_ff @(posedge clk or posedge reset) begin
    if (reset) begin
      state_r <= IDLE;
      buf_result1 <= 0;
      buf_result2 <= 0;
      buf_done <= 1'b0;
    end else begin
      state_r <= state_next;
      case (state_r)
        IDLE: begin
          // Datapath registers — drive the output ports directly via `comb`.
          // Combinational MAC — only the seq capture is gated by state.
          buf_result1 <= 0;
          buf_result2 <= 0;
          buf_done <= 1'b0;
        end
        COMPUTE: begin
          buf_result1 <= product >>> 1;
          buf_result2 <= (DATA_WIDTH + 1)'(b_ext + xs_ext);
          buf_done <= 1'b0;
        end
        DONE: begin
          buf_done <= 1'b1;
        end
        default: ;
      endcase
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    case (state_r)
      IDLE: begin
        if (start) state_next = COMPUTE;
      end
      COMPUTE: begin
        state_next = DONE;
      end
      DONE: begin
        state_next = IDLE;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    result1 = buf_result1;
    result2 = buf_result2;
    done = buf_done;
    case (state_r)
      IDLE: begin
      end
      COMPUTE: begin
      end
      DONE: begin
      end
      default: ;
    endcase
  end
  
  // synopsys translate_off
  _auto_legal_state: assert property (@(posedge clk) !reset |-> state_r < 3)
    else $fatal(1, "FSM ILLEGAL STATE: fsm_linear_reg.state_r = %0d", state_r);
  _auto_reach_Idle: cover property (@(posedge clk) state_r == IDLE);
  _auto_reach_Compute: cover property (@(posedge clk) state_r == COMPUTE);
  _auto_reach_Done: cover property (@(posedge clk) state_r == DONE);
  _auto_tr_IDLE_to_COMPUTE: cover property (@(posedge clk) state_r == IDLE && state_next == COMPUTE);
  _auto_tr_COMPUTE_to_DONE: cover property (@(posedge clk) state_r == COMPUTE && state_next == DONE);
  _auto_tr_DONE_to_IDLE: cover property (@(posedge clk) state_r == DONE && state_next == IDLE);
  // synopsys translate_on

endmodule

