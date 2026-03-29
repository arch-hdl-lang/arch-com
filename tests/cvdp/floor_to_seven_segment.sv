module floor_to_seven_segment (
  input logic clk,
  input logic [4-1:0] floor_display,
  output logic [7-1:0] seven_seg_out,
  output logic [4-1:0] seven_seg_out_anode,
  output logic [4-1:0] thousand,
  output logic [4-1:0] hundred,
  output logic [4-1:0] ten,
  output logic [4-1:0] one
);

  // BCD conversion
  logic [4-1:0] bcd_thou;
  logic [4-1:0] bcd_hund;
  logic [4-1:0] bcd_ten;
  logic [4-1:0] bcd_one;
  Binary2BCD u_bcd (
    .num(8'($unsigned(floor_display))),
    .thousand(bcd_thou),
    .hundred(bcd_hund),
    .ten(bcd_ten),
    .one(bcd_one)
  );
  assign thousand = bcd_thou;
  assign hundred = bcd_hund;
  assign ten = bcd_ten;
  assign one = bcd_one;
  // 2-bit counter for digit mux
  logic [2-1:0] digit_sel;
  always_ff @(posedge clk) begin
    digit_sel <= 2'(digit_sel + 1);
  end
  // Select which digit to display and which anode to activate
  logic [4-1:0] current_digit;
  always_comb begin
    if (digit_sel == 0) begin
      current_digit = bcd_one;
      seven_seg_out_anode = 14;
    end else if (digit_sel == 1) begin
      current_digit = bcd_ten;
      seven_seg_out_anode = 13;
    end else if (digit_sel == 2) begin
      current_digit = bcd_hund;
      seven_seg_out_anode = 11;
    end else begin
      current_digit = bcd_thou;
      seven_seg_out_anode = 7;
    end
  end
  // Seven-segment decoder
  always_comb begin
    if (current_digit == 0) begin
      seven_seg_out = 126;
    end else if (current_digit == 1) begin
      seven_seg_out = 48;
    end else if (current_digit == 2) begin
      seven_seg_out = 109;
    end else if (current_digit == 3) begin
      seven_seg_out = 121;
    end else if (current_digit == 4) begin
      seven_seg_out = 51;
    end else if (current_digit == 5) begin
      seven_seg_out = 91;
    end else if (current_digit == 6) begin
      seven_seg_out = 95;
    end else if (current_digit == 7) begin
      seven_seg_out = 112;
    end else if (current_digit == 8) begin
      seven_seg_out = 127;
    end else if (current_digit == 9) begin
      seven_seg_out = 123;
    end else begin
      seven_seg_out = 0;
    end
  end

endmodule

