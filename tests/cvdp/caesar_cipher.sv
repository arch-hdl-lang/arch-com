module caesar_cipher (
  input logic [8-1:0] input_char,
  input logic [4-1:0] key,
  output logic [8-1:0] output_char
);

  logic [8-1:0] key_ext;
  assign key_ext = 8'($unsigned(key));
  logic [9-1:0] upper_off;
  assign upper_off = input_char - 65;
  logic [9-1:0] lower_off;
  assign lower_off = input_char - 97;
  logic [10-1:0] upper_sum;
  assign upper_sum = upper_off + 9'($unsigned(key_ext));
  logic [10-1:0] lower_sum;
  assign lower_sum = lower_off + 9'($unsigned(key_ext));
  logic [10-1:0] upper_mod;
  assign upper_mod = upper_sum % 26;
  logic [10-1:0] lower_mod;
  assign lower_mod = lower_sum % 26;
  logic [11-1:0] upper_shifted;
  assign upper_shifted = upper_mod + 65;
  logic [11-1:0] lower_shifted;
  assign lower_shifted = lower_mod + 97;
  logic is_upper;
  assign is_upper = input_char >= 65 & input_char < 91;
  logic is_lower;
  assign is_lower = input_char >= 97 & input_char < 123;
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

