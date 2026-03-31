module elevator_control_system #(
  parameter int NUM_FLOORS = 4
) (
  input logic clk,
  input logic rst_n,
  input logic [NUM_FLOORS-1:0] floor_request,
  output logic [2-1:0] current_floor,
  output logic door_open,
  output logic [7-1:0] seven_seg_out
);

  typedef enum logic [1:0] {
    IDLE = 2'd0,
    MOVING_UP = 2'd1,
    MOVING_DOWN = 2'd2,
    DOOR_OPEN = 2'd3
  } elevator_control_system_state_t;
  
  elevator_control_system_state_t state_r, state_next;
  
  logic [2-1:0] floor_r;
  logic [4-1:0] door_cnt;
  
  logic [7-1:0] seg;
  assign seg = floor_r == 0 ? 7'd126 : floor_r == 1 ? 7'd48 : floor_r == 2 ? 7'd109 : 7'd121;
  logic req_any_above;
  assign req_any_above = floor_r == 0 ? floor_request[3:1] != 0 : floor_r == 1 ? floor_request[3:2] != 0 : floor_r == 2 ? floor_request[3:3] != 0 : 1'b0;
  logic req_any_below;
  assign req_any_below = floor_r == 3 ? floor_request[2:0] != 0 : floor_r == 2 ? floor_request[1:0] != 0 : floor_r == 1 ? floor_request[0:0] != 0 : 1'b0;
  logic at_floor_req;
  assign at_floor_req = floor_request[floor_r] == 1;
  
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      state_r <= IDLE;
      floor_r <= 0;
      door_cnt <= 0;
    end else begin
      state_r <= state_next;
      case (state_r)
        MOVING_UP: begin
          floor_r <= floor_r + 1;
        end
        MOVING_DOWN: begin
          floor_r <= floor_r - 1;
        end
        DOOR_OPEN: begin
          if (door_cnt == 0) begin
            door_cnt <= 8;
          end else begin
            door_cnt <= door_cnt - 1;
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
        if (at_floor_req) state_next = DOOR_OPEN;
        else if (req_any_above) state_next = MOVING_UP;
        else if (req_any_below) state_next = MOVING_DOWN;
      end
      MOVING_UP: begin
        if (req_any_above == 1'b0) state_next = IDLE;
      end
      MOVING_DOWN: begin
        if (req_any_below == 1'b0) state_next = IDLE;
      end
      DOOR_OPEN: begin
        if (door_cnt == 1) state_next = IDLE;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    case (state_r)
      IDLE: begin
        current_floor = floor_r;
        seven_seg_out = seg;
        door_open = 1'b0;
      end
      MOVING_UP: begin
        current_floor = floor_r;
        seven_seg_out = seg;
        door_open = 1'b0;
      end
      MOVING_DOWN: begin
        current_floor = floor_r;
        seven_seg_out = seg;
        door_open = 1'b0;
      end
      DOOR_OPEN: begin
        current_floor = floor_r;
        seven_seg_out = seg;
        door_open = 1'b1;
      end
      default: ;
    endcase
  end

endmodule

