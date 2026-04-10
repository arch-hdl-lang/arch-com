module sipo_top #(
  parameter int DATA_WIDTH = 16,
  parameter int SHIFT_DIRECTION = 1,
  parameter int CODE_WIDTH = DATA_WIDTH + $clog2(DATA_WIDTH + 1)
) (
  input logic clk,
  input logic reset_n,
  input logic serial_in,
  input logic shift_en,
  input logic [CODE_WIDTH-1:0] received,
  output logic done,
  output logic [DATA_WIDTH-1:0] data_out,
  output logic [CODE_WIDTH-1:0] encoded,
  output logic error_detected,
  output logic error_corrected
);

  logic sipo_done;
  logic [DATA_WIDTH-1:0] sipo_pout;
  logic [DATA_WIDTH-1:0] ecc_data_out;
  logic [CODE_WIDTH-1:0] ecc_encoded;
  logic ecc_err_det;
  logic ecc_err_cor;
  serial_in_parallel_out_8bit #(.WIDTH(DATA_WIDTH), .SHIFT_DIRECTION(SHIFT_DIRECTION)) uut_sipo (
    .clk(clk),
    .rst(reset_n),
    .sin(serial_in),
    .shift_en(shift_en),
    .done(sipo_done),
    .parallel_out(sipo_pout)
  );
  onebit_ecc #(.DATA_WIDTH(DATA_WIDTH), .CODE_WIDTH(CODE_WIDTH)) uut_ecc (
    .data_in(sipo_pout),
    .received(received),
    .data_out(ecc_data_out),
    .encoded(ecc_encoded),
    .error_detected(ecc_err_det),
    .error_corrected(ecc_err_cor)
  );
  assign done = sipo_done;
  assign data_out = ecc_data_out;
  assign encoded = ecc_encoded;
  assign error_detected = ecc_err_det;
  assign error_corrected = ecc_err_cor;

endmodule

