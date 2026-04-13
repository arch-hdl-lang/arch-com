module piso_8bit (
  input logic clk,
  input logic rst,
  output logic serial_out
);

  logic [2:0] bit_cnt;
  logic [7:0] current_byte;
  logic serial_out_r;
  logic [10:0] shifted;
  assign shifted = 11'($unsigned(current_byte)) << bit_cnt;
  assign serial_out = serial_out_r;
  always_ff @(posedge clk or negedge rst) begin
    if ((!rst)) begin
      bit_cnt <= 0;
      current_byte <= 1;
      serial_out_r <= 0;
    end else begin
      serial_out_r <= shifted[7:7];
      if (bit_cnt == 7) begin
        bit_cnt <= 0;
        current_byte <= 8'(current_byte + 8'd1);
      end else begin
        bit_cnt <= 3'(bit_cnt + 3'd1);
      end
    end
  end

endmodule

