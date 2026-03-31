module sync_serial_communication_top #(
  parameter int DATA_WIDTH = 8
) (
  input logic clk,
  input logic rst_n,
  input logic [DATA_WIDTH-1:0] data_in,
  input logic [3-1:0] sel,
  input logic load,
  input logic parity_in,
  output logic [DATA_WIDTH-1:0] data_out,
  output logic parity_err,
  output logic valid,
  output logic parity_out
);

  logic serial_link;
  logic tx_parity;
  logic [DATA_WIDTH-1:0] rx_data;
  logic rx_parity_err;
  logic rx_valid;
  tx_block_parity #(.DATA_WIDTH(DATA_WIDTH)) tx (
    .clk(clk),
    .rst_n(rst_n),
    .data_in(data_in),
    .sel(sel),
    .load(load),
    .serial_out(serial_link),
    .parity(tx_parity)
  );
  rx_block_parity #(.DATA_WIDTH(DATA_WIDTH)) rx (
    .clk(clk),
    .rst_n(rst_n),
    .serial_in(serial_link),
    .parity_in(tx_parity),
    .sel(sel),
    .data_out(rx_data),
    .parity_err(rx_parity_err),
    .valid(rx_valid)
  );
  assign data_out = rx_data;
  assign parity_err = rx_parity_err;
  assign valid = rx_valid;
  assign parity_out = tx_parity;

endmodule

