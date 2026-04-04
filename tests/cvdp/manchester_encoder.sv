module manchester_encoder #(
  parameter int N = 8
) (
  input logic clk_in,
  input logic rst_in,
  input logic enc_valid_in,
  input logic [N-1:0] enc_data_in,
  output logic enc_valid_out,
  output logic [2 * N-1:0] enc_data_out
);

  logic [2 * N-1:0] data_reg;
  logic valid_reg;
  logic [2 * N-1:0] encoded;
  // Combinational Manchester encoding: '1' -> "10", '0' -> "01"
  always_comb begin
    for (int i = 0; i <= N - 1; i++) begin
      if (enc_data_in[i +: 1] == 1) begin
        encoded[2 * i +: 2] = 2;
      end else begin
        encoded[2 * i +: 2] = 1;
      end
    end
  end
  always_ff @(posedge clk_in or posedge rst_in) begin
    if (rst_in) begin
      data_reg <= 0;
      valid_reg <= 1'b0;
    end else begin
      if (enc_valid_in) begin
        data_reg <= encoded;
        valid_reg <= 1'b1;
      end else begin
        data_reg <= (2 * N)'($unsigned(0));
        valid_reg <= 1'b0;
      end
    end
  end
  assign enc_data_out = data_reg;
  assign enc_valid_out = valid_reg;

endmodule

