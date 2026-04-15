// Simple floor number to 7-segment display decoder
// Maps floor number 0-9 to 7-segment patterns
module floor_to_seven_segment (
  input logic [3:0] floor_in,
  output logic [6:0] seven_seg
);

  always_comb begin
    if (floor_in == 0) begin
      seven_seg = 7'd126;
    end else if (floor_in == 1) begin
      seven_seg = 7'd48;
    end else if (floor_in == 2) begin
      seven_seg = 7'd109;
    end else if (floor_in == 3) begin
      seven_seg = 7'd121;
    end else if (floor_in == 4) begin
      seven_seg = 7'd51;
    end else if (floor_in == 5) begin
      seven_seg = 7'd91;
    end else if (floor_in == 6) begin
      seven_seg = 7'd95;
    end else if (floor_in == 7) begin
      seven_seg = 7'd112;
    end else if (floor_in == 8) begin
      seven_seg = 7'd127;
    end else if (floor_in == 9) begin
      seven_seg = 7'd123;
    end else begin
      seven_seg = 7'd0;
    end
  end

endmodule

