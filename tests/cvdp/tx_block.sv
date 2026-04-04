module tx_block (
  input logic clk,
  input logic reset_n,
  input logic [64-1:0] data_in,
  input logic [3-1:0] sel,
  output logic serial_out,
  output logic done,
  output logic serial_clk
);

  logic [64-1:0] shift_reg;
  logic [7-1:0] bit_cnt;
  logic active;
  logic done_r;
  assign serial_clk = active;
  assign serial_out = active & shift_reg[0];
  assign done = done_r;
  always_ff @(posedge clk or negedge reset_n) begin
    if ((!reset_n)) begin
      active <= 1'b0;
      bit_cnt <= 0;
      done_r <= 1'b0;
      shift_reg <= 0;
    end else begin
      done_r <= 1'b0;
      if (active) begin
        shift_reg <= shift_reg >> 1;
        bit_cnt <= 7'(bit_cnt - 1);
        if (bit_cnt == 0) begin
          active <= 1'b0;
          done_r <= 1'b1;
        end
      end else if (sel == 1) begin
        shift_reg <= data_in;
        bit_cnt <= 7;
        active <= 1'b1;
      end else if (sel == 2) begin
        shift_reg <= data_in;
        bit_cnt <= 15;
        active <= 1'b1;
      end else if (sel == 3) begin
        shift_reg <= data_in;
        bit_cnt <= 31;
        active <= 1'b1;
      end else if (sel == 4) begin
        shift_reg <= data_in;
        bit_cnt <= 63;
        active <= 1'b1;
      end
    end
  end

endmodule

