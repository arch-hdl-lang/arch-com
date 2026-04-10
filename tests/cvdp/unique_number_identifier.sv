module unique_number_identifier #(
  parameter int p_bit_width = 8,
  parameter int p_max_numbers = 16
) (
  input logic i_clk,
  input logic i_rst_n,
  input logic i_ready,
  input logic [p_bit_width-1:0] i_number,
  output logic [p_bit_width-1:0] o_unique_number
);

  logic [p_bit_width-1:0] xor_acc;
  always_ff @(posedge i_clk or negedge i_rst_n) begin
    if ((!i_rst_n)) begin
      xor_acc <= 0;
    end else begin
      if (i_ready) begin
        xor_acc <= xor_acc ^ i_number;
      end
    end
  end
  assign o_unique_number = xor_acc;

endmodule

