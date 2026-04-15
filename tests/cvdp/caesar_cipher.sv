module caesar_cipher (
  input logic [7:0] input_char,
  input logic [3:0] key,
  output logic [7:0] output_char
);

  logic [7:0] key_ext;
  assign key_ext = 8'($unsigned(key));
  logic [8:0] upper_off;
  assign upper_off = input_char - 65;
  logic [8:0] lower_off;
  assign lower_off = input_char - 97;
  logic [9:0] upper_sum;
  assign upper_sum = upper_off + 9'($unsigned(key_ext));
  logic [9:0] lower_sum;
  assign lower_sum = lower_off + 9'($unsigned(key_ext));
  logic [9:0] upper_mod;
  assign upper_mod = upper_sum % 26;
  logic [9:0] lower_mod;
  assign lower_mod = lower_sum % 26;
  logic [10:0] upper_shifted;
  assign upper_shifted = upper_mod + 65;
  logic [10:0] lower_shifted;
  assign lower_shifted = lower_mod + 97;
  logic is_upper;
  assign is_upper = (input_char >= 65) & (input_char < 91);
  logic is_lower;
  assign is_lower = (input_char >= 97) & (input_char < 123);
  always_comb begin
    if (is_upper) begin
      output_char = 8'(upper_shifted);
    end else if (is_lower) begin
      output_char = 8'(lower_shifted);
    end else begin
      output_char = input_char;
    end
  end

endmodule

