module barrel_shifter_8bit (
  input logic [8-1:0] data_in,
  input logic [3-1:0] shift_bits,
  input logic left_right,
  output logic [8-1:0] data_out
);

  always_comb begin
    if (left_right) begin
      data_out = 8'(data_in << 8'($unsigned(shift_bits)));
    end else begin
      data_out = 8'(data_in >> 8'($unsigned(shift_bits)));
    end
  end

endmodule

