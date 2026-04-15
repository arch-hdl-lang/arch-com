module password_generator #(
  parameter int WIDTH = 4
) (
  input logic clk,
  input logic reset,
  output logic [WIDTH * 8-1:0] password
);

  logic [7:0] cnt;
  logic [WIDTH-1:0] [7:0] char_array;
  always_ff @(posedge clk or posedge reset) begin
    if (reset) begin
      cnt <= 0;
    end else begin
      cnt <= 8'(cnt + 1);
    end
  end
  always_ff @(posedge clk or posedge reset) begin
    if (reset) begin
      for (int __ri0 = 0; __ri0 < WIDTH; __ri0++) begin
        char_array[__ri0] <= 0;
      end
      password <= 0;
    end else begin
      for (int i = 0; i <= WIDTH - 1; i++) begin
        if (i % 4 == 0) begin
          char_array[i] <= 8'((32'($unsigned(cnt)) + 32'($unsigned(char_array[(i + 1) % WIDTH]))) % 26 + 97);
          password[i * 8 +: 8] <= 8'((32'($unsigned(cnt)) + 32'($unsigned(char_array[(i + 1) % WIDTH]))) % 26 + 97);
        end else if (i % 4 == 1) begin
          char_array[i] <= 8'((32'($unsigned(cnt)) + i) % 26 + 65);
          password[i * 8 +: 8] <= 8'((32'($unsigned(cnt)) + i) % 26 + 65);
        end else if (i % 4 == 2) begin
          char_array[i] <= 8'((32'($unsigned(cnt)) + 32'($unsigned(char_array[((i + WIDTH) - 1) % WIDTH]))) % 14 + 33);
          password[i * 8 +: 8] <= 8'((32'($unsigned(cnt)) + 32'($unsigned(char_array[((i + WIDTH) - 1) % WIDTH]))) % 14 + 33);
        end else begin
          char_array[i] <= 8'((32'($unsigned(cnt)) + i) % 10 + 48);
          password[i * 8 +: 8] <= 8'((32'($unsigned(cnt)) + i) % 10 + 48);
        end
      end
    end
  end

endmodule

