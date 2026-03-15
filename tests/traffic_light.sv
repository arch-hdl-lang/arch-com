// domain SysDomain
//   freq_mhz: 100

module TrafficLight #(
  parameter int TIMER_W = 8
) (
  input logic clk,
  input logic rst,
  input logic [TIMER_W-1:0] timer,
  output logic red,
  output logic yellow,
  output logic green
);

  typedef enum logic [1:0] {
    RED = 2'd0,
    YELLOW = 2'd1,
    GREEN = 2'd2
  } TrafficLight_state_t;
  
  TrafficLight_state_t state_r, state_next;
  
  always_ff @(posedge clk) begin
    if (rst) begin
      state_r <= RED;
    end else begin
      state_r <= state_next;
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    case (state_r)
      RED: begin
        unique if ((timer == 0)) state_next = GREEN;
      end
      GREEN: begin
        unique if ((timer == 0)) state_next = YELLOW;
      end
      YELLOW: begin
        unique if ((timer == 0)) state_next = RED;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    red = '0; // default
    yellow = '0; // default
    green = '0; // default
    case (state_r)
      RED: begin
        red = 1'b1;
        yellow = 1'b0;
        green = 1'b0;
      end
      GREEN: begin
        red = 1'b0;
        yellow = 1'b0;
        green = 1'b1;
      end
      YELLOW: begin
        red = 1'b0;
        yellow = 1'b1;
        green = 1'b0;
      end
      default: ;
    endcase
  end

endmodule

