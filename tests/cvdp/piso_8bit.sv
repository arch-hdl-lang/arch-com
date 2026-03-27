module piso_8bit (
  input logic clk,
  input logic rst,
  output logic serial_out
);

  logic [8-1:0] tmp;
  logic [3-1:0] bit_cnt;
  always_ff @(posedge clk or negedge rst) begin
    if ((!rst)) begin
      bit_cnt <= 0;
      serial_out <= 0;
      tmp <= 1;
    end else begin
      serial_out <= tmp[7 - bit_cnt];
      if (bit_cnt == 3'd7) begin
        bit_cnt <= 3'd0;
        tmp <= 8'(tmp + 8'd1);
      end else begin
        bit_cnt <= 3'(bit_cnt + 3'd1);
      end
    end
  end

endmodule

// Output MSB-first: bit (7 - bit_cnt)
