module convolutional_encoder (
  input logic clk,
  input logic rst,
  input logic data_in,
  output logic encoded_bit1,
  output logic encoded_bit2
);

  logic [1:0] sr;
  always_ff @(posedge clk) begin
    if (rst) begin
      encoded_bit1 <= 0;
      encoded_bit2 <= 0;
      sr <= 0;
    end else begin
      if (rst) begin
        sr <= 0;
        encoded_bit1 <= 0;
        encoded_bit2 <= 0;
      end else begin
        sr <= {data_in, sr[1]};
        encoded_bit1 <= data_in ^ sr[1] ^ sr[0];
        encoded_bit2 <= data_in ^ sr[0];
      end
    end
  end

endmodule

