module barrel_shifter #(
  parameter int data_width = 16,
  parameter int shift_bits_width = 4
) (
  input logic [data_width-1:0] data_in,
  input logic [shift_bits_width-1:0] shift_bits,
  input logic left_right,
  input logic rotate_left_right,
  output logic [data_width-1:0] data_out
);

  logic [data_width-1:0] shift_amt;
  logic [data_width-1:0] rev_amt;
  logic [data_width-1:0] shifted_left;
  logic [data_width-1:0] shifted_right;
  logic [data_width-1:0] rotated_left;
  logic [data_width-1:0] rotated_right;
  always_comb begin
    shift_amt = data_width'($unsigned(shift_bits));
    rev_amt = data_width'(data_width - shift_amt);
    shifted_left = data_in << shift_amt;
    shifted_right = data_in >> shift_amt;
    rotated_left = data_in << shift_amt | data_in >> rev_amt;
    rotated_right = data_in >> shift_amt | data_in << rev_amt;
    if (rotate_left_right == 1) begin
      if (left_right == 1) begin
        data_out = rotated_left;
      end else begin
        data_out = rotated_right;
      end
    end else if (left_right == 1) begin
      data_out = shifted_left;
    end else begin
      data_out = shifted_right;
    end
  end

endmodule

