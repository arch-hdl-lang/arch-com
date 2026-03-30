module sync_serial_communication_tx_rx (
  input logic clk,
  input logic reset_n,
  input logic [64-1:0] data_in,
  input logic [3-1:0] sel,
  output logic [64-1:0] data_out,
  output logic done
);

  logic serial_out_w;
  logic tx_done_w;
  logic serial_clk_w;
  tx_block tx (
    .clk(clk),
    .reset_n(reset_n),
    .data_in(data_in),
    .sel(sel),
    .serial_out(serial_out_w),
    .done(tx_done_w),
    .serial_clk(serial_clk_w)
  );
  rx_block rx (
    .clk(clk),
    .reset_n(reset_n),
    .data_in(serial_out_w),
    .sel(sel),
    .serial_clk(serial_clk_w),
    .data_out(data_out),
    .done(done)
  );

endmodule

