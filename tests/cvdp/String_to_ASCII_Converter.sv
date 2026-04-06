module String_to_ASCII_Converter (
  input logic clk,
  input logic reset,
  input logic start,
  input logic [8-1:0] char_in [8-1:0],
  output logic [8-1:0] ascii_out [8-1:0],
  output logic valid,
  output logic ready
);

  logic [8-1:0] ascii_reg [8-1:0];
  logic valid_reg;
  logic ready_reg;
  assign ascii_out = ascii_reg;
  assign valid = valid_reg;
  assign ready = ready_reg;
  // Conversion function: map encoded value to ASCII
  // 0..9   -> 48..57
  // 10..35 -> 65..90
  // 36..61 -> 97..122
  // 62..95 -> 33..66
  logic [8-1:0] conv0;
  logic [8-1:0] conv1;
  logic [8-1:0] conv2;
  logic [8-1:0] conv3;
  logic [8-1:0] conv4;
  logic [8-1:0] conv5;
  logic [8-1:0] conv6;
  logic [8-1:0] conv7;
  assign conv0 = char_in[0] < 10 ? 8'(char_in[0] + 48) : char_in[0] < 36 ? 8'(char_in[0] + 55) : char_in[0] < 62 ? 8'(char_in[0] + 61) : 8'(char_in[0] - 29);
  assign conv1 = char_in[1] < 10 ? 8'(char_in[1] + 48) : char_in[1] < 36 ? 8'(char_in[1] + 55) : char_in[1] < 62 ? 8'(char_in[1] + 61) : 8'(char_in[1] - 29);
  assign conv2 = char_in[2] < 10 ? 8'(char_in[2] + 48) : char_in[2] < 36 ? 8'(char_in[2] + 55) : char_in[2] < 62 ? 8'(char_in[2] + 61) : 8'(char_in[2] - 29);
  assign conv3 = char_in[3] < 10 ? 8'(char_in[3] + 48) : char_in[3] < 36 ? 8'(char_in[3] + 55) : char_in[3] < 62 ? 8'(char_in[3] + 61) : 8'(char_in[3] - 29);
  assign conv4 = char_in[4] < 10 ? 8'(char_in[4] + 48) : char_in[4] < 36 ? 8'(char_in[4] + 55) : char_in[4] < 62 ? 8'(char_in[4] + 61) : 8'(char_in[4] - 29);
  assign conv5 = char_in[5] < 10 ? 8'(char_in[5] + 48) : char_in[5] < 36 ? 8'(char_in[5] + 55) : char_in[5] < 62 ? 8'(char_in[5] + 61) : 8'(char_in[5] - 29);
  assign conv6 = char_in[6] < 10 ? 8'(char_in[6] + 48) : char_in[6] < 36 ? 8'(char_in[6] + 55) : char_in[6] < 62 ? 8'(char_in[6] + 61) : 8'(char_in[6] - 29);
  assign conv7 = char_in[7] < 10 ? 8'(char_in[7] + 48) : char_in[7] < 36 ? 8'(char_in[7] + 55) : char_in[7] < 62 ? 8'(char_in[7] + 61) : 8'(char_in[7] - 29);
  always_ff @(posedge clk) begin
    if (reset) begin
      for (int __ri0 = 0; __ri0 < 8; __ri0++) begin
        ascii_reg[__ri0] <= 0;
      end
      ready_reg <= 1'b1;
      valid_reg <= 1'b0;
    end else begin
      if (start) begin
        ascii_reg[0] <= conv0;
        ascii_reg[1] <= conv1;
        ascii_reg[2] <= conv2;
        ascii_reg[3] <= conv3;
        ascii_reg[4] <= conv4;
        ascii_reg[5] <= conv5;
        ascii_reg[6] <= conv6;
        ascii_reg[7] <= conv7;
        valid_reg <= 1'b1;
        ready_reg <= 1'b0;
      end else begin
        for (int i = 0; i <= 7; i++) begin
          ascii_reg[i] <= 0;
        end
        valid_reg <= 1'b0;
        ready_reg <= 1'b1;
      end
    end
  end

endmodule

