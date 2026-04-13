module binary_to_one_hot_decoder_sequential #(
  parameter int BINARY_WIDTH = 4,
  parameter int OUTPUT_WIDTH = 16
) (
  input logic i_clk,
  input logic i_rstb,
  input logic [BINARY_WIDTH-1:0] i_binary_in,
  output logic [OUTPUT_WIDTH-1:0] o_one_hot_out
);

  logic [OUTPUT_WIDTH-1:0] one_hot;
  assign o_one_hot_out = one_hot;
  always_ff @(posedge i_clk or negedge i_rstb) begin
    if ((!i_rstb)) begin
      one_hot <= 0;
    end else begin
      one_hot <= OUTPUT_WIDTH'($unsigned(1)) << i_binary_in;
    end
  end

endmodule

