module elevator_control_system #(
  parameter int N = 8
) (
  input logic clk,
  input logic reset,
  input logic [N-1:0] call_requests,
  input logic emergency_stop,
  output logic [7:0] current_floor,
  output logic door_open,
  output logic [6:0] seven_seg_out,
  output logic [3:0] seven_seg_out_anode,
  output logic [1:0] system_status
);

  logic [7:0] floor_r;
  logic [7:0] target_floor_r;
  logic request_active_r;
  logic door_open_r;
  logic [3:0] door_timer_r;
  logic [1:0] scan_sel_r;
  logic [7:0] requested_floor;
  logic have_request;
  logic [3:0] current_digit;
  logic [6:0] seg_digit;
  always_comb begin
    requested_floor = 0;
    have_request = 1'b0;
    for (int i = 0; i <= N - 1; i++) begin
      if (call_requests[i +: 1]) begin
        requested_floor = 8'($unsigned(i));
        have_request = 1'b1;
      end
    end
  end
  logic [3:0] ones_digit;
  assign ones_digit = 4'(floor_r % 10);
  logic [3:0] tens_digit;
  assign tens_digit = 4'((floor_r / 10) % 10);
  logic [3:0] hundreds_digit;
  assign hundreds_digit = 4'((floor_r / 100) % 10);
  always_comb begin
    if (scan_sel_r == 0) begin
      current_digit = ones_digit;
      seven_seg_out_anode = 4'd14;
    end else if (scan_sel_r == 1) begin
      current_digit = tens_digit;
      seven_seg_out_anode = 4'd13;
    end else begin
      current_digit = hundreds_digit;
      seven_seg_out_anode = 4'd11;
    end
  end
  always_comb begin
    if (current_digit == 0) begin
      seg_digit = 7'd126;
    end else if (current_digit == 1) begin
      seg_digit = 7'd48;
    end else if (current_digit == 2) begin
      seg_digit = 7'd109;
    end else if (current_digit == 3) begin
      seg_digit = 7'd121;
    end else if (current_digit == 4) begin
      seg_digit = 7'd51;
    end else if (current_digit == 5) begin
      seg_digit = 7'd91;
    end else if (current_digit == 6) begin
      seg_digit = 7'd95;
    end else if (current_digit == 7) begin
      seg_digit = 7'd112;
    end else if (current_digit == 8) begin
      seg_digit = 7'd127;
    end else begin
      seg_digit = 7'd123;
    end
  end
  always_comb begin
    current_floor = floor_r;
    door_open = door_open_r;
    seven_seg_out = seg_digit;
    if (emergency_stop) begin
      system_status = 3;
    end else if (door_open_r) begin
      system_status = 0;
    end else if (request_active_r & (floor_r < target_floor_r)) begin
      system_status = 1;
    end else if (request_active_r & (floor_r > target_floor_r)) begin
      system_status = 2;
    end else begin
      system_status = 0;
    end
  end
  always_ff @(posedge clk) begin
    if (reset) begin
      door_open_r <= 1'b0;
      door_timer_r <= 0;
      floor_r <= 0;
      request_active_r <= 1'b0;
      scan_sel_r <= 0;
      target_floor_r <= 0;
    end else begin
      if (scan_sel_r == 2) begin
        scan_sel_r <= 0;
      end else begin
        scan_sel_r <= 2'(scan_sel_r + 1);
      end
      if (emergency_stop) begin
        request_active_r <= 1'b0;
        door_open_r <= 1'b0;
        door_timer_r <= 0;
      end else if (door_open_r) begin
        if (door_timer_r == 0) begin
          door_open_r <= 1'b0;
          request_active_r <= 1'b0;
        end else begin
          door_timer_r <= 4'(door_timer_r - 1);
        end
      end else if (request_active_r) begin
        if (floor_r < target_floor_r) begin
          floor_r <= 8'(floor_r + 1);
        end else if (floor_r > target_floor_r) begin
          floor_r <= 8'(floor_r - 1);
        end else begin
          door_open_r <= 1'b1;
          door_timer_r <= 4;
        end
      end else if (have_request) begin
        target_floor_r <= requested_floor;
        request_active_r <= 1'b1;
        if (requested_floor == floor_r) begin
          door_open_r <= 1'b1;
          door_timer_r <= 4;
        end
      end
    end
  end

endmodule

