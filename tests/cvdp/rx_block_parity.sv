module rx_block_parity #(
  parameter int DATA_WIDTH = 8
) (
  input logic clk,
  input logic rst_n,
  input logic serial_in,
  input logic parity_in,
  input logic [3-1:0] sel,
  output logic [DATA_WIDTH-1:0] data_out,
  output logic parity_err,
  output logic valid
);

  logic [DATA_WIDTH-1:0] shift_reg;
  logic [8-1:0] bit_cnt;
  logic valid_reg;
  logic par_err_reg;
  logic par_acc;
  assign data_out = shift_reg;
  assign valid = valid_reg;
  assign parity_err = par_err_reg;
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      bit_cnt <= 0;
      par_acc <= 1'b0;
      par_err_reg <= 1'b0;
      shift_reg <= 0;
      valid_reg <= 1'b0;
    end else begin
      if (bit_cnt == 8'($unsigned(DATA_WIDTH))) begin
        valid_reg <= 1'b1;
        par_err_reg <= par_acc ^ parity_in;
        bit_cnt <= 0;
        par_acc <= 1'b0;
      end else begin
        shift_reg <= {serial_in, shift_reg[DATA_WIDTH - 1:1]};
        par_acc <= par_acc ^ serial_in;
        bit_cnt <= 8'(bit_cnt + 1);
        valid_reg <= 1'b0;
      end
    end
  end

endmodule

