module car_parking_system #(
  parameter int TOTAL_SPACES = 12
) (
  input logic clk,
  input logic reset,
  input logic vehicle_entry_sensor,
  input logic vehicle_exit_sensor,
  input logic [3:0] current_slot,
  input logic [31:0] current_time,
  input logic [4:0] hour_of_day,
  output logic [3:0] available_spaces,
  output logic [3:0] count_car,
  output logic led_status,
  output logic [15:0] parking_fee,
  output logic fee_ready,
  output logic [127:0] qr_code,
  output logic [6:0] seven_seg_display_available_tens,
  output logic [6:0] seven_seg_display_available_units,
  output logic [6:0] seven_seg_display_count_tens,
  output logic [6:0] seven_seg_display_count_units
);

  logic [31:0] entry_time_slot0;
  logic [31:0] entry_time_slot1;
  logic [31:0] entry_time_slot2;
  logic entry_seen_r;
  logic exit_seen_r;
  logic [3:0] avail_tens_digit;
  logic [3:0] count_tens_digit;
  logic [6:0] seg_avail_tens;
  logic [6:0] seg_avail_units;
  logic [6:0] seg_count_tens;
  logic [6:0] seg_count_units;
  logic [31:0] selected_entry_time;
  logic [15:0] rate_per_hour;
  logic [15:0] final_fee;
  always_comb begin
    if (available_spaces >= 10) begin
      avail_tens_digit = 1;
    end else begin
      avail_tens_digit = 0;
    end
  end
  logic [3:0] avail_tens_x10;
  assign avail_tens_x10 = 4'(avail_tens_digit * 10);
  logic [3:0] avail_units_digit;
  assign avail_units_digit = 4'(available_spaces - avail_tens_x10);
  always_comb begin
    if (count_car >= 10) begin
      count_tens_digit = 1;
    end else begin
      count_tens_digit = 0;
    end
  end
  logic [3:0] count_tens_x10;
  assign count_tens_x10 = 4'(count_tens_digit * 10);
  logic [3:0] count_units_digit;
  assign count_units_digit = 4'(count_car - count_tens_x10);
  always_comb begin
    if (current_slot == 0) begin
      selected_entry_time = entry_time_slot0;
    end else if (current_slot == 1) begin
      selected_entry_time = entry_time_slot1;
    end else if (current_slot == 2) begin
      selected_entry_time = entry_time_slot2;
    end else begin
      selected_entry_time = 0;
    end
  end
  logic [31:0] time_spent;
  assign time_spent = 32'(current_time - selected_entry_time);
  logic [15:0] rounded_hours;
  assign rounded_hours = 16'((time_spent + 3599) / 3600);
  always_comb begin
    if ((hour_of_day >= 8) & (hour_of_day <= 10)) begin
      rate_per_hour = 100;
    end else begin
      rate_per_hour = 50;
    end
  end
  logic [15:0] uncapped_fee;
  assign uncapped_fee = 16'(rounded_hours * rate_per_hour);
  logic [15:0] qr_time_field;
  assign qr_time_field = time_spent[15:0];
  always_comb begin
    if (uncapped_fee > 500) begin
      final_fee = 500;
    end else begin
      final_fee = uncapped_fee;
    end
  end
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
  always_ff @(posedge clk or posedge reset) begin
    if (reset) begin
      available_spaces <= TOTAL_SPACES;
      count_car <= 0;
      entry_seen_r <= 1'b0;
      entry_time_slot0 <= 0;
      entry_time_slot1 <= 0;
      entry_time_slot2 <= 0;
      exit_seen_r <= 1'b0;
      fee_ready <= 1'b0;
      led_status <= 1'b1;
      parking_fee <= 0;
      qr_code <= 0;
      seven_seg_display_available_tens <= 0;
      seven_seg_display_available_units <= 0;
      seven_seg_display_count_tens <= 0;
      seven_seg_display_count_units <= 0;
    end else begin
      if ((vehicle_entry_sensor | entry_seen_r) & (available_spaces > 0)) begin
        fee_ready <= 1'b0;
        available_spaces <= 4'(available_spaces - 1);
        count_car <= 4'(count_car + 1);
        if (current_slot == 0) begin
          entry_time_slot0 <= current_time;
        end else if (current_slot == 1) begin
          entry_time_slot1 <= current_time;
        end else if (current_slot == 2) begin
          entry_time_slot2 <= current_time;
        end
      end else if ((vehicle_exit_sensor | exit_seen_r) & (count_car > 0)) begin
        available_spaces <= 4'(available_spaces + 1);
        count_car <= 4'(count_car - 1);
        parking_fee <= final_fee;
        fee_ready <= 1'b1;
        qr_code <= 128'($unsigned(current_slot)) << 112 | 128'($unsigned(final_fee)) << 96 | 128'($unsigned(qr_time_field)) << 80;
      end
      entry_seen_r <= vehicle_entry_sensor;
      exit_seen_r <= vehicle_exit_sensor;
      led_status <= available_spaces > 0;
      seven_seg_display_available_tens <= seg_avail_tens;
      seven_seg_display_available_units <= seg_avail_units;
      seven_seg_display_count_tens <= seg_count_tens;
      seven_seg_display_count_units <= seg_count_units;
    end
  end

endmodule

