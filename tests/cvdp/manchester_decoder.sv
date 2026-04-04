module manchester_decoder #(
  parameter int N = 8
) (
  input logic clk_in,
  input logic rst_in,
  input logic dec_valid_in,
  input logic [2 * N-1:0] dec_data_in,
  output logic dec_valid_out,
  output logic [N-1:0] dec_data_out
);

  logic [N-1:0] decoded;
  // Decode Manchester: pair bit[2i+1:2i]
  // "10" (=2) -> 1, "01" (=1) -> 0
  always_comb begin
    for (int i = 0; i <= N - 1; i++) begin
      if (dec_data_in[2 * i +: 2] == 2) begin
        decoded[i +: 1] = 1;
      end else begin
        decoded[i +: 1] = 0;
      end
    end
  end
  logic [N-1:0] data_reg;
  logic valid_reg;
  always_ff @(posedge clk_in or posedge rst_in) begin
    if (rst_in) begin
      data_reg <= 0;
      valid_reg <= 1'b0;
    end else begin
      if (dec_valid_in) begin
        data_reg <= decoded;
        valid_reg <= 1'b1;
      end else begin
        valid_reg <= 1'b0;
      end
    end
  end
  assign dec_data_out = data_reg;
  assign dec_valid_out = valid_reg;

endmodule

