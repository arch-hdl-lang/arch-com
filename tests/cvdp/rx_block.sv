module rx_block (
  input logic clk,
  input logic reset_n,
  input logic serial_clk,
  input logic [2:0] sel,
  input logic data_in,
  output logic [63:0] data_out,
  output logic done
);

  logic [63:0] shift_reg;
  logic [6:0] bit_cnt;
  logic active;
  logic [63:0] data_out_r;
  logic done_r;
  assign data_out = data_out_r;
  assign done = done_r;
  always_ff @(posedge clk or negedge reset_n) begin
    if ((!reset_n)) begin
      active <= 1'b0;
      bit_cnt <= 0;
      data_out_r <= 0;
      done_r <= 1'b0;
      shift_reg <= 0;
    end else begin
      done_r <= 1'b0;
      if (active) begin
        if (serial_clk) begin
          shift_reg <= {data_in, shift_reg[63:1]};
          bit_cnt <= 7'(bit_cnt - 1);
          if (bit_cnt == 0) begin
            active <= 1'b0;
            done_r <= 1'b1;
            if (sel == 1) begin
              data_out_r <= {56'd0, data_in, shift_reg[63:57]};
            end else if (sel == 2) begin
              data_out_r <= {48'd0, data_in, shift_reg[63:49]};
            end else if (sel == 3) begin
              data_out_r <= {32'd0, data_in, shift_reg[63:33]};
            end else if (sel == 4) begin
              data_out_r <= {data_in, shift_reg[63:1]};
            end else begin
              data_out_r <= 0;
            end
          end
        end
      end else if (serial_clk) begin
        if (sel == 1) begin
          shift_reg <= {data_in, shift_reg[63:1]};
          bit_cnt <= 6;
          active <= 1'b1;
        end else if (sel == 2) begin
          shift_reg <= {data_in, shift_reg[63:1]};
          bit_cnt <= 14;
          active <= 1'b1;
        end else if (sel == 3) begin
          shift_reg <= {data_in, shift_reg[63:1]};
          bit_cnt <= 30;
          active <= 1'b1;
        end else if (sel == 4) begin
          shift_reg <= {data_in, shift_reg[63:1]};
          bit_cnt <= 62;
          active <= 1'b1;
        end
      end
    end
  end

endmodule

