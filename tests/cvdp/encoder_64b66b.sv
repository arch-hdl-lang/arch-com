module encoder_64b66b (
  input logic clk_in,
  input logic rst_in,
  input logic [64-1:0] encoder_data_in,
  input logic [8-1:0] encoder_control_in,
  output logic [66-1:0] encoder_data_out
);

  always_ff @(posedge clk_in or posedge rst_in) begin
    if (rst_in) begin
      encoder_data_out <= 0;
    end else begin
      if (encoder_control_in == 8'd0) begin
        encoder_data_out <= {2'd1, encoder_data_in};
      end else begin
        encoder_data_out <= {2'd2, 64'd0};
      end
    end
  end

endmodule

