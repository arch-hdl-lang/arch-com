module caesar_cipher (
  input logic [8-1:0] input_char,
  input logic [4-1:0] key,
  output logic [8-1:0] output_char
);

  logic is_upper;
  assign is_upper = input_char >= 8'd65 & input_char < 8'd91;
  logic is_lower;
  assign is_lower = input_char >= 8'd97 & input_char < 8'd123;
  logic [8-1:0] key8;
  assign key8 = 8'($unsigned(key));
  logic [8-1:0] upper_shifted;
  assign upper_shifted = 8'(8'(input_char - 8'd65 + key8) % 8'd26 + 8'd65);
  logic [8-1:0] lower_shifted;
  assign lower_shifted = 8'(8'(input_char - 8'd97 + key8) % 8'd26 + 8'd97);
  always_comb begin
    if (is_upper) begin
      output_char = upper_shifted;
    end else if (is_lower) begin
      output_char = lower_shifted;
    end else begin
      output_char = input_char;
    end
  end

endmodule

