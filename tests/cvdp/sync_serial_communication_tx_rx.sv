module tx_block (
  input logic clk,
  input logic reset_n,
  input logic [63:0] data_in,
  input logic [2:0] sel,
  output logic serial_out,
  output logic done,
  output logic serial_clk
);

  logic [63:0] shift_reg;
  logic [6:0] bit_cnt;
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

module sync_serial_communication_tx_rx (
  input logic clk,
  input logic reset_n,
  input logic [63:0] data_in,
  input logic [2:0] sel,
  output logic [63:0] data_out,
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

