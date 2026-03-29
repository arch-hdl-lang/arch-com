module decoder_64b66b (
  input logic clk_in,
  input logic rst_in,
  input logic [66-1:0] decoder_data_in,
  output logic [64-1:0] decoder_data_out,
  output logic sync_error
);

  logic [2-1:0] sync_header;
  assign sync_header = decoder_data_in[65:64];
  logic [64-1:0] data_in;
  assign data_in = decoder_data_in[63:0];
  always_ff @(posedge clk_in or posedge rst_in) begin
    if (rst_in) begin
      decoder_data_out <= 0;
      sync_error <= 1'b0;
    end else begin
      if (sync_header == 2'd1) begin
        decoder_data_out <= data_in;
        sync_error <= 1'b0;
      end else begin
        decoder_data_out <= 0;
        sync_error <= 1'b1;
      end
    end
  end

endmodule

