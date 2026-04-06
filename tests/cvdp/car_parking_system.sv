module car_parking_system #(
  parameter int TOTAL_SPACES = 12
) (
  input logic clk,
  input logic reset,
  input logic vehicle_entry_sensor,
  input logic vehicle_exit_sensor,
  output logic [4-1:0] available_spaces,
  output logic [4-1:0] count_car,
  output logic led_status,
  output logic [7-1:0] seven_seg_display_available_tens,
  output logic [7-1:0] seven_seg_display_available_units,
  output logic [7-1:0] seven_seg_display_count_tens,
  output logic [7-1:0] seven_seg_display_count_units
);

  logic [2-1:0] state;
  logic [4-1:0] avail_tens_digit;
  logic [4-1:0] avail_units_digit;
  logic [4-1:0] count_tens_digit;
  logic [4-1:0] count_units_digit;
  logic [7-1:0] seg_avail_tens;
  logic [7-1:0] seg_avail_units;
  logic [7-1:0] seg_count_tens;
  logic [7-1:0] seg_count_units;
  always_comb begin
    if (available_spaces >= 10) begin
      avail_tens_digit = 1;
    end else begin
      avail_tens_digit = 0;
    end
  end
  logic [4-1:0] avail_tens_x10;
  assign avail_tens_x10 = 4'(avail_tens_digit * 10);
  assign avail_units_digit = 4'(available_spaces - avail_tens_x10);
  always_comb begin
    if (count_car >= 10) begin
      count_tens_digit = 1;
    end else begin
      count_tens_digit = 0;
    end
  end
  logic [4-1:0] count_tens_x10;
  assign count_tens_x10 = 4'(count_tens_digit * 10);
  assign count_units_digit = 4'(count_car - count_tens_x10);
  always_comb begin
    if (avail_tens_digit == 0) begin
      seg_avail_tens = 126;
    end else if (avail_tens_digit == 1) begin
      seg_avail_tens = 48;
    end else begin
      seg_avail_tens = 0;
    end
  end
  always_comb begin
    if (avail_units_digit == 0) begin
      seg_avail_units = 126;
    end else if (avail_units_digit == 1) begin
      seg_avail_units = 48;
    end else if (avail_units_digit == 2) begin
      seg_avail_units = 109;
    end else if (avail_units_digit == 3) begin
      seg_avail_units = 121;
    end else if (avail_units_digit == 4) begin
      seg_avail_units = 51;
    end else if (avail_units_digit == 5) begin
      seg_avail_units = 91;
    end else if (avail_units_digit == 6) begin
      seg_avail_units = 95;
    end else if (avail_units_digit == 7) begin
      seg_avail_units = 112;
    end else if (avail_units_digit == 8) begin
      seg_avail_units = 127;
    end else if (avail_units_digit == 9) begin
      seg_avail_units = 123;
    end else begin
      seg_avail_units = 0;
    end
  end
  always_comb begin
    if (count_tens_digit == 0) begin
      seg_count_tens = 126;
    end else if (count_tens_digit == 1) begin
      seg_count_tens = 48;
    end else begin
      seg_count_tens = 0;
    end
  end
  always_comb begin
    if (count_units_digit == 0) begin
      seg_count_units = 126;
    end else if (count_units_digit == 1) begin
      seg_count_units = 48;
    end else if (count_units_digit == 2) begin
      seg_count_units = 109;
    end else if (count_units_digit == 3) begin
      seg_count_units = 121;
    end else if (count_units_digit == 4) begin
      seg_count_units = 51;
    end else if (count_units_digit == 5) begin
      seg_count_units = 91;
    end else if (count_units_digit == 6) begin
      seg_count_units = 95;
    end else if (count_units_digit == 7) begin
      seg_count_units = 112;
    end else if (count_units_digit == 8) begin
      seg_count_units = 127;
    end else if (count_units_digit == 9) begin
      seg_count_units = 123;
    end else begin
      seg_count_units = 0;
    end
  end
  // FSM: 0=Idle, 1=Entry, 2=Exit, 3=Full
  always_ff @(posedge clk or posedge reset) begin
    if (reset) begin
      available_spaces <= TOTAL_SPACES;
      count_car <= 0;
      led_status <= 1'b1;
      seven_seg_display_available_tens <= 0;
      seven_seg_display_available_units <= 0;
      seven_seg_display_count_tens <= 0;
      seven_seg_display_count_units <= 0;
      state <= 0;
    end else begin
      if (state == 0) begin
        if (vehicle_entry_sensor & available_spaces > 0) begin
          available_spaces <= 4'(available_spaces - 1);
          count_car <= 4'(count_car + 1);
          state <= 1;
        end else if (vehicle_entry_sensor & available_spaces == 0) begin
          state <= 3;
        end else if (vehicle_exit_sensor & count_car > 0) begin
          available_spaces <= 4'(available_spaces + 1);
          count_car <= 4'(count_car - 1);
          state <= 2;
        end
      end else if (state == 1) begin
        if (~vehicle_entry_sensor) begin
          state <= 0;
        end
      end else if (state == 2) begin
        if (~vehicle_exit_sensor) begin
          state <= 0;
        end
      end else if (state == 3) begin
        if (vehicle_exit_sensor & count_car > 0) begin
          available_spaces <= 4'(available_spaces + 1);
          count_car <= 4'(count_car - 1);
          state <= 2;
        end else if (~vehicle_entry_sensor) begin
          state <= 0;
        end
      end
      led_status <= available_spaces > 0;
      seven_seg_display_available_tens <= seg_avail_tens;
      seven_seg_display_available_units <= seg_avail_units;
      seven_seg_display_count_tens <= seg_count_tens;
      seven_seg_display_count_units <= seg_count_units;
    end
  end

endmodule

