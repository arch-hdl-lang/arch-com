module top_manchester #(
  parameter int N = 8
) (
  input logic clk_in,
  input logic rst_in,
  input logic enc_valid_in,
  input logic [N-1:0] enc_data_in,
  output logic enc_valid_out,
  output logic [2 * N-1:0] enc_data_out,
  input logic dec_valid_in,
  input logic [2 * N-1:0] dec_data_in,
  output logic dec_valid_out,
  output logic [N-1:0] dec_data_out
);

  logic enc_vo;
  logic [2 * N-1:0] enc_do;
  logic dec_vo;
  logic [N-1:0] dec_do;
  manchester_encoder #(.N(N)) enc (
    .clk_in(clk_in),
    .rst_in(rst_in),
    .enc_valid_in(enc_valid_in),
    .enc_data_in(enc_data_in),
    .enc_valid_out(enc_vo),
    .enc_data_out(enc_do)
  );
  manchester_decoder #(.N(N)) dec (
    .clk_in(clk_in),
    .rst_in(rst_in),
    .dec_valid_in(dec_valid_in),
    .dec_data_in(dec_data_in),
    .dec_valid_out(dec_vo),
    .dec_data_out(dec_do)
  );
  assign enc_valid_out = enc_vo;
  assign enc_data_out = enc_do;
  assign dec_valid_out = dec_vo;
  assign dec_data_out = dec_do;

endmodule

