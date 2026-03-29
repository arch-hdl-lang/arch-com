module binary_to_one_hot_decoder_sequential #(
  parameter int BINARY_WIDTH = 5,
  parameter int OUTPUT_WIDTH = 32
) (
  input logic i_clk,
  input logic i_rstb,
  input logic [BINARY_WIDTH-1:0] i_binary_in,
  output logic [OUTPUT_WIDTH-1:0] o_one_hot_out
);

  always_ff @(posedge i_clk or negedge i_rstb) begin
    if ((!i_rstb)) begin
      o_one_hot_out <= 0;
    end else begin
      o_one_hot_out <= 32'd1 << 32'($unsigned(i_binary_in));
    end
  end

endmodule

