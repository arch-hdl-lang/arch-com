module convolutional_encoder (
  input logic clk,
  input logic rst,
  input logic data_in,
  output logic encoded_bit1,
  output logic encoded_bit2
);

  // shift_reg[0] = most recent, shift_reg[1] = oldest
  logic [2-1:0] shift_reg;
  always_ff @(posedge clk or posedge rst) begin
    if (rst) begin
      encoded_bit1 <= 0;
      encoded_bit2 <= 0;
      shift_reg <= 0;
    end else begin
      // g1 = 111: data_in ^ shift_reg[0] ^ shift_reg[1]
      encoded_bit1 <= data_in ^ shift_reg[0] ^ shift_reg[1];
      // g2 = 101: data_in ^ shift_reg[1]
      encoded_bit2 <= data_in ^ shift_reg[1];
      // Shift: new data goes to [0], old [0] goes to [1]
      shift_reg <= {shift_reg[0], data_in};
    end
  end

endmodule

