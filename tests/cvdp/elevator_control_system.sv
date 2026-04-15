// Elevator control system
// param N: number of floors (default 8)
// system_status: 0=IDLE, 1=MOVING_UP, 2=MOVING_DOWN, 3=EMERGENCY
module elevator_control_system #(
  parameter int N = 8
) (
  input logic clk,
  input logic reset,
  input logic [N-1:0] call_requests,
  input logic emergency_stop,
  output logic [3:0] current_floor,
  output logic door_open,
  output logic [6:0] seven_seg_out,
  output logic [1:0] system_status
);

  // State encoding: 0=IDLE, 1=MOVING_UP, 2=MOVING_DOWN, 3=DOOR_OPEN, 4=EMERGENCY
  logic [2:0] state_r;
  logic [3:0] floor_r;
  logic [7:0] door_cnt;
  // Latch one-cycle call pulses so requests are not lost between cycles.
  logic [N-1:0] pending_r;
  // Seven-segment decode for current floor
  logic [6:0] seg;
  always_comb begin
    if (floor_r == 0) begin
      seg = 7'd126;
    end else if (floor_r == 1) begin
      seg = 7'd48;
    end else if (floor_r == 2) begin
      seg = 7'd109;
    end else if (floor_r == 3) begin
      seg = 7'd121;
    end else if (floor_r == 4) begin
      seg = 7'd51;
    end else if (floor_r == 5) begin
      seg = 7'd91;
    end else if (floor_r == 6) begin
      seg = 7'd95;
    end else if (floor_r == 7) begin
      seg = 7'd112;
    end else if (floor_r == 8) begin
      seg = 7'd127;
    end else begin
      seg = 7'd123;
    end
  end
  // Check for pending requests above/below/at current floor
  logic req_above;
  logic req_below;
  logic req_here;
  always_comb begin
    req_above = 1'b0;
    req_below = 1'b0;
    req_here = 1'b0;
    for (int i = 0; i <= N - 1; i++) begin
      if ((4'($unsigned(i)) > floor_r) & pending_r[i +: 1]) begin
        req_above = 1'b1;
      end
      if ((4'($unsigned(i)) < floor_r) & pending_r[i +: 1]) begin
        req_below = 1'b1;
      end
      if ((4'($unsigned(i)) == floor_r) & pending_r[i +: 1]) begin
        req_here = 1'b1;
      end
    end
  end
  // Output logic
  always_comb begin
    current_floor = floor_r;
    seven_seg_out = seg;
    if (state_r == 3) begin
      door_open = 1'b1;
    end else begin
      door_open = 1'b0;
    end
    if (state_r == 4) begin
      system_status = 3;
    end else if (state_r == 1) begin
      system_status = 1;
    end else if (state_r == 2) begin
      system_status = 2;
    end else begin
      system_status = 0;
    end
  end
  // Next state logic
  always_ff @(posedge clk) begin
    if (reset) begin
      state_r <= 0;
      floor_r <= 0;
      door_cnt <= 0;
      pending_r <= 0;
    end else begin
      // Capture incoming one-cycle requests.
      pending_r <= pending_r | call_requests;
      if (req_here) begin
        // Clear the request once this floor is being served.
        pending_r[floor_r +: 1] <= 0;
      end
      if (state_r == 0) begin
        // IDLE
        if (emergency_stop) begin
          state_r <= 4;
        end else if (req_here) begin
          state_r <= 3;
          door_cnt <= 50;
        end else if (req_above) begin
          state_r <= 1;
        end else if (req_below) begin
          state_r <= 2;
        end
      end else if (state_r == 1) begin
        // MOVING_UP
        if (emergency_stop) begin
          state_r <= 4;
        end else if (req_here) begin
          state_r <= 3;
          door_cnt <= 50;
        end else if (~req_above & ~req_here) begin
          state_r <= 0;
        end else if (pending_r[floor_r +: 1] == 0) begin
          floor_r <= 4'(floor_r + 1);
        end
      end else if (state_r == 2) begin
        // MOVING_DOWN
        if (emergency_stop) begin
          state_r <= 4;
        end else if (req_here) begin
          state_r <= 3;
          door_cnt <= 50;
        end else if (~req_below & ~req_here) begin
          state_r <= 0;
        end else if (pending_r[floor_r +: 1] == 0) begin
          floor_r <= 4'(floor_r - 1);
        end
      end else if (state_r == 3) begin
        // DOOR_OPEN
        if (emergency_stop) begin
          state_r <= 4;
        end else if (door_cnt == 0) begin
          if (req_above) begin
            state_r <= 1;
          end else if (req_below) begin
            state_r <= 2;
          end else begin
            state_r <= 0;
          end
        end else begin
          door_cnt <= 8'(door_cnt - 1);
        end
      end else if (state_r == 4) begin
        // EMERGENCY
        if (~emergency_stop) begin
          state_r <= 0;
        end
      end
    end
  end

endmodule

