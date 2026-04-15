module TrafficLight (
  input logic clk,
  input logic rst,
  input logic tick,
  output logic [1:0] light
);

  typedef enum logic [1:0] {
    RED = 2'd0,
    GREEN = 2'd1,
    YELLOW = 2'd2
  } TrafficLight_state_t;
  
  TrafficLight_state_t state_r, state_next;
  
  always_ff @(posedge clk or posedge rst) begin
    if (rst) begin
      state_r <= RED;
      light <= 0;
    end else begin
      state_r <= state_next;
      case (state_r)
        RED: begin
          light <= 0;
        end
        GREEN: begin
          light <= 2;
        end
        YELLOW: begin
          light <= 1;
        end
        default: ;
      endcase
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    case (state_r)
      RED: begin
        if (tick) state_next = GREEN;
      end
      GREEN: begin
        if (tick) state_next = YELLOW;
      end
      YELLOW: begin
        if (tick) state_next = RED;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    case (state_r)
      RED: begin
      end
      GREEN: begin
      end
      YELLOW: begin
      end
      default: ;
    endcase
  end
  
  // synopsys translate_off
  _auto_legal_state: assert property (@(posedge clk) !rst |-> state_r < 3)
    else $fatal(1, "FSM ILLEGAL STATE: TrafficLight.state_r = %0d", state_r);
  _auto_reach_Red: cover property (@(posedge clk) state_r == RED);
  _auto_reach_Green: cover property (@(posedge clk) state_r == GREEN);
  _auto_reach_Yellow: cover property (@(posedge clk) state_r == YELLOW);
  _auto_tr_RED_to_GREEN: cover property (@(posedge clk) state_r == RED && state_next == GREEN);
  _auto_tr_GREEN_to_YELLOW: cover property (@(posedge clk) state_r == GREEN && state_next == YELLOW);
  _auto_tr_YELLOW_to_RED: cover property (@(posedge clk) state_r == YELLOW && state_next == RED);
  // synopsys translate_on

endmodule

