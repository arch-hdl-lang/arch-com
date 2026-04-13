module morse_encoder (
  input logic [7:0] ascii_in,
  output logic [9:0] morse_out,
  output logic [3:0] morse_length
);

  logic [9:0] code;
  logic [3:0] len;
  always_comb begin
    if (ascii_in == 65) begin
      code = 'b1;
      len = 2;
    end else if (ascii_in == 66) begin
      code = 'b1000;
      len = 4;
    end else if (ascii_in == 67) begin
      code = 'b1010;
      len = 4;
    end else if (ascii_in == 68) begin
      code = 'b100;
      len = 3;
    end else if (ascii_in == 69) begin
      code = 'b0;
      len = 1;
    end else if (ascii_in == 70) begin
      code = 'b10;
      len = 4;
    end else if (ascii_in == 71) begin
      code = 'b110;
      len = 3;
    end else if (ascii_in == 72) begin
      code = 'b0;
      len = 4;
    end else if (ascii_in == 73) begin
      code = 'b0;
      len = 2;
    end else if (ascii_in == 74) begin
      code = 'b111;
      len = 4;
    end else if (ascii_in == 75) begin
      code = 'b101;
      len = 3;
    end else if (ascii_in == 76) begin
      code = 'b100;
      len = 4;
    end else if (ascii_in == 77) begin
      code = 'b11;
      len = 2;
    end else if (ascii_in == 78) begin
      code = 'b10;
      len = 2;
    end else if (ascii_in == 79) begin
      code = 'b111;
      len = 3;
    end else if (ascii_in == 80) begin
      code = 'b110;
      len = 4;
    end else if (ascii_in == 81) begin
      code = 'b1101;
      len = 4;
    end else if (ascii_in == 82) begin
      code = 'b10;
      len = 3;
    end else if (ascii_in == 83) begin
      code = 'b0;
      len = 3;
    end else if (ascii_in == 84) begin
      code = 'b1;
      len = 1;
    end else if (ascii_in == 85) begin
      code = 'b1;
      len = 3;
    end else if (ascii_in == 86) begin
      code = 'b1;
      len = 4;
    end else if (ascii_in == 87) begin
      code = 'b11;
      len = 3;
    end else if (ascii_in == 88) begin
      code = 'b1001;
      len = 4;
    end else if (ascii_in == 89) begin
      code = 'b1011;
      len = 4;
    end else if (ascii_in == 90) begin
      code = 'b1100;
      len = 4;
    end else if (ascii_in == 48) begin
      code = 'b11111;
      len = 5;
    end else if (ascii_in == 49) begin
      code = 'b1111;
      len = 5;
    end else if (ascii_in == 50) begin
      code = 'b111;
      len = 5;
    end else if (ascii_in == 51) begin
      code = 'b11;
      len = 5;
    end else if (ascii_in == 52) begin
      code = 'b1;
      len = 5;
    end else if (ascii_in == 53) begin
      code = 'b0;
      len = 5;
    end else if (ascii_in == 54) begin
      code = 'b10000;
      len = 5;
    end else if (ascii_in == 55) begin
      code = 'b11000;
      len = 5;
    end else if (ascii_in == 56) begin
      code = 'b11100;
      len = 5;
    end else if (ascii_in == 57) begin
      code = 'b11110;
      len = 5;
    end else begin
      code = 0;
      len = 0;
    end
    morse_out = code;
    morse_length = len;
  end

endmodule

